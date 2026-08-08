use std::sync::{Arc, Mutex};

use chrono::Utc;
use cloud_api::gateway::{
    handle_edgelink_tls_session, handle_edgelink_tls_session_with_store, EdgeGatewayTlsConfig,
};
use cloud_control::CloudControlStore;
use edge_core::{
    CloudSyncMetrics, CollectionRuntimeMetrics, EdgeHealth, EdgeRuntimeMetricsSnapshot,
    LocalStoreMetrics, SystemRuntimeMetrics,
};
use edge_runtime::{
    connect_edgelink_tls_once, publish_edgelink_runtime_status_tls_once, EdgeLinkClientTlsConfig,
    RocksEdgeRuntimeStore,
};
use tempfile::tempdir;
use tokio::net::TcpListener;

const CA_CERT: &str = include_str!("fixtures/edgelink/ca.pem");
const SERVER_CERT: &str = include_str!("fixtures/edgelink/server.pem");
const SERVER_KEY: &str = include_str!("fixtures/edgelink/server-key.pem");
const CLIENT_CERT: &str = include_str!("fixtures/edgelink/client.pem");
const CLIENT_KEY: &str = include_str!("fixtures/edgelink/client-key.pem");

#[test]
fn gateway_tls_config_rejects_invalid_certificate_material() {
    let error = match EdgeGatewayTlsConfig::from_pem(
        "not a server cert",
        "not a server key",
        "not a client ca cert",
    ) {
        Ok(_) => panic!("invalid gateway certificate material should fail"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("server certificate"));
}

#[tokio::test]
async fn runtime_and_gateway_complete_mutual_tls_edgelink_handshake() {
    let gateway_tls = EdgeGatewayTlsConfig::from_pem(SERVER_CERT, SERVER_KEY, CA_CERT).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, peer_addr) = listener.accept().await.unwrap();
        handle_edgelink_tls_session(stream, peer_addr, &gateway_tls)
            .await
            .unwrap()
    });
    let client_tls = EdgeLinkClientTlsConfig {
        ca_cert_pem: CA_CERT.to_string(),
        client_cert_pem: CLIENT_CERT.to_string(),
        client_key_pem: CLIENT_KEY.to_string(),
        server_name: "localhost".to_string(),
    };

    let report = connect_edgelink_tls_once(
        &address.to_string(),
        "edge-tls-1",
        "runtime-tls-1",
        "0.1.0",
        Some("v3".to_string()),
        &client_tls,
    )
    .await
    .unwrap();
    let session = server.await.unwrap();

    assert!(report.acked);
    assert_eq!(report.gateway_addr, address.to_string());
    assert_eq!(session.edge_id, "edge-tls-1");
    assert_eq!(session.runtime_id, "runtime-tls-1");
    assert_eq!(session.runtime_version, "0.1.0");
}

#[tokio::test]
async fn runtime_rejects_gateway_certificate_for_wrong_server_name() {
    let gateway_tls = EdgeGatewayTlsConfig::from_pem(SERVER_CERT, SERVER_KEY, CA_CERT).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, peer_addr) = listener.accept().await.unwrap();
        handle_edgelink_tls_session(stream, peer_addr, &gateway_tls).await
    });
    let client_tls = EdgeLinkClientTlsConfig {
        ca_cert_pem: CA_CERT.to_string(),
        client_cert_pem: CLIENT_CERT.to_string(),
        client_key_pem: CLIENT_KEY.to_string(),
        server_name: "wrong.example".to_string(),
    };

    let error = connect_edgelink_tls_once(
        &address.to_string(),
        "edge-tls-1",
        "runtime-tls-1",
        "0.1.0",
        None,
        &client_tls,
    )
    .await
    .unwrap_err();

    assert!(error
        .to_string()
        .contains("failed to open EdgeLink TLS session"));
    assert!(server.await.unwrap().is_err());
}

#[tokio::test]
async fn runtime_publishes_metrics_over_mutual_tls_and_receives_ack() {
    let gateway_tls = EdgeGatewayTlsConfig::from_pem(SERVER_CERT, SERVER_KEY, CA_CERT).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let cloud_store = Arc::new(Mutex::new(CloudControlStore::default()));
    let server_store = cloud_store.clone();
    let server = tokio::spawn(async move {
        let (stream, peer_addr) = listener.accept().await.unwrap();
        handle_edgelink_tls_session_with_store(stream, peer_addr, &gateway_tls, server_store)
            .await
            .unwrap()
    });
    let client_tls = EdgeLinkClientTlsConfig {
        ca_cert_pem: CA_CERT.to_string(),
        client_cert_pem: CLIENT_CERT.to_string(),
        client_key_pem: CLIENT_KEY.to_string(),
        server_name: "localhost".to_string(),
    };
    let runtime_dir = tempdir().unwrap();
    let runtime_store =
        RocksEdgeRuntimeStore::open(runtime_dir.path().join("runtime.rocksdb")).unwrap();

    let report = publish_edgelink_runtime_status_tls_once(
        &address.to_string(),
        "edge-tls-metrics",
        "runtime-tls-metrics",
        "0.1.0",
        runtime_metrics(),
        Vec::new(),
        &runtime_store,
        vec!["protocol:dlt645-2007".to_string()],
        None,
        false,
        &client_tls,
    )
    .await
    .unwrap();
    let gateway_report = server.await.unwrap();

    assert_eq!(report.acked_message_count, 1);
    assert_eq!(gateway_report.accepted_message_count, 1);
    assert_eq!(gateway_report.session.edge_id, "edge-tls-metrics");
    assert_eq!(
        gateway_report.session.capabilities,
        vec!["protocol:dlt645-2007".to_string()]
    );
    assert!(cloud_store
        .lock()
        .unwrap()
        .runtime_metrics("edge-tls-metrics")
        .is_some());
}

fn runtime_metrics() -> EdgeRuntimeMetricsSnapshot {
    EdgeRuntimeMetricsSnapshot {
        edge_id: "edge-tls-metrics".to_string(),
        runtime_id: "runtime-tls-metrics".to_string(),
        config_version: "v3".to_string(),
        timestamp: Utc::now(),
        health: EdgeHealth::Healthy,
        system: SystemRuntimeMetrics {
            cpu_percent: 12.0,
            memory_percent: 34.0,
            disk_percent: 56.0,
            process_uptime_seconds: 90,
        },
        collection: CollectionRuntimeMetrics {
            active_task_count: 1,
            success_rate: 1.0,
            average_latency_ms: 8,
            bad_point_count: 0,
        },
        protocols: Vec::new(),
        local_store: LocalStoreMetrics {
            backend: "rocksdb".to_string(),
            buffered_records: 0,
            oldest_buffer_age_seconds: 0,
            disk_usage_percent: 2.0,
        },
        algorithms: Vec::new(),
        mqtt: Default::default(),
        cloud_sync: CloudSyncMetrics {
            connected: true,
            last_sync_seconds_ago: 0,
            pending_uploads: 0,
            desired_version: "v3".to_string(),
            reported_version: "v3".to_string(),
        },
    }
}
