use std::io::{Read, Write};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use edge_core::{
    decode_edgelink_frame, encode_edgelink_frame, EdgeConfigPackage, EdgeLinkMessage,
    EdgeLinkMessageKind, EdgeLinkPayload,
};
use edge_runtime::RocksEdgeRuntimeStore;
use tempfile::tempdir;

#[test]
fn edge_runtime_cli_reports_metrics_over_edgelink_once() {
    let (gateway_addr, received) = spawn_edgelink_recorder(2);
    let dir = tempdir().unwrap();
    let runtime_db = dir.path().join("runtime.rocksdb");

    let output = Command::new(env!("CARGO_BIN_EXE_edge-runtime"))
        .args([
            "--edge-id",
            "edge-cli",
            "--runtime-id",
            "runtime-cli",
            "--cloud-gateway-addr",
            &gateway_addr,
            "--runtime-db",
            runtime_db.to_str().unwrap(),
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

#[test]
fn edge_runtime_cli_persists_config_deploy_in_runtime_db() {
    let (gateway_addr, received) = spawn_edgelink_config_deploy_recorder();
    let dir = tempdir().unwrap();
    let runtime_db = dir.path().join("runtime.rocksdb");

    let output = Command::new(env!("CARGO_BIN_EXE_edge-runtime"))
        .args([
            "--edge-id",
            "edge-cli-config",
            "--runtime-id",
            "runtime-cli-config",
            "--cloud-gateway-addr",
            &gateway_addr,
            "--runtime-db",
            runtime_db.to_str().unwrap(),
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
    assert_eq!(messages[1].kind, EdgeLinkMessageKind::ConfigReport);
    assert_eq!(messages[2].kind, EdgeLinkMessageKind::RuntimeMetrics);

    let store = RocksEdgeRuntimeStore::open(runtime_db).unwrap();
    let active = store
        .active_config("edge-cli-config")
        .unwrap()
        .expect("active config should be persisted");
    assert_eq!(active.version, "2026.06.27-040");
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

fn spawn_edgelink_config_deploy_recorder() -> (String, mpsc::Receiver<Vec<EdgeLinkMessage>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let gateway_addr = listener.local_addr().unwrap().to_string();
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut messages = Vec::new();

        let hello = read_one_message(&mut stream).unwrap();
        write_ack_for(&mut stream, &hello);
        messages.push(hello);

        let deploy = EdgeLinkMessage::config_deploy(
            "edge-cli-config",
            "runtime-cli-config",
            2,
            EdgeConfigPackage::new("edge-cli-config", "2026.06.27-040"),
        );
        let deploy_frame = encode_edgelink_frame(&deploy).unwrap();
        stream.write_all(&deploy_frame).unwrap();

        let report = read_one_message(&mut stream).unwrap();
        write_ack_for(&mut stream, &report);
        messages.push(report);

        let metrics = read_one_message(&mut stream).unwrap();
        write_ack_for(&mut stream, &metrics);
        messages.push(metrics);

        tx.send(messages).unwrap();
    });

    (gateway_addr, rx)
}

fn write_ack_for(stream: &mut std::net::TcpStream, message: &EdgeLinkMessage) {
    let ack = EdgeLinkMessage::ack(
        message.edge_id.clone(),
        message.runtime_id.clone().unwrap(),
        message.message_id,
        message.sequence,
    );
    let ack_frame = encode_edgelink_frame(&ack).unwrap();
    stream.write_all(&ack_frame).unwrap();
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
