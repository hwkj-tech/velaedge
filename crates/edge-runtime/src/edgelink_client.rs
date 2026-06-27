use std::{io::Cursor, sync::Arc};

use anyhow::{bail, Context, Result};
use edge_core::{
    decode_edgelink_frame, encode_edgelink_frame, EdgeLinkMessage, EdgeLinkPayload,
    EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot, EDGELINK_MAX_FRAME_BYTES,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};
use tokio_rustls::{
    rustls::{
        self,
        pki_types::{CertificateDer, PrivateKeyDer, ServerName},
        RootCertStore,
    },
    TlsConnector,
};

use crate::{AppliedEdgeConfig, ConfiguredSimulatedRuntime, RocksEdgeRuntimeStore};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeLinkConnectReport {
    pub edge_id: String,
    pub runtime_id: String,
    pub gateway_addr: String,
    pub acked: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeLinkPublishReport {
    pub edge_id: String,
    pub runtime_id: String,
    pub gateway_addr: String,
    pub acked_message_count: usize,
    pub applied_config_version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeLinkClientTlsConfig {
    pub ca_cert_pem: String,
    pub client_cert_pem: String,
    pub client_key_pem: String,
    pub server_name: String,
}

#[derive(Clone)]
pub struct EdgeLinkClientTlsConnector {
    connector: TlsConnector,
    server_name: ServerName<'static>,
}

impl EdgeLinkClientTlsConfig {
    pub fn build_connector(&self) -> Result<EdgeLinkClientTlsConnector> {
        let ca_certs = load_certs(&self.ca_cert_pem, "CA certificate")?;
        let client_certs = load_certs(&self.client_cert_pem, "client certificate")?;
        let client_key = load_private_key(&self.client_key_pem, "client private key")?;

        let mut roots = RootCertStore::empty();
        for cert in ca_certs {
            roots.add(cert).context("invalid CA certificate")?;
        }

        let config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_client_auth_cert(client_certs, client_key)
            .context("failed to build EdgeLink client TLS config")?;
        let server_name = ServerName::try_from(self.server_name.as_str())
            .context("invalid EdgeLink TLS server name")?
            .to_owned();

        Ok(EdgeLinkClientTlsConnector {
            connector: TlsConnector::from(Arc::new(config)),
            server_name,
        })
    }
}

pub async fn connect_edgelink_once(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    applied_config_version: Option<String>,
) -> Result<EdgeLinkConnectReport> {
    connect_edgelink_once_with_capabilities(
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        applied_config_version,
        Vec::new(),
    )
    .await
}

pub async fn connect_edgelink_once_with_capabilities(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    applied_config_version: Option<String>,
    capabilities: Vec<String>,
) -> Result<EdgeLinkConnectReport> {
    let mut stream = TcpStream::connect(gateway_addr)
        .await
        .with_context(|| format!("failed to connect EdgeLink gateway at {gateway_addr}"))?;
    connect_edgelink_over_stream(
        &mut stream,
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        applied_config_version,
        capabilities,
    )
    .await
}

pub async fn connect_edgelink_tls_once(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    applied_config_version: Option<String>,
    tls_config: &EdgeLinkClientTlsConfig,
) -> Result<EdgeLinkConnectReport> {
    let stream = TcpStream::connect(gateway_addr)
        .await
        .with_context(|| format!("failed to connect EdgeLink gateway at {gateway_addr}"))?;
    let tls = tls_config.build_connector()?;
    let mut stream = tls
        .connector
        .connect(tls.server_name, stream)
        .await
        .context("failed to open EdgeLink TLS session")?;

    connect_edgelink_over_stream(
        &mut stream,
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        applied_config_version,
        Vec::new(),
    )
    .await
}

pub async fn publish_edgelink_runtime_status_once(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    snapshot: EdgeRuntimeMetricsSnapshot,
    events: Vec<EdgeRuntimeEvent>,
) -> Result<EdgeLinkPublishReport> {
    publish_edgelink_runtime_status_inner(
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        snapshot,
        events,
        None,
        Vec::new(),
    )
    .await
}

pub async fn publish_edgelink_runtime_status_with_store_once(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    snapshot: EdgeRuntimeMetricsSnapshot,
    events: Vec<EdgeRuntimeEvent>,
    store: &RocksEdgeRuntimeStore,
) -> Result<EdgeLinkPublishReport> {
    publish_edgelink_runtime_status_with_store_and_capabilities_once(
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        snapshot,
        events,
        store,
        Vec::new(),
    )
    .await
}

pub async fn publish_edgelink_runtime_status_with_store_and_capabilities_once(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    snapshot: EdgeRuntimeMetricsSnapshot,
    events: Vec<EdgeRuntimeEvent>,
    store: &RocksEdgeRuntimeStore,
    capabilities: Vec<String>,
) -> Result<EdgeLinkPublishReport> {
    publish_edgelink_runtime_status_inner(
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        snapshot,
        events,
        Some(store),
        capabilities,
    )
    .await
}

async fn publish_edgelink_runtime_status_inner(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    snapshot: EdgeRuntimeMetricsSnapshot,
    events: Vec<EdgeRuntimeEvent>,
    store: Option<&RocksEdgeRuntimeStore>,
    capabilities: Vec<String>,
) -> Result<EdgeLinkPublishReport> {
    if snapshot.edge_id != edge_id {
        bail!("runtime metrics edge_id does not match EdgeLink edge_id");
    }
    if snapshot.runtime_id != runtime_id {
        bail!("runtime metrics runtime_id does not match EdgeLink runtime_id");
    }
    for event in &events {
        if event.edge_id != edge_id {
            bail!("runtime event edge_id does not match EdgeLink edge_id");
        }
    }

    let mut stream = TcpStream::connect(gateway_addr)
        .await
        .with_context(|| format!("failed to connect EdgeLink gateway at {gateway_addr}"))?;
    connect_edgelink_over_stream(
        &mut stream,
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        Some(snapshot.config_version.clone()),
        capabilities,
    )
    .await?;

    let config_apply =
        apply_optional_config_deploy(&mut stream, edge_id, runtime_id, store).await?;
    let mut acked_message_count = 0;
    let metrics =
        EdgeLinkMessage::runtime_metrics(edge_id, runtime_id, config_apply.next_sequence, snapshot);
    write_edgelink_message_and_expect_ack(&mut stream, &metrics).await?;
    acked_message_count += 1;

    for (index, event) in events.into_iter().enumerate() {
        let message = EdgeLinkMessage::runtime_event(
            edge_id,
            runtime_id,
            config_apply.next_sequence + index as u64 + 1,
            event,
        );
        write_edgelink_message_and_expect_ack(&mut stream, &message).await?;
        acked_message_count += 1;
    }

    Ok(EdgeLinkPublishReport {
        edge_id: edge_id.to_string(),
        runtime_id: runtime_id.to_string(),
        gateway_addr: gateway_addr.to_string(),
        acked_message_count,
        applied_config_version: config_apply.applied_config_version,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EdgeLinkOptionalConfigApply {
    applied_config_version: Option<String>,
    next_sequence: u64,
}

async fn connect_edgelink_over_stream<S>(
    stream: &mut S,
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    applied_config_version: Option<String>,
    capabilities: Vec<String>,
) -> Result<EdgeLinkConnectReport>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let hello = EdgeLinkMessage::hello(
        edge_id,
        runtime_id,
        runtime_version,
        applied_config_version,
        capabilities,
    );
    write_edgelink_message(stream, &hello)
        .await
        .context("failed to write EdgeLink hello")?;

    let ack = read_edgelink_message(stream)
        .await
        .context("failed to read EdgeLink hello ack")?;
    let EdgeLinkPayload::Ack(payload) = ack.payload else {
        bail!("EdgeLink gateway did not return an ack");
    };
    if payload.ack_message_id != hello.message_id || payload.ack_sequence != hello.sequence {
        bail!("EdgeLink gateway ack does not match hello message");
    }

    Ok(EdgeLinkConnectReport {
        edge_id: edge_id.to_string(),
        runtime_id: runtime_id.to_string(),
        gateway_addr: gateway_addr.to_string(),
        acked: payload.accepted,
    })
}

async fn apply_optional_config_deploy<S>(
    stream: &mut S,
    edge_id: &str,
    runtime_id: &str,
    store: Option<&RocksEdgeRuntimeStore>,
) -> Result<EdgeLinkOptionalConfigApply>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let message = match timeout(Duration::from_millis(50), read_edgelink_message(stream)).await {
        Err(_) => {
            return Ok(EdgeLinkOptionalConfigApply {
                applied_config_version: None,
                next_sequence: 2,
            });
        }
        Ok(result) => result.context("failed to read optional EdgeLink config deploy")?,
    };

    let EdgeLinkPayload::ConfigDeploy(package) = message.payload else {
        bail!("expected EdgeLink config deploy from gateway");
    };
    let desired_version = package.version.clone();
    let report_sequence = message.sequence + 1;

    if package.edge_id != edge_id {
        let report = EdgeLinkMessage::config_report(
            edge_id,
            runtime_id,
            report_sequence,
            desired_version,
            None,
            false,
            Some("config package targets a different edge".to_string()),
        );
        write_edgelink_message_and_expect_ack(stream, &report).await?;
        bail!("config package targets a different edge");
    }

    if let Some(store) = store {
        store
            .put_desired_config(&package)
            .context("failed to persist EdgeLink desired config")?;
    }

    let applied = match AppliedEdgeConfig::apply(package) {
        Ok(applied) => applied,
        Err(error) => {
            let report = EdgeLinkMessage::config_report(
                edge_id,
                runtime_id,
                report_sequence,
                desired_version,
                None,
                false,
                Some(error.to_string()),
            );
            write_edgelink_message_and_expect_ack(stream, &report).await?;
            return Err(error).context("failed to apply EdgeLink config deploy");
        }
    };
    let mut runtime = ConfiguredSimulatedRuntime::new(applied.clone());
    runtime
        .collect_once()
        .await
        .context("failed to run collection after EdgeLink config deploy")?;
    let applied_version = runtime.reported_version().to_string();
    if let Some(store) = store {
        store
            .promote_active_config(edge_id, &applied_version)
            .context("failed to promote EdgeLink active config")?;
    }

    let report = EdgeLinkMessage::config_report(
        edge_id,
        runtime_id,
        report_sequence,
        desired_version,
        Some(applied_version.clone()),
        true,
        None,
    );
    write_edgelink_message_and_expect_ack(stream, &report).await?;

    Ok(EdgeLinkOptionalConfigApply {
        applied_config_version: Some(applied_version),
        next_sequence: report_sequence + 1,
    })
}

async fn write_edgelink_message_and_expect_ack<S>(
    stream: &mut S,
    message: &EdgeLinkMessage,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    write_edgelink_message(stream, message).await?;
    let ack = read_edgelink_message(stream).await?;
    let EdgeLinkPayload::Ack(payload) = ack.payload else {
        bail!("EdgeLink gateway did not return an ack");
    };
    if payload.ack_message_id != message.message_id || payload.ack_sequence != message.sequence {
        bail!("EdgeLink gateway ack does not match message");
    }
    if !payload.accepted {
        bail!(
            "EdgeLink gateway rejected message: {}",
            payload.reason.unwrap_or_else(|| "no reason".to_string())
        );
    }
    Ok(())
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
