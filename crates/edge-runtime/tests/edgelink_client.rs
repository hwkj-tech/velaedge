use edge_core::{
    decode_edgelink_frame, encode_edgelink_frame, EdgeLinkMessage, EdgeLinkMessageKind,
    EdgeLinkPayload,
};
use edge_runtime::connect_edgelink_once;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::test]
async fn runtime_client_sends_hello_and_accepts_cloud_ack() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let gateway_addr = listener.local_addr().expect("listener should expose addr");

    let gateway = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("runtime should connect");
        let hello = read_one_message(&mut stream).await;
        let ack = EdgeLinkMessage::ack(
            hello.edge_id.clone(),
            hello
                .runtime_id
                .clone()
                .expect("hello envelope should include runtime id"),
            hello.message_id,
            hello.sequence,
        );
        let ack_frame = encode_edgelink_frame(&ack).expect("ack should encode");
        stream
            .write_all(&ack_frame)
            .await
            .expect("ack should be written");
        hello
    });

    let report = connect_edgelink_once(
        &gateway_addr.to_string(),
        "edge-dev",
        "runtime-dev",
        "0.1.0",
        Some("2026.06.26-001".to_string()),
    )
    .await
    .expect("runtime client should connect");

    assert_eq!(report.edge_id, "edge-dev");
    assert_eq!(report.runtime_id, "runtime-dev");
    assert_eq!(report.gateway_addr, gateway_addr.to_string());
    assert!(report.acked);

    let observed = gateway.await.expect("gateway task should finish");
    assert_eq!(observed.kind, EdgeLinkMessageKind::Hello);
    assert_eq!(observed.edge_id, "edge-dev");
    assert_eq!(observed.runtime_id.as_deref(), Some("runtime-dev"));

    let EdgeLinkPayload::Hello(payload) = observed.payload else {
        panic!("expected hello payload");
    };
    assert_eq!(payload.runtime_version, "0.1.0");
    assert_eq!(
        payload.applied_config_version.as_deref(),
        Some("2026.06.26-001")
    );
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
    decode_edgelink_frame(&frame).expect("message should decode")
}
