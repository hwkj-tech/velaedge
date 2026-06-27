use std::sync::{Arc, Mutex};

use chrono::Utc;
use cloud_api::gateway::{
    handle_edgelink_session, handle_edgelink_session_with_store,
    handle_edgelink_session_with_store_and_sqlite, serve_edgelink_gateway_for_sessions,
};
use cloud_control::{CloudControlStore, ReleaseService, ReleaseStatus, SqliteCloudStore};
use edge_core::{
    decode_edgelink_frame, encode_edgelink_frame, CloudSyncMetrics, CollectionRuntimeMetrics,
    DiscoveredPoint, DiscoveryReport, EdgeConfigPackage, EdgeHealth, EdgeLinkMessage,
    EdgeLinkMessageKind, EdgeLinkPayload, EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot,
    LocalStoreMetrics, PointAddress, PointMappingSuggestion, ProtocolRuntimeMetrics,
    RuntimeEventCategory, RuntimeEventSeverity, SystemRuntimeMetrics, TelemetryType,
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

#[tokio::test]
async fn gateway_ingests_runtime_metrics_and_events_after_hello() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let gateway_addr = listener.local_addr().expect("listener should expose addr");
    let store = Arc::new(Mutex::new(CloudControlStore::default()));
    let gateway_store = store.clone();

    let gateway = tokio::spawn(async move {
        let (stream, peer_addr) = listener.accept().await.expect("runtime should connect");
        handle_edgelink_session_with_store(stream, peer_addr, gateway_store)
            .await
            .expect("session should process runtime messages")
    });

    let mut runtime = TcpStream::connect(gateway_addr)
        .await
        .expect("runtime should connect to gateway");
    let hello = EdgeLinkMessage::hello(
        "edge-live",
        "runtime-live",
        "0.1.0",
        Some("2026.06.27-001".to_string()),
        vec!["protocol:modbus-tcp".to_string()],
    );
    write_one_message(&mut runtime, &hello).await;
    assert_ack_for(&mut runtime, &hello).await;

    let metrics = EdgeLinkMessage::runtime_metrics(
        "edge-live",
        "runtime-live",
        2,
        runtime_metrics("edge-live", "runtime-live"),
    );
    write_one_message(&mut runtime, &metrics).await;
    assert_ack_for(&mut runtime, &metrics).await;

    let event = EdgeLinkMessage::runtime_event(
        "edge-live",
        "runtime-live",
        3,
        EdgeRuntimeEvent::new(
            "edge-live",
            RuntimeEventSeverity::Warning,
            RuntimeEventCategory::Protocol,
            "modbus.timeout",
            "Modbus request timed out",
        ),
    );
    write_one_message(&mut runtime, &event).await;
    assert_ack_for(&mut runtime, &event).await;
    drop(runtime);

    let report = gateway.await.expect("gateway task should finish");
    assert_eq!(report.session.edge_id, "edge-live");
    assert_eq!(report.session.runtime_id, "runtime-live");
    assert_eq!(report.accepted_message_count, 2);

    let store = store.lock().expect("store mutex should not be poisoned");
    let stored_metrics = store
        .runtime_metrics("edge-live")
        .expect("metrics should be stored");
    assert_eq!(stored_metrics.runtime_id, "runtime-live");
    assert_eq!(stored_metrics.config_version, "2026.06.27-001");
    assert_eq!(store.runtime_events().len(), 1);
    assert_eq!(store.runtime_events()[0].code, "modbus.timeout");
}

#[tokio::test]
async fn gateway_persists_runtime_metrics_and_events_to_sqlite() {
    let tempdir = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite://{}", tempdir.path().join("cloud.db").display());
    let sqlite_store = SqliteCloudStore::connect(&database_url).await.unwrap();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let gateway_addr = listener.local_addr().expect("listener should expose addr");
    let store = Arc::new(Mutex::new(CloudControlStore::default()));
    let gateway_store = store.clone();

    let gateway = tokio::spawn(async move {
        let (stream, peer_addr) = listener.accept().await.expect("runtime should connect");
        handle_edgelink_session_with_store_and_sqlite(
            stream,
            peer_addr,
            gateway_store,
            sqlite_store,
        )
        .await
        .expect("session should process runtime messages")
    });

    let mut runtime = TcpStream::connect(gateway_addr)
        .await
        .expect("runtime should connect to gateway");
    let hello = EdgeLinkMessage::hello(
        "edge-sqlite",
        "runtime-sqlite",
        "0.1.0",
        Some("2026.06.27-001".to_string()),
        vec!["protocol:modbus-tcp".to_string()],
    );
    write_one_message(&mut runtime, &hello).await;
    assert_ack_for(&mut runtime, &hello).await;

    let metrics = EdgeLinkMessage::runtime_metrics(
        "edge-sqlite",
        "runtime-sqlite",
        2,
        runtime_metrics("edge-sqlite", "runtime-sqlite"),
    );
    write_one_message(&mut runtime, &metrics).await;
    assert_ack_for(&mut runtime, &metrics).await;

    let event = EdgeLinkMessage::runtime_event(
        "edge-sqlite",
        "runtime-sqlite",
        3,
        EdgeRuntimeEvent::new(
            "edge-sqlite",
            RuntimeEventSeverity::Warning,
            RuntimeEventCategory::Protocol,
            "modbus.timeout",
            "Modbus request timed out",
        ),
    );
    write_one_message(&mut runtime, &event).await;
    assert_ack_for(&mut runtime, &event).await;
    drop(runtime);

    let report = gateway.await.expect("gateway task should finish");
    assert_eq!(report.accepted_message_count, 2);

    let reopened = SqliteCloudStore::connect(&database_url).await.unwrap();
    let stored_metrics = reopened
        .runtime_metrics("edge-sqlite")
        .await
        .unwrap()
        .expect("metrics should be persisted");
    assert_eq!(stored_metrics.runtime_id, "runtime-sqlite");
    let events = reopened.runtime_events().await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].code, "modbus.timeout");
}

