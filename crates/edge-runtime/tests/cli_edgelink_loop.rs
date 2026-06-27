use std::io::{Read, Write};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use edge_core::{
    decode_edgelink_frame, encode_edgelink_frame, EdgeLinkMessage, EdgeLinkMessageKind,
    EdgeLinkPayload,
};

#[test]
fn edge_runtime_cli_reports_metrics_over_edgelink_once() {
    let (gateway_addr, received) = spawn_edgelink_recorder(2);

    let output = Command::new(env!("CARGO_BIN_EXE_edge-runtime"))
        .args([
            "--edge-id",
            "edge-cli",
            "--runtime-id",
            "runtime-cli",
            "--cloud-gateway-addr",
            &gateway_addr,
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "edge-runtime failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let messages = received
        .recv_timeout(Duration::from_secs(2))
        .expect("EdgeLink messages should be recorded");
    assert_eq!(messages[0].kind, EdgeLinkMessageKind::Hello);
    assert_eq!(messages[1].kind, EdgeLinkMessageKind::RuntimeMetrics);

    let EdgeLinkPayload::RuntimeMetrics(metrics) = &messages[1].payload else {
        panic!("expected runtime metrics payload");
    };
    assert_eq!(metrics.edge_id, "edge-cli");
    assert_eq!(metrics.runtime_id, "runtime-cli");
    assert_eq!(metrics.local_store.backend, "jsonl");
}

fn spawn_edgelink_recorder(
    expected_messages: usize,
) -> (String, mpsc::Receiver<Vec<EdgeLinkMessage>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let gateway_addr = listener.local_addr().unwrap().to_string();
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut messages = Vec::new();
        for _ in 0..expected_messages {
            let message = read_one_message(&mut stream).unwrap();
            let ack = EdgeLinkMessage::ack(
                message.edge_id.clone(),
                message.runtime_id.clone().unwrap(),
                message.message_id,
                message.sequence,
            );
            let ack_frame = encode_edgelink_frame(&ack).unwrap();
            stream.write_all(&ack_frame).unwrap();
            messages.push(message);
        }
        tx.send(messages).unwrap();
    });

    (gateway_addr, rx)
}

fn read_one_message(stream: &mut std::net::TcpStream) -> std::io::Result<EdgeLinkMessage> {
    let mut header = [0_u8; 4];
    stream.read_exact(&mut header)?;
    let len = u32::from_be_bytes(header) as usize;
    let mut frame = vec![0_u8; 4 + len];
    frame[..4].copy_from_slice(&header);
    stream.read_exact(&mut frame[4..])?;
    decode_edgelink_frame(&frame)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}
