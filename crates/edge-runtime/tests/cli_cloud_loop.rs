use std::io::{Read, Write};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use edge_core::{
    CollectionTask, DeviceInstance, EdgeConfigPackage, PointAddress, ProtocolConnection,
    TelemetryPointMapping, TelemetryType,
};
use serde_json::json;

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

#[test]
fn edge_runtime_cli_can_run_cloud_config_sync_and_runtime_reporting_once() {
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
    assert!(requests[2].contains("\"runtime_id\":\"runtime-cli\""));
    assert!(requests[2].contains("\"config_version\":\"2026.06.26-005\""));
}

#[test]
fn edge_runtime_cli_can_run_scheduled_collection_ticks_from_cloud_config() {
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
