use std::{
    collections::HashMap,
    fmt,
    io::{Cursor, ErrorKind},
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, bail, Context, Result};
use cloud_control::{CloudControlStore, EdgeNode, ReleaseService, ReleaseStatus, SqliteCloudStore};
use edge_core::{
    decode_edgelink_frame, encode_edgelink_frame, DiscoveryReport, DiscoveryRequest,
    EdgeConfigPackage, EdgeLinkConfigReport, EdgeLinkMessage, EdgeLinkPayload,
    EDGELINK_MAX_FRAME_BYTES,
};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Mutex as AsyncMutex};
use tokio::time::{timeout, Duration};
use tokio_rustls::{
    rustls::{
        self,
        pki_types::{CertificateDer, PrivateKeyDer},
        server::WebPkiClientVerifier,
        RootCertStore,
    },
    TlsAcceptor,
};
use tracing::warn;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeGatewaySession {
    pub edge_id: String,
    pub runtime_id: String,
    pub runtime_version: String,
    pub capabilities: Vec<String>,
    pub peer_addr: SocketAddr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeGatewaySessionReport {
    pub session: EdgeGatewaySession,
    pub accepted_message_count: usize,
    pub config_report_count: usize,
}

#[derive(Clone, Default)]
pub struct EdgeGatewayCommandRegistry {
    sessions: Arc<AsyncMutex<HashMap<String, RegisteredEdgeSession>>>,
}

#[derive(Clone)]
struct RegisteredEdgeSession {
    session_id: uuid::Uuid,
    sender: mpsc::Sender<EdgeGatewayCommand>,
}

enum EdgeGatewayCommand {
    Discovery {
        request: DiscoveryRequest,
        response: oneshot::Sender<Result<DiscoveryReport, String>>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EdgeGatewayDispatchError {
    Offline,
    Busy,
    Timeout,
    Failed(String),
}

impl fmt::Display for EdgeGatewayDispatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Offline => formatter.write_str("edge runtime is not connected"),
            Self::Busy => formatter.write_str("edge runtime command queue is busy"),
            Self::Timeout => formatter.write_str("edge runtime command timed out"),
            Self::Failed(reason) => write!(formatter, "edge runtime command failed: {reason}"),
        }
    }
}

impl std::error::Error for EdgeGatewayDispatchError {}

impl EdgeGatewayCommandRegistry {
    pub async fn is_online(&self, edge_id: &str) -> bool {
        self.sessions.lock().await.contains_key(edge_id)
    }

    pub async fn dispatch_discovery(
        &self,
        edge_id: &str,
        request: DiscoveryRequest,
        wait: Duration,
    ) -> Result<DiscoveryReport, EdgeGatewayDispatchError> {
        let sender = self
            .sessions
            .lock()
            .await
            .get(edge_id)
            .map(|session| session.sender.clone())
            .ok_or(EdgeGatewayDispatchError::Offline)?;
        let (response_tx, response_rx) = oneshot::channel();
        sender
            .try_send(EdgeGatewayCommand::Discovery {
                request,
                response: response_tx,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => EdgeGatewayDispatchError::Busy,
                mpsc::error::TrySendError::Closed(_) => EdgeGatewayDispatchError::Offline,
            })?;

        match timeout(wait, response_rx).await {
            Err(_) => Err(EdgeGatewayDispatchError::Timeout),
            Ok(Err(_)) => Err(EdgeGatewayDispatchError::Offline),
            Ok(Ok(Err(reason))) => Err(EdgeGatewayDispatchError::Failed(reason)),
            Ok(Ok(Ok(report))) => Ok(report),
        }
    }

    async fn register(
        &self,
        edge_id: &str,
        sender: mpsc::Sender<EdgeGatewayCommand>,
    ) -> uuid::Uuid {
        let session_id = uuid::Uuid::new_v4();
        self.sessions.lock().await.insert(
            edge_id.to_string(),
            RegisteredEdgeSession { session_id, sender },
        );
        session_id
    }

    async fn unregister(&self, edge_id: &str, session_id: uuid::Uuid) {
        let mut sessions = self.sessions.lock().await;
        if sessions
            .get(edge_id)
            .is_some_and(|session| session.session_id == session_id)
        {
            sessions.remove(edge_id);
        }
    }
}

#[derive(Clone)]
pub struct EdgeGatewayTlsConfig {
    acceptor: TlsAcceptor,
}

