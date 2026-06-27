use anyhow::{bail, Context, Result};
use edge_core::{
    decode_edgelink_frame, encode_edgelink_frame, EdgeLinkMessage, EdgeLinkPayload,
    EDGELINK_MAX_FRAME_BYTES,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeLinkConnectReport {
    pub edge_id: String,
    pub runtime_id: String,
    pub gateway_addr: String,
    pub acked: bool,
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

    let hello = EdgeLinkMessage::hello(
        edge_id,
        runtime_id,
        runtime_version,
        applied_config_version,
        Vec::new(),
    );
    write_edgelink_message(&mut stream, &hello)
        .await
        .context("failed to write EdgeLink hello")?;

    let ack = read_edgelink_message(&mut stream)
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

async fn read_edgelink_message(stream: &mut TcpStream) -> Result<EdgeLinkMessage> {
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

async fn write_edgelink_message(stream: &mut TcpStream, message: &EdgeLinkMessage) -> Result<()> {
    let frame = encode_edgelink_frame(message).context("failed to encode EdgeLink frame")?;
    stream
        .write_all(&frame)
        .await
        .context("failed to write EdgeLink frame")?;
    Ok(())
}
