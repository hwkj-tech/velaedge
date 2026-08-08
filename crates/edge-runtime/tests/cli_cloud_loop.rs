use std::io::{Read, Write};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use edge_core::{
    CollectionTask, DeviceInstance, EdgeConfigPackage, PointAddress, ProtocolConnection,
    ProtocolType, TelemetryPointMapping, TelemetryType,
};
use serde_json::json;
use tempfile::tempdir;

fn package() -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", "2026.06.26-005")
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

fn package_with_one_failing_collection_task() -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", "2026.06.26-006")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_device(DeviceInstance::new("pump-2", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_protocol_connection(ProtocolConnection {
            connection_id: "unsupported-main".to_string(),
            protocol: ProtocolType::ModbusTcp,
            endpoint: Some("127.0.0.1:502".to_string()),
            serial: None,
            iec101: None,
            iec104: None,
            opc_ua: None,
            bacnet_ip: None,
            siemens_s7: None,
            omron_fins: None,
            circuit_breaker: Default::default(),
        })
        .with_point_mapping(TelemetryPointMapping::new(
            "pressure",
            "pump-1",
            "pressure",
            "sim-main",
            PointAddress::simulated("pressure"),
            TelemetryType::Float,
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "flow",
            "pump-2",
            "flow",
            "unsupported-main",
            PointAddress::modbus_holding_register(40001),
            TelemetryType::Integer,
        ))
        .with_collection_task(CollectionTask::interval(
            "good-task",
            "pump-1",
            vec!["pressure".to_string()],
            1000,
        ))
        .with_collection_task(CollectionTask::interval(
            "bad-task",
            "pump-2",
            vec!["flow".to_string()],
            1000,
        ))
}

#[test]
fn edge_runtime_cli_can_run_cloud_config_sync_and_runtime_reporting_once() {
    let dir = tempdir().unwrap();
    let runtime_db = dir.path().join("runtime.rocksdb");
    let desired = json!({
        "edgeId": "edge-dev",
        "desiredVersion": "2026.06.26-005",
        "package": package(),
    })
    .to_string();
    let (base_url, received) =
        spawn_http_recorder(vec![desired, "{}".to_string(), "{}".to_string()]);

    let output = Command::new(env!("CARGO_BIN_EXE_edge-runtime"))
        .args([
            "--edge-id",
            "edge-dev",
            "--runtime-id",
            "runtime-cli",
            "--cloud-api-url",
            &base_url,
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

    let requests = received
        .recv_timeout(Duration::from_secs(2))
        .expect("requests should be recorded");

    assert!(requests[0].starts_with("GET /api/edges/edge-dev/desired-config HTTP/1.1"));
    assert!(requests[1].starts_with("POST /api/edges/edge-dev/reported-config HTTP/1.1"));
    assert!(requests[1].contains("\"desiredVersion\":\"2026.06.26-005\""));
    assert!(requests[1].contains("\"reportedVersion\":\"2026.06.26-005\""));
    assert!(requests[2].starts_with("POST /api/edges/edge-dev/runtime-metrics HTTP/1.1"));
    assert!(requests[2].contains("\"runtime_id\":\"runtime-cli\""));
    assert!(requests[2].contains("\"config_version\":\"2026.06.26-005\""));
}

#[test]
fn edge_runtime_cli_can_run_scheduled_collection_ticks_from_cloud_config() {
    let dir = tempdir().unwrap();
    let runtime_db = dir.path().join("runtime.rocksdb");
    let desired = json!({
        "edgeId": "edge-dev",
        "desiredVersion": "2026.06.26-005",
        "package": package(),
    })
    .to_string();
    let (base_url, received) =
        spawn_http_recorder(vec![desired, "{}".to_string(), "{}".to_string()]);

    let output = Command::new(env!("CARGO_BIN_EXE_edge-runtime"))
        .args([
            "--edge-id",
            "edge-dev",
            "--runtime-id",
            "runtime-cli",
            "--cloud-api-url",
            &base_url,
            "--scheduled-ticks",
            "2",
            "--scheduler-tick-ms",
            "1000",
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

    let requests = received
        .recv_timeout(Duration::from_secs(2))
        .expect("requests should be recorded");

    assert!(requests[0].starts_with("GET /api/edges/edge-dev/desired-config HTTP/1.1"));
    assert!(requests[1].starts_with("POST /api/edges/edge-dev/reported-config HTTP/1.1"));
    assert!(requests[2].starts_with("POST /api/edges/edge-dev/runtime-metrics HTTP/1.1"));
}

#[test]
fn edge_runtime_cli_reports_collection_failure_events_during_scheduled_ticks() {
    let dir = tempdir().unwrap();
    let runtime_db = dir.path().join("runtime.rocksdb");
    let desired = json!({
        "edgeId": "edge-dev",
        "desiredVersion": "2026.06.26-006",
        "package": package_with_one_failing_collection_task(),
    })
    .to_string();
    let (base_url, received) = spawn_http_recorder(vec![
        desired,
        "{}".to_string(),
        "{}".to_string(),
        "{}".to_string(),
    ]);

    let output = Command::new(env!("CARGO_BIN_EXE_edge-runtime"))
        .args([
            "--edge-id",
            "edge-dev",
            "--runtime-id",
            "runtime-cli",
            "--cloud-api-url",
            &base_url,
            "--scheduled-ticks",
            "1",
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

    let requests = received
        .recv_timeout(Duration::from_secs(2))
        .expect("requests should be recorded");

    assert!(requests[2].starts_with("POST /api/edges/edge-dev/runtime-metrics HTTP/1.1"));
    assert!(requests[2].contains("\"success_rate\":0.5"));
    assert!(requests[3].starts_with("POST /api/edges/edge-dev/runtime-events HTTP/1.1"));
    assert!(requests[3].contains("\"code\":\"collection.task_failed\""));
    assert!(requests[3].contains("\"task_id\":\"bad-task\""));
}

fn spawn_http_recorder(responses: Vec<String>) -> (String, mpsc::Receiver<Vec<String>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let mut requests = Vec::new();
        for response_body in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 8192];
            let read = stream.read(&mut buffer).unwrap();
            requests.push(String::from_utf8_lossy(&buffer[..read]).to_string());
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
        tx.send(requests).unwrap();
    });

    (base_url, rx)
}
