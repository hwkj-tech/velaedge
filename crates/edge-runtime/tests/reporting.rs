use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Duration;

use async_trait::async_trait;
use edge_core::{
    CollectionTask, DeviceInstance, EdgeConfigPackage, EdgeRuntimeEvent,
    EdgeRuntimeMetricsSnapshot, PointAddress, ProtocolConnection, RuntimeEventCategory,
    RuntimeEventSeverity, TelemetryPointMapping, TelemetryType,
};
use edge_runtime::{
    report_runtime_status_once, report_runtime_status_with_store_once, AppliedEdgeConfig,
    HttpRuntimeStatusReporter, MqttPublishMessage, RocksEdgeRuntimeStore, RuntimeStatusReporter,
};
use tempfile::tempdir;

fn package() -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", "2026.06.26-003")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_point_mapping(TelemetryPointMapping::new(
            "pressure",
            "pump-1",
            "pressure",
            "sim-main",
            PointAddress::simulated("pressure"),
            TelemetryType::Float,
        ))
        .with_collection_task(CollectionTask::interval(
            "pump-main",
            "pump-1",
            vec!["pressure".to_string()],
            1000,
        ))
}

#[derive(Default)]
struct RecordingReporter {
    metrics: Vec<EdgeRuntimeMetricsSnapshot>,
    events: Vec<EdgeRuntimeEvent>,
}

#[async_trait]
impl RuntimeStatusReporter for RecordingReporter {
    async fn report_metrics(&mut self, snapshot: EdgeRuntimeMetricsSnapshot) -> anyhow::Result<()> {
        self.metrics.push(snapshot);
        Ok(())
    }

    async fn report_event(&mut self, event: EdgeRuntimeEvent) -> anyhow::Result<()> {
        self.events.push(event);
        Ok(())
    }
}

#[tokio::test]
async fn report_runtime_status_once_generates_and_uploads_snapshot_from_applied_config() {
    let applied = AppliedEdgeConfig::apply(package()).unwrap();
    let mut reporter = RecordingReporter::default();

    let snapshot = report_runtime_status_once("runtime-a", applied, &mut reporter)
        .await
        .unwrap();

    assert_eq!(snapshot.edge_id, "edge-dev");
    assert_eq!(snapshot.runtime_id, "runtime-a");
    assert_eq!(snapshot.config_version, "2026.06.26-003");
    assert_eq!(snapshot.collection.active_task_count, 1);
    assert_eq!(reporter.metrics, vec![snapshot]);
}

#[tokio::test]
async fn report_runtime_status_with_store_includes_pending_mqtt_outbox() {
    let applied = AppliedEdgeConfig::apply(package()).unwrap();
    let dir = tempdir().unwrap();
    let store = RocksEdgeRuntimeStore::open(dir.path().join("runtime.rocksdb")).unwrap();
    store
        .enqueue_mqtt_message(MqttPublishMessage {
            sink_id: "velamq-main".to_string(),
            broker: "mqtt://127.0.0.1:1883".to_string(),
            client_id: "edge-dev".to_string(),
            topic: "factory/edge-dev/status".to_string(),
            qos: 1,
            payload: b"{}".to_vec(),
        })
        .unwrap();
    let mut reporter = RecordingReporter::default();

    let snapshot =
        report_runtime_status_with_store_once("runtime-a", applied, &store, &mut reporter)
            .await
            .unwrap();

    assert_eq!(snapshot.local_store.buffered_records, 1);
    assert_eq!(snapshot.cloud_sync.pending_uploads, 1);
    assert_eq!(snapshot.health, edge_core::EdgeHealth::Degraded);
    assert_eq!(reporter.metrics, vec![snapshot]);
}

#[tokio::test]
async fn http_runtime_status_reporter_posts_metrics_and_events_to_cloud_api_paths() {
    let (base_url, received) = spawn_http_recorder(2);
    let applied = AppliedEdgeConfig::apply(package()).unwrap();
    let snapshot = report_runtime_status_once(
        "runtime-http",
        applied,
        &mut HttpRuntimeStatusReporter::new(&base_url).unwrap(),
    )
    .await
    .unwrap();

    let mut reporter = HttpRuntimeStatusReporter::new(&base_url).unwrap();
    let event = EdgeRuntimeEvent::new(
        "edge-dev",
        RuntimeEventSeverity::Warning,
        RuntimeEventCategory::Protocol,
        "simulated.warning",
        "simulated warning",
    );
    reporter.report_event(event).await.unwrap();

    let requests = received
        .recv_timeout(Duration::from_secs(2))
        .expect("requests should be recorded");

    assert!(requests[0].starts_with("POST /api/edges/edge-dev/runtime-metrics HTTP/1.1"));
    assert!(requests[0].contains("\"edge_id\":\"edge-dev\""));
    assert!(requests[0].contains("\"runtime_id\":\"runtime-http\""));
    assert!(requests[0].contains(&format!(
        "\"config_version\":\"{}\"",
        snapshot.config_version
    )));
    assert!(requests[1].starts_with("POST /api/edges/edge-dev/runtime-events HTTP/1.1"));
    assert!(requests[1].contains("\"code\":\"simulated.warning\""));
}

fn spawn_http_recorder(expected_requests: usize) -> (String, mpsc::Receiver<Vec<String>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let mut requests = Vec::new();
        for _ in 0..expected_requests {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 8192];
            let read = stream.read(&mut buffer).unwrap();
            requests.push(String::from_utf8_lossy(&buffer[..read]).to_string());
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
                .unwrap();
        }
        tx.send(requests).unwrap();
    });

    (base_url, rx)
}