impl EdgeGatewayTlsConfig {
    pub fn from_pem(
        server_cert_pem: &str,
        server_key_pem: &str,
        client_ca_cert_pem: &str,
    ) -> Result<Self> {
        let server_certs = load_certs(server_cert_pem, "server certificate")?;
        let server_key = load_private_key(server_key_pem, "server private key")?;
        let client_ca_certs = load_certs(client_ca_cert_pem, "client CA certificate")?;

        let mut client_roots = RootCertStore::empty();
        for cert in client_ca_certs {
            client_roots
                .add(cert)
                .context("invalid client CA certificate")?;
        }

        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let client_verifier =
            WebPkiClientVerifier::builder_with_provider(Arc::new(client_roots), provider.clone())
                .build()
                .context("failed to build client certificate verifier")?;
        let config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .context("failed to select EdgeLink TLS protocol versions")?
            .with_client_cert_verifier(client_verifier)
            .with_single_cert(server_certs, server_key)
            .context("failed to build EdgeLink server TLS config")?;

        Ok(Self {
            acceptor: TlsAcceptor::from(Arc::new(config)),
        })
    }

    pub fn acceptor(&self) -> TlsAcceptor {
        self.acceptor.clone()
    }
}

pub async fn handle_edgelink_session(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
) -> Result<EdgeGatewaySession> {
    handle_edgelink_stream(&mut stream, peer_addr, None).await
}

pub async fn serve_edgelink_gateway(
    listener: TcpListener,
    store: Arc<Mutex<CloudControlStore>>,
) -> Result<()> {
    loop {
        let (stream, peer_addr) = listener
            .accept()
            .await
            .context("failed to accept EdgeLink runtime connection")?;
        let session_store = store.clone();
        tokio::spawn(async move {
            if let Err(error) =
                handle_edgelink_session_with_store(stream, peer_addr, session_store).await
            {
                warn!(%peer_addr, error = %error, "EdgeLink runtime session failed");
            }
        });
    }
}

pub async fn serve_edgelink_gateway_with_sqlite(
    listener: TcpListener,
    store: Arc<Mutex<CloudControlStore>>,
    sqlite_store: SqliteCloudStore,
) -> Result<()> {
    loop {
        let (stream, peer_addr) = listener
            .accept()
            .await
            .context("failed to accept EdgeLink runtime connection")?;
        let session_store = store.clone();
        let session_sqlite_store = sqlite_store.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_edgelink_session_with_store_and_sqlite(
                stream,
                peer_addr,
                session_store,
                session_sqlite_store,
            )
            .await
            {
                warn!(%peer_addr, error = %error, "EdgeLink runtime session failed");
            }
        });
    }
}

pub async fn serve_edgelink_gateway_with_registry_and_sqlite(
    listener: TcpListener,
    store: Arc<Mutex<CloudControlStore>>,
    registry: EdgeGatewayCommandRegistry,
    sqlite_store: SqliteCloudStore,
) -> Result<()> {
    loop {
        let (stream, peer_addr) = listener
            .accept()
            .await
            .context("failed to accept EdgeLink runtime connection")?;
        let session_store = store.clone();
        let session_registry = registry.clone();
        let session_sqlite_store = sqlite_store.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_edgelink_session_with_registry_and_sqlite(
                stream,
                peer_addr,
                session_store,
                session_registry,
                session_sqlite_store,
            )
            .await
            {
                warn!(%peer_addr, error = %error, "EdgeLink runtime session failed");
            }
        });
    }
}

pub async fn serve_edgelink_gateway_with_registry(
    listener: TcpListener,
    store: Arc<Mutex<CloudControlStore>>,
    registry: EdgeGatewayCommandRegistry,
) -> Result<()> {
    loop {
        let (stream, peer_addr) = listener
            .accept()
            .await
            .context("failed to accept EdgeLink runtime connection")?;
        let session_store = store.clone();
        let session_registry = registry.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_edgelink_session_with_registry(
                stream,
                peer_addr,
                session_store,
                session_registry,
            )
            .await
            {
                warn!(%peer_addr, error = %error, "EdgeLink runtime session failed");
            }
        });
    }
}

pub async fn serve_edgelink_tls_gateway(
    listener: TcpListener,
    tls_config: EdgeGatewayTlsConfig,
    store: Arc<Mutex<CloudControlStore>>,
) -> Result<()> {
    loop {
        let (stream, peer_addr) = listener
            .accept()
            .await
            .context("failed to accept EdgeLink TLS runtime connection")?;
        let session_store = store.clone();
        let session_tls_config = tls_config.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_edgelink_tls_session_with_store(
                stream,
                peer_addr,
                &session_tls_config,
                session_store,
            )
            .await
            {
                warn!(%peer_addr, error = %error, "EdgeLink TLS runtime session failed");
            }
        });
    }
}

pub async fn serve_edgelink_tls_gateway_with_sqlite(
    listener: TcpListener,
    tls_config: EdgeGatewayTlsConfig,
    store: Arc<Mutex<CloudControlStore>>,
    sqlite_store: SqliteCloudStore,
) -> Result<()> {
    loop {
        let (stream, peer_addr) = listener
            .accept()
            .await
            .context("failed to accept EdgeLink TLS runtime connection")?;
        let session_store = store.clone();
        let session_sqlite_store = sqlite_store.clone();
        let session_tls_config = tls_config.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_edgelink_tls_session_with_store_and_sqlite(
                stream,
                peer_addr,
                &session_tls_config,
                session_store,
                session_sqlite_store,
            )
            .await
            {
                warn!(%peer_addr, error = %error, "EdgeLink TLS runtime session failed");
            }
        });
    }
}

