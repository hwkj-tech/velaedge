use std::{io::Cursor, net::SocketAddr, sync::Arc};

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
        pki_types::{CertificateDer, PrivateKeyDer},
        server::WebPkiClientVerifier,
        RootCertStore,
    },
    TlsAcceptor,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeGatewaySession {
    pub edge_id: String,
    pub runtime_id: String,
    pub peer_addr: SocketAddr,
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