#[tokio::test]
async fn gateway_ingests_discovery_reports_and_persists_suggestions() {
    let tempdir = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite://{}", tempdir.path().join("cloud.db").display());
    let sqlite_store = SqliteCloudStore::connect(&database_url).await.unwrap();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let gateway_addr = listener.local_addr().expect("listener should expose addr");
    let store = Arc::new(Mutex::new(CloudControlStore::default()));
    let gateway_store = store.clone();

    let gateway = tokio::spawn(async move {
        let (stream, peer_addr) = listener.accept().await.expect("runtime should connect");
        handle_edgelink_session_with_store_and_sqlite(
            stream,
            peer_addr,
            gateway_store,
            sqlite_store,
        )
        .await
        .expect("session should process discovery report")
    });

    let mut runtime = TcpStream::connect(gateway_addr)
        .await
        .expect("runtime should connect to gateway");
    let hello = EdgeLinkMessage::hello(
        "edge-discovery",
        "runtime-discovery",
        "0.1.0",
        None,
        vec![
            "protocol:modbus-rtu".to_string(),
            "transport:serial".to_string(),
        ],
    );
    write_one_message(&mut runtime, &hello).await;
    assert_ack_for(&mut runtime, &hello).await;

    let discovery = EdgeLinkMessage::discovery_report(
        "edge-discovery",
        "runtime-discovery",
        2,
        discovery_report(),
    );
    write_one_message(&mut runtime, &discovery).await;
    assert_ack_for(&mut runtime, &discovery).await;
    drop(runtime);

    let report = gateway.await.expect("gateway task should finish");
    assert_eq!(report.accepted_message_count, 1);

    {
        let store = store.lock().expect("store mutex should not be poisoned");
        let suggestions = store.discovery_suggestions("edge-discovery");
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].point_id, "meter_voltage_a");
    }

    let reopened = SqliteCloudStore::connect(&database_url).await.unwrap();
    let suggestions = reopened
        .discovery_suggestions("edge-discovery")
        .await
        .unwrap();
    assert_eq!(suggestions.len(), 1);
    assert_eq!(suggestions[0].semantic_id, "electric.voltage_a");
}

#[tokio::test]
async fn gateway_listener_accepts_runtime_session_and_updates_store() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let gateway_addr = listener.local_addr().expect("listener should expose addr");
    let store = Arc::new(Mutex::new(CloudControlStore::default()));
    let gateway_store = store.clone();

    let gateway = tokio::spawn(async move {
        serve_edgelink_gateway_for_sessions(listener, gateway_store, 1)
            .await
            .expect("gateway listener should process one session")
    });

    let mut runtime = TcpStream::connect(gateway_addr)
        .await
        .expect("runtime should connect to gateway listener");
    let hello = EdgeLinkMessage::hello(
        "edge-listener",
        "runtime-listener",
        "0.1.0",
        Some("2026.06.27-001".to_string()),
        vec!["protocol:modbus-tcp".to_string()],
    );
    write_one_message(&mut runtime, &hello).await;
    assert_ack_for(&mut runtime, &hello).await;

    let metrics = EdgeLinkMessage::runtime_metrics(
        "edge-listener",
        "runtime-listener",
        2,
        runtime_metrics("edge-listener", "runtime-listener"),
    );
    write_one_message(&mut runtime, &metrics).await;
    assert_ack_for(&mut runtime, &metrics).await;
    drop(runtime);

    let accepted_sessions = gateway.await.expect("gateway task should finish");
    assert_eq!(accepted_sessions, 1);

    let store = store.lock().expect("store mutex should not be poisoned");
    assert_eq!(
        store
            .runtime_metrics("edge-listener")
            .expect("metrics should be stored")
            .runtime_id,
        "runtime-listener"
    );
}