pub async fn serve_edgelink_tls_gateway_with_registry_and_sqlite(
    listener: TcpListener,
    tls_config: EdgeGatewayTlsConfig,
    store: Arc<Mutex<CloudControlStore>>,
    registry: EdgeGatewayCommandRegistry,
    sqlite_store: SqliteCloudStore,
) -> Result<()> {
    loop {
        let (stream, peer_addr) = listener
            .accept()
            .await
            .context("failed to accept EdgeLink TLS runtime connection")?;
        let session_store = store.clone();
        let session_registry = registry.clone();
        let session_sqlite_store = sqlite_store.clone();
        let session_tls_config = tls_config.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_edgelink_tls_session_with_registry_and_sqlite(
                stream,
                peer_addr,
                &session_tls_config,
                session_store,
                session_registry,
                session_sqlite_store,
            )
            .await
            {
                warn!(%peer_addr, error = %error, "EdgeLink TLS runtime session failed");
            }
        });
    }
}

pub async fn serve_edgelink_tls_gateway_with_registry(
    listener: TcpListener,
    tls_config: EdgeGatewayTlsConfig,
    store: Arc<Mutex<CloudControlStore>>,
    registry: EdgeGatewayCommandRegistry,
) -> Result<()> {
    loop {
        let (stream, peer_addr) = listener
            .accept()
            .await
            .context("failed to accept EdgeLink TLS runtime connection")?;
        let session_store = store.clone();
        let session_registry = registry.clone();
        let session_tls_config = tls_config.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_edgelink_tls_session_with_registry(
                stream,
                peer_addr,
                &session_tls_config,
                session_store,
                session_registry,
            )
            .await
            {
                warn!(%peer_addr, error = %error, "EdgeLink TLS runtime session failed");
            }
        });
    }
}

pub async fn serve_edgelink_gateway_for_sessions(
    listener: TcpListener,
    store: Arc<Mutex<CloudControlStore>>,
    session_count: usize,
) -> Result<usize> {
    let mut accepted_sessions = 0;
    for _ in 0..session_count {
        let (stream, peer_addr) = listener
            .accept()
            .await
            .context("failed to accept EdgeLink runtime connection")?;
        handle_edgelink_session_with_store(stream, peer_addr, store.clone()).await?;
        accepted_sessions += 1;
    }
    Ok(accepted_sessions)
}

pub async fn handle_edgelink_session_with_store(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    store: Arc<Mutex<CloudControlStore>>,
) -> Result<EdgeGatewaySessionReport> {
    handle_edgelink_session_with_optional_sqlite(&mut stream, peer_addr, store, None).await
}

pub async fn handle_edgelink_session_with_store_and_sqlite(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    store: Arc<Mutex<CloudControlStore>>,
    sqlite_store: SqliteCloudStore,
) -> Result<EdgeGatewaySessionReport> {
    handle_edgelink_session_with_optional_sqlite(&mut stream, peer_addr, store, Some(sqlite_store))
        .await
}

pub async fn handle_edgelink_session_with_registry(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    store: Arc<Mutex<CloudControlStore>>,
    registry: EdgeGatewayCommandRegistry,
) -> Result<EdgeGatewaySessionReport> {
    handle_edgelink_session_with_optional_registry(
        &mut stream,
        peer_addr,
        store,
        None,
        Some(registry),
    )
    .await
}

pub async fn handle_edgelink_session_with_registry_and_sqlite(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    store: Arc<Mutex<CloudControlStore>>,
    registry: EdgeGatewayCommandRegistry,
    sqlite_store: SqliteCloudStore,
) -> Result<EdgeGatewaySessionReport> {
    handle_edgelink_session_with_optional_registry(
        &mut stream,
        peer_addr,
        store,
        Some(sqlite_store),
        Some(registry),
    )
    .await
}

async fn handle_edgelink_session_with_optional_sqlite<S>(
    stream: &mut S,
    peer_addr: SocketAddr,
    store: Arc<Mutex<CloudControlStore>>,
    sqlite_store: Option<SqliteCloudStore>,
) -> Result<EdgeGatewaySessionReport>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    handle_edgelink_session_with_optional_registry(stream, peer_addr, store, sqlite_store, None)
        .await
}

