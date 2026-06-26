use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Duration;

use edge_core::{
    CollectionTask, DeviceInstance, EdgeConfigPackage, PointAddress, ProtocolConnection,
    TelemetryPointMapping, TelemetryType,
};
use edge_runtime::{EdgeConfigSyncClient, HttpEdgeConfigSyncClient};
use serde_json::json;

fn package() -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", "2026.06.26-004")
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

#[tokio::test]
async fn http_edge_config_sync_client_fetches_desired_config_and_reports_version() {
    let desired = json!({
        "edgeId": "edge-dev",
        "desiredVersion": "2026.06.26-004",
        "package": package(),
    })
    .to_string();
    let (base_url, received) = spawn_http_recorder(vec![desired, "{}".to_string()]);
    let mut client = HttpEdgeConfigSyncClient::new(&base_url).unwrap();

    let desired = client.fetch_desired_config("edge-dev").await.unwrap();
    client
        .report_applied_version("edge-dev", "2026.06.26-004")
        .await
        .unwrap();

    let requests = received
        .recv_timeout(Duration::from_secs(2))
        .expect("requests should be recorded");

    assert_eq!(desired.desired_version, "2026.06.26-004");
    assert_eq!(desired.package.edge_id, "edge-dev");
    assert!(requests[0].starts_with("GET /api/edges/edge-dev/desired-config HTTP/1.1"));
    assert!(requests[1].starts_with("POST /api/edges/edge-dev/reported-config HTTP/1.1"));
    assert!(requests[1].contains("\"reportedVersion\":\"2026.06.26-004\""));
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
