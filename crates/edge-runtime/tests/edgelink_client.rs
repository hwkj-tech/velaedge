use chrono::Utc;
use edge_core::{
    decode_edgelink_frame, encode_edgelink_frame, CloudSyncMetrics, CollectionRuntimeMetrics,
    EdgeHealth, EdgeLinkMessage, EdgeLinkMessageKind, EdgeLinkPayload, EdgeRuntimeEvent,
    EdgeRuntimeMetricsSnapshot, LocalStoreMetrics, ProtocolRuntimeMetrics, RuntimeEventCategory,
    RuntimeEventSeverity, SystemRuntimeMetrics,
};
use edge_runtime::{connect_edgelink_once, publish_edgelink_runtime_status_once};
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

#[tokio::test]
async fn runtime_client_publishes_metrics_and_events_after_hello() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let gateway_addr = listener.local_addr().expect("listener should expose addr");

    let gateway = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("runtime should connect");

        let hello = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &hello).await;

        let metrics = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &metrics).await;

        let event = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &event).await;

        vec![hello, metrics, event]
    });

    let report = publish_edgelink_runtime_status_once(
        &gateway_addr.to_string(),
        "edge-dev",
        "runtime-dev",
        "0.1.0",
        runtime_metrics("edge-dev", "runtime-dev"),
        vec![EdgeRuntimeEvent::new(
            "edge-dev",
            RuntimeEventSeverity::Warning,
            RuntimeEventCategory::Protocol,
            "modbus.timeout",
            "Modbus request timed out",
        )],
    )
    .await
    .expect("runtime status should publish");

    assert_eq!(report.edge_id, "edge-dev");
    assert_eq!(report.runtime_id, "runtime-dev");
    assert_eq!(report.acked_message_count, 2);

    let observed = gateway.await.expect("gateway task should finish");
    assert_eq!(observed[0].kind, EdgeLinkMessageKind::Hello);
    assert_eq!(observed[1].kind, EdgeLinkMessageKind::RuntimeMetrics);
    assert_eq!(observed[2].kind, EdgeLinkMessageKind::RuntimeEvent);

    let EdgeLinkPayload::RuntimeMetrics(metrics) = &observed[1].payload else {
        panic!("expected runtime metrics payload");
    };
    assert_eq!(metrics.config_version, "2026.06.27-001");

    let EdgeLinkPayload::RuntimeEvent(event) = &observed[2].payload else {
        panic!("expected runtime event payload");
    };
    assert_eq!(event.code, "modbus.timeout");
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

async fn write_ack_for(stream: &mut TcpStream, message: &EdgeLinkMessage) {
    let ack = EdgeLinkMessage::ack(
        message.edge_id.clone(),
        message
            .runtime_id
            .clone()
            .expect("runtime message should include runtime id"),
        message.message_id,
        message.sequence,
    );
    let ack_frame = encode_edgelink_frame(&ack).expect("ack should encode");
    stream
        .write_all(&ack_frame)
        .await
        .expect("ack should be written");
}

fn runtime_metrics(edge_id: &str, runtime_id: &str) -> EdgeRuntimeMetricsSnapshot {
    EdgeRuntimeMetricsSnapshot {
        edge_id: edge_id.to_string(),
        runtime_id: runtime_id.to_string(),
        config_version: "2026.06.27-001".to_string(),
        timestamp: Utc::now(),
        health: EdgeHealth::Healthy,
        system: SystemRuntimeMetrics {
            cpu_percent: 21.0,
            memory_percent: 44.0,
            disk_percent: 58.0,
            process_uptime_seconds: 180,
        },
        collection: CollectionRuntimeMetrics {
            active_task_count: 1,
            success_rate: 0.99,
            average_latency_ms: 20,
            bad_point_count: 0,
        },
        protocols: vec![ProtocolRuntimeMetrics {
            connection_id: "modbus-main".to_string(),
            protocol: "Modbus TCP".to_string(),
            connected: true,
            latency_ms: 10,
            timeout_count: 1,
            error_count: 0,
            reconnect_count: 0,
        }],
        local_store: LocalStoreMetrics {
            backend: "rocksdb".to_string(),
            buffered_records: 0,
            oldest_buffer_age_seconds: 0,
            disk_usage_percent: 32.0,
        },
        algorithms: Vec::new(),
        cloud_sync: CloudSyncMetrics {
            connected: true,
            last_sync_seconds_ago: 0,
            pending_uploads: 0,
            desired_version: "2026.06.27-001".to_string(),
            reported_version: "2026.06.27-001".to_string(),
        },
    }
}