async fn handle_edgelink_session_with_optional_registry<S>(
    stream: &mut S,
    peer_addr: SocketAddr,
    store: Arc<Mutex<CloudControlStore>>,
    sqlite_store: Option<SqliteCloudStore>,
    registry: Option<EdgeGatewayCommandRegistry>,
) -> Result<EdgeGatewaySessionReport>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let session = handle_edgelink_stream(stream, peer_addr, Some(&store)).await?;
    persist_runtime_discovered_edge(&session, store.clone(), sqlite_store.as_ref()).await?;
    let config_report_count =
        deploy_pending_config_if_available(stream, &session, store.clone(), sqlite_store.as_ref())
            .await?;
    let accepted_message_count = if let Some(registry) = registry {
        handle_edgelink_runtime_messages_with_registry(
            stream,
            &session,
            store,
            sqlite_store,
            registry,
        )
        .await?
    } else {
        handle_edgelink_runtime_messages(stream, &session, store, sqlite_store).await?
    };

    Ok(EdgeGatewaySessionReport {
        session,
        accepted_message_count,
        config_report_count,
    })
}

pub async fn handle_edgelink_tls_session(
    stream: TcpStream,
    peer_addr: SocketAddr,
    tls_config: &EdgeGatewayTlsConfig,
) -> Result<EdgeGatewaySession> {
    let mut stream = tls_config
        .acceptor()
        .accept(stream)
        .await
        .context("failed to accept EdgeLink TLS session")?;
    handle_edgelink_stream(&mut stream, peer_addr, None).await
}

pub async fn handle_edgelink_tls_session_with_store(
    stream: TcpStream,
    peer_addr: SocketAddr,
    tls_config: &EdgeGatewayTlsConfig,
    store: Arc<Mutex<CloudControlStore>>,
) -> Result<EdgeGatewaySessionReport> {
    let mut stream = tls_config
        .acceptor()
        .accept(stream)
        .await
        .context("failed to accept EdgeLink TLS session")?;
    let session = handle_edgelink_stream(&mut stream, peer_addr, Some(&store)).await?;
    persist_runtime_discovered_edge(&session, store.clone(), None).await?;
    let config_report_count =
        deploy_pending_config_if_available(&mut stream, &session, store.clone(), None).await?;
    let accepted_message_count =
        handle_edgelink_runtime_messages(&mut stream, &session, store, None).await?;

    Ok(EdgeGatewaySessionReport {
        session,
        accepted_message_count,
        config_report_count,
    })
}

pub async fn handle_edgelink_tls_session_with_store_and_sqlite(
    stream: TcpStream,
    peer_addr: SocketAddr,
    tls_config: &EdgeGatewayTlsConfig,
    store: Arc<Mutex<CloudControlStore>>,
    sqlite_store: SqliteCloudStore,
) -> Result<EdgeGatewaySessionReport> {
    let mut stream = tls_config
        .acceptor()
        .accept(stream)
        .await
        .context("failed to accept EdgeLink TLS session")?;
    let session = handle_edgelink_stream(&mut stream, peer_addr, Some(&store)).await?;
    persist_runtime_discovered_edge(&session, store.clone(), Some(&sqlite_store)).await?;
    let config_report_count = deploy_pending_config_if_available(
        &mut stream,
        &session,
        store.clone(),
        Some(&sqlite_store),
    )
    .await?;
    let accepted_message_count =
        handle_edgelink_runtime_messages(&mut stream, &session, store, Some(sqlite_store)).await?;

    Ok(EdgeGatewaySessionReport {
        session,
        accepted_message_count,
        config_report_count,
    })
}

pub async fn handle_edgelink_tls_session_with_registry(
    stream: TcpStream,
    peer_addr: SocketAddr,
    tls_config: &EdgeGatewayTlsConfig,
    store: Arc<Mutex<CloudControlStore>>,
    registry: EdgeGatewayCommandRegistry,
) -> Result<EdgeGatewaySessionReport> {
    let mut stream = tls_config
        .acceptor()
        .accept(stream)
        .await
        .context("failed to accept EdgeLink TLS session")?;
    handle_edgelink_session_with_optional_registry(
        &mut stream,
        peer_addr,
        store,
        None,
        Some(registry),
    )
    .await
}

pub async fn handle_edgelink_tls_session_with_registry_and_sqlite(
    stream: TcpStream,
    peer_addr: SocketAddr,
    tls_config: &EdgeGatewayTlsConfig,
    store: Arc<Mutex<CloudControlStore>>,
    registry: EdgeGatewayCommandRegistry,
    sqlite_store: SqliteCloudStore,
) -> Result<EdgeGatewaySessionReport> {
    let mut stream = tls_config
        .acceptor()
        .accept(stream)
        .await
        .context("failed to accept EdgeLink TLS session")?;
    handle_edgelink_session_with_optional_registry(
        &mut stream,
        peer_addr,
        store,
        Some(sqlite_store),
        Some(registry),
    )
    .await
}

