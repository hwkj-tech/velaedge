use std::{
    io::{Cursor, ErrorKind},
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, bail, Context, Result};
use cloud_control::CloudControlStore;
use edge_core::{
    decode_edgelink_frame, encode_edgelink_frame, EdgeLinkMessage, EdgeLinkPayload,
    EDGELINK_MAX_FRAME_BYTES,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
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
    pub peer_addr: SocketAddr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeGatewaySessionReport {
    pub session: EdgeGatewaySession,
    pub accepted_message_count: usize,
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

        let client_verifier = WebPkiClientVerifier::builder(Arc::new(client_roots))
            .build()
            .context("failed to build client certificate verifier")?;
        let config = rustls::ServerConfig::builder()
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
    handle_edgelink_stream(&mut stream, peer_addr).await
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
    let session = handle_edgelink_stream(&mut stream, peer_addr).await?;
    let accepted_message_count =
        handle_edgelink_runtime_messages(&mut stream, &session, store).await?;

    Ok(EdgeGatewaySessionReport {
        session,
        accepted_message_count,
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
    handle_edgelink_stream(&mut stream, peer_addr).await
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
    let session = handle_edgelink_stream(&mut stream, peer_addr).await?;
    let accepted_message_count =
        handle_edgelink_runtime_messages(&mut stream, &session, store).await?;

    Ok(EdgeGatewaySessionReport {
        session,
        accepted_message_count,
    })
}

async fn handle_edgelink_stream<S>(
    stream: &mut S,
    peer_addr: SocketAddr,
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
        peer_addr,
    })
}

async fn handle_edgelink_runtime_messages<S>(
    stream: &mut S,
    session: &EdgeGatewaySession,
    store: Arc<Mutex<CloudControlStore>>,
) -> Result<usize>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut accepted = 0;
    while let Some(message) = read_optional_edgelink_message(stream).await? {
        let response = persist_runtime_message(session, message, store.clone())?;
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

fn persist_runtime_message(
    session: &EdgeGatewaySession,
    message: EdgeLinkMessage,
    store: Arc<Mutex<CloudControlStore>>,
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
                .upsert_runtime_metrics(snapshot);
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
                .push_runtime_event(event);
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
