use std::net::SocketAddr;

use anyhow::{bail, Context, Result};
use edge_core::{
    decode_edgelink_frame, encode_edgelink_frame, EdgeLinkMessage, EdgeLinkPayload,
    EDGELINK_MAX_FRAME_BYTES,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeGatewaySession {
    pub edge_id: String,
    pub runtime_id: String,
    pub peer_addr: SocketAddr,
}

pub async fn handle_edgelink_session(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
) -> Result<EdgeGatewaySession> {
    let message = read_edgelink_message(&mut stream)
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
    write_edgelink_message(&mut stream, &ack)
        .await
        .context("failed to write EdgeLink hello ack")?;

    Ok(EdgeGatewaySession {
        edge_id: message.edge_id,
        runtime_id: hello.runtime_id.clone(),
        peer_addr,
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