async fn handle_edgelink_stream<S>(
    stream: &mut S,
    peer_addr: SocketAddr,
    auth_store: Option<&Arc<Mutex<CloudControlStore>>>,
) -> Result<EdgeGatewaySession>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let message = read_edgelink_message(stream)
        .await
        .context("failed to read EdgeLink hello")?;

    let EdgeLinkPayload::Hello(hello) = &message.payload else {
        bail!("first EdgeLink message must be hello");
    };

    let Some(runtime_id) = message.runtime_id.as_deref() else {
        bail!("EdgeLink hello is missing runtime_id");
    };
    if runtime_id != hello.runtime_id {
        bail!("EdgeLink hello runtime_id does not match envelope runtime_id");
    }

    if let Some(store) = auth_store {
        let expected_hash = {
            let store = store.lock().expect("store mutex poisoned");
            store
                .active_edge_credential(&message.edge_id)
                .map(|credential| credential.token_hash.clone())
        };
        if let Some(expected_hash) = expected_hash {
            let accepted = hello
                .access_token
                .as_deref()
                .map(hash_access_token)
                .is_some_and(|actual_hash| actual_hash == expected_hash);
            if !accepted {
                let reason = "invalid or missing edge access token";
                let nack = EdgeLinkMessage::nack(
                    message.edge_id.clone(),
                    message.runtime_id.clone(),
                    message.message_id,
                    message.sequence,
                    reason,
                );
                write_edgelink_message(stream, &nack)
                    .await
                    .context("failed to write EdgeLink authentication rejection")?;
                bail!(reason);
            }
        }
    }

    let ack = EdgeLinkMessage::ack(
        message.edge_id.clone(),
        hello.runtime_id.clone(),
        message.message_id,
        message.sequence,
    );
    write_edgelink_message(stream, &ack)
        .await
        .context("failed to write EdgeLink hello ack")?;

    Ok(EdgeGatewaySession {
        edge_id: message.edge_id,
        runtime_id: hello.runtime_id.clone(),
        runtime_version: hello.runtime_version.clone(),
        capabilities: hello.capabilities.clone(),
        peer_addr,
    })
}

