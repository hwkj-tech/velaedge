use cloud_api::gateway::handle_edgelink_session;
use edge_core::{
    decode_edgelink_frame, encode_edgelink_frame, EdgeLinkMessage, EdgeLinkMessageKind,
    EdgeLinkPayload,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::test]
async fn gateway_acknowledges_runtime_hello_and_records_session_identity() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let gateway_addr = listener.local_addr().expect("listener should expose addr");

    let gateway = tokio::spawn(async move {
        let (stream, peer_addr) = listener.accept().await.expect("runtime should connect");
        handle_edgelink_session(stream, peer_addr)
            .await
            .expect("session should handshake")
    });

    let mut runtime = TcpStream::connect(gateway_addr)
        .await
        .expect("runtime should connect to gateway");
    let hello = EdgeLinkMessage::hello(
        "edge-dev",
        "runtime-dev",
        "0.1.0",
        Some("2026.06.26-001".to_string()),
        vec!["protocol:modbus-tcp".to_string()],
    );
    let hello_frame = encode_edgelink_frame(&hello).expect("hello should encode");
    runtime
        .write_all(&hello_frame)
        .await
        .expect("runtime should write hello");

    let ack = read_one_message(&mut runtime).await;
    assert_eq!(ack.kind, EdgeLinkMessageKind::Ack);
    let EdgeLinkPayload::Ack(payload) = ack.payload else {
        panic!("expected ack payload");
    };
    assert_eq!(payload.ack_message_id, hello.message_id);
    assert_eq!(payload.ack_sequence, hello.sequence);
    assert!(payload.accepted);

    let session = gateway.await.expect("gateway task should finish");
    assert_eq!(session.edge_id, "edge-dev");
    assert_eq!(session.runtime_id, "runtime-dev");
    assert!(session.peer_addr.ip().is_loopback());
}

async fn read_one_message(stream: &mut TcpStream) -> EdgeLinkMessage {
    let mut header = [0_u8; 4];
    stream
        .read_exact(&mut header)
        .await
        .expect("frame header should be readable");
    let len = u32::from_be_bytes(header) as usize;
    let mut frame = vec![0_u8; 4 + len];
    frame[..4].copy_from_slice(&header);
    stream
        .read_exact(&mut frame[4..])
        .await
        .expect("frame body should be readable");
    decode_edgelink_frame(&frame).expect("ack should decode")
}
