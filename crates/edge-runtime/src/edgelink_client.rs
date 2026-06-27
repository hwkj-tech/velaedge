use std::{io::Cursor, sync::Arc};

use anyhow::{bail, Context, Result};
use edge_core::{
    decode_edgelink_frame, encode_edgelink_frame, EdgeLinkMessage, EdgeLinkPayload,
    EDGELINK_MAX_FRAME_BYTES,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::{
    rustls::{
        self,
        pki_types::{CertificateDer, PrivateKeyDer, ServerName},
        RootCertStore,
    },
    TlsConnector,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeLinkConnectReport {
    pub edge_id: String,
    pub runtime_id: String,
    pub gateway_addr: String,
    pub acked: bool,
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
    )
    .await
}

async fn connect_edgelink_over_stream<S>(
    stream: &mut S,
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    applied_config_version: Option<String>,
) -> Result<EdgeLinkConnectReport>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let hello = EdgeLinkMessage::hello(
        edge_id,
        runtime_id,
        runtime_version,
        applied_config_version,
        Vec::new(),
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