fn hash_access_token(access_token: &str) -> String {
    Sha256::digest(access_token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn persist_runtime_discovered_edge(
    session: &EdgeGatewaySession,
    store: Arc<Mutex<CloudControlStore>>,
    sqlite_store: Option<&SqliteCloudStore>,
) -> Result<()> {
    let node = {
        let mut store = store.lock().expect("store mutex poisoned");
        let existing = store
            .edge_nodes()
            .find(|edge| edge.edge_id == session.edge_id)
            .cloned();
        let mut node = existing.unwrap_or_else(|| {
            EdgeNode::new(session.edge_id.clone(), session.edge_id.clone())
                .at_site(format!("runtime/{}", session.runtime_id))
        });
        if matches!(
            node.display_name.as_str(),
            "" | "新边端注册草稿" | "新边端待确认"
        ) {
            node.display_name = session.edge_id.clone();
        }
        if node.site.as_deref().is_none() || node.site.as_deref() == Some("待分配") {
            node.site = Some(format!("runtime/{}", session.runtime_id));
        }
        node.capabilities
            .retain(|capability| capability != "registration:draft");
        push_unique(
            &mut node.capabilities,
            "registration:runtime-discovered".to_string(),
        );
        push_unique(
            &mut node.capabilities,
            format!("runtime-version:{}", session.runtime_version),
        );
        for capability in &session.capabilities {
            push_unique(&mut node.capabilities, capability.clone());
        }
        store.register_edge(node.clone());
        node
    };

    if let Some(sqlite_store) = sqlite_store {
        sqlite_store
            .upsert_edge_node(node)
            .await
            .context("persist runtime-discovered edge node")?;
    }

    Ok(())
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

async fn handle_edgelink_runtime_messages<S>(
    stream: &mut S,
    session: &EdgeGatewaySession,
    store: Arc<Mutex<CloudControlStore>>,
    sqlite_store: Option<SqliteCloudStore>,
) -> Result<usize>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut accepted = 0;
    while let Some(message) = read_optional_edgelink_message(stream).await? {
        let response =
            persist_runtime_message(session, message, store.clone(), sqlite_store.as_ref()).await?;
        let acked = matches!(&response.payload, EdgeLinkPayload::Ack(ack) if ack.accepted);
        write_edgelink_message(stream, &response)
            .await
            .context("failed to write EdgeLink runtime message ack")?;
        if acked {
            accepted += 1;
        }
    }

    Ok(accepted)
}

struct PendingDiscovery {
    request_message_id: uuid::Uuid,
    protocol_connection_id: String,
    response: oneshot::Sender<Result<DiscoveryReport, String>>,
}

async fn handle_edgelink_runtime_messages_with_registry<S>(
    stream: &mut S,
    session: &EdgeGatewaySession,
    store: Arc<Mutex<CloudControlStore>>,
    sqlite_store: Option<SqliteCloudStore>,
    registry: EdgeGatewayCommandRegistry,
) -> Result<usize>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (sender, mut commands) = mpsc::channel(1);
    let session_id = registry.register(&session.edge_id, sender).await;
    let (mut reader, mut writer) = tokio::io::split(stream);
    let mut pending = HashMap::<String, PendingDiscovery>::new();
    let mut accepted = 0;
    let mut sequence = 10_000_u64;

    let result = async {
        loop {
            tokio::select! {
                incoming = read_optional_edgelink_message(&mut reader) => {
                    let Some(message) = incoming? else {
                        break;
                    };
                    if let EdgeLinkPayload::Nack(nack) = &message.payload {
                        let pending_job = pending.iter().find_map(|(job_id, request)| {
                            (request.request_message_id == nack.ack_message_id)
                                .then(|| job_id.clone())
                        });
                        if let Some(job_id) = pending_job {
                            if let Some(request) = pending.remove(&job_id) {
                                let reason = nack
                                    .reason
                                    .clone()
                                    .unwrap_or_else(|| "runtime rejected discovery request".to_string());
                                let _ = request.response.send(Err(reason));
                            }
                            continue;
                        }
                    }
                    if let EdgeLinkPayload::DiscoveryReport(report) = &message.payload {
                        let Some(expected) = pending.remove(&report.job_id) else {
                            let nack = EdgeLinkMessage::nack(
                                session.edge_id.clone(),
                                Some(session.runtime_id.clone()),
                                message.message_id,
                                message.sequence,
                                "unsolicited discovery report",
                            );
                            write_edgelink_message(&mut writer, &nack).await?;
                            continue;
                        };
                        if report.protocol_connection_id != expected.protocol_connection_id {
                            let reason = "discovery report protocol connection does not match request";
                            let _ = expected.response.send(Err(reason.to_string()));
                            let nack = EdgeLinkMessage::nack(
                                session.edge_id.clone(),
                                Some(session.runtime_id.clone()),
                                message.message_id,
                                message.sequence,
                                reason,
                            );
                            write_edgelink_message(&mut writer, &nack).await?;
                            continue;
                        }
                        let report = report.clone();
                        let response = persist_runtime_message(
                            session,
                            message,
                            store.clone(),
                            sqlite_store.as_ref(),
                        )
                        .await?;
                        write_edgelink_message(&mut writer, &response).await?;
                        let _ = expected.response.send(Ok(report));
                        accepted += 1;
                        continue;
                    }

                    let response = persist_runtime_message(
                        session,
                        message,
                        store.clone(),
                        sqlite_store.as_ref(),
                    )
                    .await?;
                    if matches!(&response.payload, EdgeLinkPayload::Ack(ack) if ack.accepted) {
                        accepted += 1;
                    }
                    write_edgelink_message(&mut writer, &response).await?;
                }
                command = commands.recv() => {
                    let Some(command) = command else {
                        break;
                    };
                    match command {
                        EdgeGatewayCommand::Discovery { request, response } => {
                            if !pending.is_empty() {
                                let _ = response.send(Err("another discovery request is already running".to_string()));
                                continue;
                            }
                            let job_id = request.job_id.clone();
                            let protocol_connection_id = request.protocol_connection_id.clone();
                            let message = EdgeLinkMessage::discovery_request(
                                session.edge_id.clone(),
                                session.runtime_id.clone(),
                                sequence,
                                request,
                            );
                            sequence = sequence.saturating_add(1);
                            let request_message_id = message.message_id;
                            if let Err(error) = write_edgelink_message(&mut writer, &message).await {
                                let _ = response.send(Err(error.to_string()));
                                return Err(error);
                            }
                            pending.insert(
                                job_id,
                                PendingDiscovery {
                                    request_message_id,
                                    protocol_connection_id,
                                    response,
                                },
                            );
                        }
                    }
                }
            }
        }
        Ok::<usize, anyhow::Error>(accepted)
    }
    .await;

    registry.unregister(&session.edge_id, session_id).await;
    for (_, request) in pending {
        let _ = request.response.send(Err(format!(
            "runtime disconnected before discovery request {} completed",
            request.request_message_id
        )));
    }
    result
}

async fn deploy_pending_config_if_available<S>(
    stream: &mut S,
    session: &EdgeGatewaySession,
    store: Arc<Mutex<CloudControlStore>>,
    sqlite_store: Option<&SqliteCloudStore>,
) -> Result<usize>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let Some(package) = pending_config_package(&store, &session.edge_id)? else {
        return Ok(0);
    };

    let desired_version = package.version.clone();
    let deploy = EdgeLinkMessage::config_deploy(
        session.edge_id.clone(),
        session.runtime_id.clone(),
        2,
        package,
    );
    write_edgelink_message(stream, &deploy)
        .await
        .context("failed to write EdgeLink config deploy")?;

    let report_message = read_edgelink_message(stream)
        .await
        .context("failed to read EdgeLink config report")?;
    if report_message.edge_id != session.edge_id {
        bail!("config report edge_id does not match session edge_id");
    }
    if report_message.runtime_id.as_deref() != Some(session.runtime_id.as_str()) {
        bail!("config report runtime_id does not match session runtime_id");
    }

    let EdgeLinkPayload::ConfigReport(report) = report_message.payload.clone() else {
        bail!("expected EdgeLink config report after config deploy");
    };
    if let Some((release_id, reported_version, reported_node)) =
        persist_config_report(&store, &session.edge_id, &desired_version, &report)?
    {
        if let Some(sqlite_store) = sqlite_store {
            sqlite_store
                .mark_release_reported(release_id, reported_version)
                .await
                .context("persist EdgeLink config report to sqlite")?;
            if let Some(node) = reported_node {
                sqlite_store
                    .upsert_edge_node(node)
                    .await
                    .context("persist EdgeLink product version report to sqlite")?;
            }
        }
    }

    let ack = EdgeLinkMessage::ack(
        session.edge_id.clone(),
        session.runtime_id.clone(),
        report_message.message_id,
        report_message.sequence,
    );
    write_edgelink_message(stream, &ack)
        .await
        .context("failed to write EdgeLink config report ack")?;

    Ok(1)
}