#[tokio::test]
async fn gateway_deploys_latest_config_and_marks_release_applied_from_report() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let gateway_addr = listener.local_addr().expect("listener should expose addr");
    let mut seeded = CloudControlStore::default();
    let release = ReleaseService::create_release(
        &mut seeded,
        EdgeConfigPackage::new("edge-config", "2026.06.27-010"),
    )
    .expect("release should be valid");
    let store = Arc::new(Mutex::new(seeded));
    let gateway_store = store.clone();

    let gateway = tokio::spawn(async move {
        let (stream, peer_addr) = listener.accept().await.expect("runtime should connect");
        handle_edgelink_session_with_store(stream, peer_addr, gateway_store)
            .await
            .expect("session should process config report")
    });

    let mut runtime = TcpStream::connect(gateway_addr)
        .await
        .expect("runtime should connect to gateway");
    let hello = EdgeLinkMessage::hello(
        "edge-config",
        "runtime-config",
        "0.1.0",
        Some("2026.06.27-009".to_string()),
        Vec::new(),
    );
    write_one_message(&mut runtime, &hello).await;
    assert_ack_for(&mut runtime, &hello).await;

    let deploy = read_one_message(&mut runtime).await;
    assert_eq!(deploy.kind, EdgeLinkMessageKind::ConfigDeploy);
    let EdgeLinkPayload::ConfigDeploy(package) = deploy.payload else {
        panic!("expected config deploy payload");
    };
    assert_eq!(package.edge_id, "edge-config");
    assert_eq!(package.version, "2026.06.27-010");

    let report = EdgeLinkMessage::config_report(
        "edge-config",
        "runtime-config",
        2,
        "2026.06.27-010",
        Some("2026.06.27-010".to_string()),
        true,
        None,
    );
    write_one_message(&mut runtime, &report).await;
    assert_ack_for(&mut runtime, &report).await;
    drop(runtime);

    let session = gateway.await.expect("gateway task should finish");
    assert_eq!(session.config_report_count, 1);

    let store = store.lock().expect("store mutex should not be poisoned");
    let updated = store
        .release(release.release_id)
        .expect("release should still exist");
    assert_eq!(updated.status, ReleaseStatus::Applied);
    assert_eq!(updated.reported_version.as_deref(), Some("2026.06.27-010"));
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

async fn write_one_message(stream: &mut TcpStream, message: &EdgeLinkMessage) {
    let frame = encode_edgelink_frame(message).expect("message should encode");
    stream
        .write_all(&frame)
        .await
        .expect("message should be written");
}

async fn assert_ack_for(stream: &mut TcpStream, message: &EdgeLinkMessage) {
    let ack = read_one_message(stream).await;
    assert_eq!(ack.kind, EdgeLinkMessageKind::Ack);
    let EdgeLinkPayload::Ack(payload) = ack.payload else {
        panic!("expected ack payload");
    };
    assert_eq!(payload.ack_message_id, message.message_id);
    assert_eq!(payload.ack_sequence, message.sequence);
    assert!(payload.accepted);
}

fn runtime_metrics(edge_id: &str, runtime_id: &str) -> EdgeRuntimeMetricsSnapshot {
    EdgeRuntimeMetricsSnapshot {
        edge_id: edge_id.to_string(),
        runtime_id: runtime_id.to_string(),
        config_version: "2026.06.27-001".to_string(),
        timestamp: Utc::now(),
        health: EdgeHealth::Healthy,
        system: SystemRuntimeMetrics {
            cpu_percent: 22.0,
            memory_percent: 48.0,
            disk_percent: 61.0,
            process_uptime_seconds: 120,
        },
        collection: CollectionRuntimeMetrics {
            active_task_count: 1,
            success_rate: 0.99,
            average_latency_ms: 18,
            bad_point_count: 0,
        },
        protocols: vec![ProtocolRuntimeMetrics {
            connection_id: "modbus-main".to_string(),
            protocol: "Modbus TCP".to_string(),
            connected: true,
            latency_ms: 9,
            timeout_count: 1,
            error_count: 0,
            reconnect_count: 0,
        }],
        local_store: LocalStoreMetrics {
            backend: "rocksdb".to_string(),
            buffered_records: 3,
            oldest_buffer_age_seconds: 1,
            disk_usage_percent: 34.0,
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

fn discovery_report() -> DiscoveryReport {
    DiscoveryReport::new("job-1", "meter-rs485-bus-1")
        .with_point(
            DiscoveredPoint::new(
                "meter-rs485-bus-1",
                PointAddress::modbus_holding_register(40001),
                TelemetryType::Float,
            )
            .with_sample_values(vec!["220.1".to_string(), "220.3".to_string()])
            .with_confidence(0.72),
        )
        .with_suggestion(
            PointMappingSuggestion::new(
                "meter_voltage_a",
                "meter-1",
                "electric.voltage_a",
                "meter-rs485-bus-1",
                PointAddress::modbus_holding_register(40001),
                TelemetryType::Float,
            )
            .with_unit("V")
            .with_confidence(0.82)
            .with_evidence("数值范围和波动特征符合 A 相电压"),
        )
}
