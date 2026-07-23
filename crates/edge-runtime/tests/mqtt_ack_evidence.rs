use std::process::Command;

use edge_runtime::{MqttPublishMessage, RocksEdgeRuntimeStore};
use tempfile::tempdir;

#[test]
fn mqtt_ack_evidence_cli_exports_only_acknowledged_route_metadata() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("runtime.rocksdb");
    {
        let store = RocksEdgeRuntimeStore::open(&db_path).unwrap();
        let sequence = store
            .enqueue_mqtt_message(MqttPublishMessage {
                sink_id: "primary".to_string(),
                broker: "mqtts://velamq.example:8883".to_string(),
                client_id: "edge-field".to_string(),
                topic: "factory/edge-field/telemetry".to_string(),
                qos: 1,
                payload: br#"{"pressure":1.25}"#.to_vec(),
            })
            .unwrap();
        store.acknowledge_mqtt_message(sequence).unwrap();
    }

    let output = Command::new(env!("CARGO_BIN_EXE_mqtt-ack-evidence"))
        .args(["--runtime-db", db_path.to_str().unwrap(), "--limit", "10"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "export failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["receiptCount"], 1);
    assert_eq!(report["acknowledgements"][0]["sequence"], 1);
    assert_eq!(report["acknowledgements"][0]["sinkId"], "primary");
    assert_eq!(
        report["acknowledgements"][0]["topic"],
        "factory/edge-field/telemetry"
    );
    assert_eq!(report["acknowledgements"][0]["qos"], 1);
    assert_eq!(report["acknowledgements"][0]["payloadBytes"], 17);
    assert!(report["acknowledgements"][0].get("payload").is_none());
}