fn pending_config_package(
    store: &Arc<Mutex<CloudControlStore>>,
    edge_id: &str,
) -> Result<Option<EdgeConfigPackage>> {
    let store = store
        .lock()
        .map_err(|_| anyhow!("cloud control store mutex poisoned"))?;
    let Some(release) = store
        .releases()
        .filter(|release| release.edge_id == edge_id && release.status == ReleaseStatus::Pending)
        .max_by(|left, right| left.desired_version.cmp(&right.desired_version))
    else {
        return Ok(None);
    };

    Ok(store
        .config_package(&release.edge_id, &release.desired_version)
        .cloned())
}

fn persist_config_report(
    store: &Arc<Mutex<CloudControlStore>>,
    edge_id: &str,
    expected_version: &str,
    report: &EdgeLinkConfigReport,
) -> Result<Option<(uuid::Uuid, String, Option<EdgeNode>)>> {
    if report.desired_version != expected_version {
        bail!(
            "config report desired version {} does not match deployed version {}",
            report.desired_version,
            expected_version
        );
    }

    let mut store = store
        .lock()
        .map_err(|_| anyhow!("cloud control store mutex poisoned"))?;
    let Some(release_id) = store
        .releases()
        .find(|release| {
            release.edge_id == edge_id
                && release.desired_version == report.desired_version
                && release.status == ReleaseStatus::Pending
        })
        .map(|release| release.release_id)
    else {
        return Ok(None);
    };

    let reported_version = report
        .applied_version
        .clone()
        .unwrap_or_else(|| "rejected".to_string());
    let updated_release =
        ReleaseService::mark_reported(&mut store, release_id, reported_version.clone())
            .ok_or_else(|| {
                anyhow!("pending release disappeared before config report was applied")
            })?;
    let reported_node = if updated_release.status == ReleaseStatus::Applied {
        let mut node = store
            .edge_nodes()
            .find(|node| node.edge_id == edge_id)
            .cloned();
        if let Some(node) = node.as_mut() {
            node.reported_product_version = Some(reported_version.clone());
            store.register_edge(node.clone());
        }
        node
    } else {
        None
    };
    Ok(Some((release_id, reported_version, reported_node)))
}

async fn persist_runtime_message(
    session: &EdgeGatewaySession,
    message: EdgeLinkMessage,
    store: Arc<Mutex<CloudControlStore>>,
    sqlite_store: Option<&SqliteCloudStore>,
) -> Result<EdgeLinkMessage> {
    if message.edge_id != session.edge_id {
        bail!(
            "EdgeLink message edge_id {} does not match session edge_id {}",
            message.edge_id,
            session.edge_id
        );
    }
    if message.runtime_id.as_deref() != Some(session.runtime_id.as_str()) {
        bail!("EdgeLink message runtime_id does not match session runtime_id");
    }

    let ack_message_id = message.message_id;
    let ack_sequence = message.sequence;
    match message.payload {
        EdgeLinkPayload::RuntimeMetrics(snapshot) => {
            if snapshot.edge_id != session.edge_id {
                bail!("runtime metrics edge_id does not match session edge_id");
            }
            store
                .lock()
                .map_err(|_| anyhow!("cloud control store mutex poisoned"))?
                .upsert_runtime_metrics(snapshot.clone());
            if let Some(sqlite_store) = sqlite_store {
                sqlite_store
                    .upsert_runtime_metrics(snapshot)
                    .await
                    .context("persist EdgeLink runtime metrics to sqlite")?;
            }
            Ok(EdgeLinkMessage::ack(
                session.edge_id.clone(),
                session.runtime_id.clone(),
                ack_message_id,
                ack_sequence,
            ))
        }
        EdgeLinkPayload::RuntimeEvent(event) => {
            if event.edge_id != session.edge_id {
                bail!("runtime event edge_id does not match session edge_id");
            }
            store
                .lock()
                .map_err(|_| anyhow!("cloud control store mutex poisoned"))?
                .push_runtime_event(event.clone());
            if let Some(sqlite_store) = sqlite_store {
                sqlite_store
                    .push_runtime_event(event)
                    .await
                    .context("persist EdgeLink runtime event to sqlite")?;
            }
            Ok(EdgeLinkMessage::ack(
                session.edge_id.clone(),
                session.runtime_id.clone(),
                ack_message_id,
                ack_sequence,
            ))
        }
        EdgeLinkPayload::DiscoveryReport(report) => {
            store
                .lock()
                .map_err(|_| anyhow!("cloud control store mutex poisoned"))?
                .insert_discovery_report(session.edge_id.clone(), report.clone());
            if let Some(sqlite_store) = sqlite_store {
                sqlite_store
                    .insert_discovery_report(&session.edge_id, report)
                    .await
                    .context("persist EdgeLink discovery report to sqlite")?;
            }
            Ok(EdgeLinkMessage::ack(
                session.edge_id.clone(),
                session.runtime_id.clone(),
                ack_message_id,
                ack_sequence,
            ))
        }
        EdgeLinkPayload::Heartbeat(_) => Ok(EdgeLinkMessage::ack(
            session.edge_id.clone(),
            session.runtime_id.clone(),
            ack_message_id,
            ack_sequence,
        )),
        _ => Ok(EdgeLinkMessage::nack(
            session.edge_id.clone(),
            Some(session.runtime_id.clone()),
            ack_message_id,
            ack_sequence,
            "unsupported EdgeLink payload for runtime ingestion",
        )),
    }
}

async fn read_edgelink_message<S>(stream: &mut S) -> Result<EdgeLinkMessage>
where
    S: AsyncRead + Unpin,
{
    let mut header = [0_u8; 4];
    stream
        .read_exact(&mut header)
        .await
        .context("failed to read EdgeLink frame header")?;

    let payload_len = u32::from_be_bytes(header) as usize;
    if payload_len > EDGELINK_MAX_FRAME_BYTES {
        bail!(
            "EdgeLink frame too large: {} bytes exceeds {} bytes",
            payload_len,
            EDGELINK_MAX_FRAME_BYTES
        );
    }

    let mut frame = vec![0_u8; 4 + payload_len];
    frame[..4].copy_from_slice(&header);
    stream
        .read_exact(&mut frame[4..])
        .await
        .context("failed to read EdgeLink frame body")?;

    decode_edgelink_frame(&frame).context("failed to decode EdgeLink frame")
}

async fn read_optional_edgelink_message<S>(stream: &mut S) -> Result<Option<EdgeLinkMessage>>
where
    S: AsyncRead + Unpin,
{
    let mut header = [0_u8; 4];
    let read = stream
        .read(&mut header)
        .await
        .context("failed to read EdgeLink frame header")?;
    if read == 0 {
        return Ok(None);
    }
    if read < header.len() {
        stream
            .read_exact(&mut header[read..])
            .await
            .map_err(|error| {
                if error.kind() == ErrorKind::UnexpectedEof {
                    anyhow!(
                        "incomplete EdgeLink frame header: expected 4 bytes, got {}",
                        read
                    )
                } else {
                    anyhow!(error).context("failed to read remaining EdgeLink frame header")
                }
            })?;
    }

    let payload_len = u32::from_be_bytes(header) as usize;
    if payload_len > EDGELINK_MAX_FRAME_BYTES {
        bail!(
            "EdgeLink frame too large: {} bytes exceeds {} bytes",
            payload_len,
            EDGELINK_MAX_FRAME_BYTES
        );
    }

    let mut frame = vec![0_u8; 4 + payload_len];
    frame[..4].copy_from_slice(&header);
    stream
        .read_exact(&mut frame[4..])
        .await
        .context("failed to read EdgeLink frame body")?;

    decode_edgelink_frame(&frame)
        .map(Some)
        .context("failed to decode EdgeLink frame")
}

async fn write_edgelink_message<S>(stream: &mut S, message: &EdgeLinkMessage) -> Result<()>
where
    S: AsyncWrite + Unpin,
{
    let frame = encode_edgelink_frame(message).context("failed to encode EdgeLink frame")?;
    stream
        .write_all(&frame)
        .await
        .context("failed to write EdgeLink frame")?;
    Ok(())
}

fn load_certs(pem: &str, label: &str) -> Result<Vec<CertificateDer<'static>>> {
    let mut reader = Cursor::new(pem.as_bytes());
    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to read {label} PEM"))?;
    if certs.is_empty() {
        bail!("missing {label}");
    }
    Ok(certs)
}

fn load_private_key(pem: &str, label: &str) -> Result<PrivateKeyDer<'static>> {
    let mut reader = Cursor::new(pem.as_bytes());
    rustls_pemfile::private_key(&mut reader)
        .with_context(|| format!("failed to read {label} PEM"))?
        .with_context(|| format!("missing {label}"))
}
