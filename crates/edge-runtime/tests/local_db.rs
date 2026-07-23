use edge_core::{CollectionTask, DeviceInstance, EdgeConfigPackage, ProtocolConnection};
use edge_runtime::{MqttPublishMessage, RocksEdgeRuntimeStore};
use tempfile::tempdir;

#[test]
fn rocks_store_persists_desired_and_active_config_across_reopen() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("runtime.rocksdb");
    let package = EdgeConfigPackage::new("edge-dev", "2026.06.27-020")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"));

    {
        let store = RocksEdgeRuntimeStore::open(&db_path).unwrap();
        store.put_desired_config(&package).unwrap();
        store
            .promote_active_config("edge-dev", "2026.06.27-020")
            .unwrap();
    }

    let reopened = RocksEdgeRuntimeStore::open(&db_path).unwrap();
    let desired = reopened
        .desired_config("edge-dev", "2026.06.27-020")
        .unwrap()
        .expect("desired config should survive reopen");
    assert_eq!(desired.version, "2026.06.27-020");

    let active = reopened
        .active_config("edge-dev")
        .unwrap()
        .expect("active config should survive reopen");
    assert_eq!(active.edge_id, "edge-dev");
    assert_eq!(active.version, "2026.06.27-020");
    assert_eq!(active.protocol_connections[0].connection_id, "sim-main");

    let recovered = reopened
        .recover_active_config("edge-dev")
        .unwrap()
        .expect("active config should be validated and recovered");
    assert_eq!(recovered.version(), "2026.06.27-020");
}

#[test]
fn rocks_store_rejects_promoting_missing_config() {
    let dir = tempdir().unwrap();
    let store = RocksEdgeRuntimeStore::open(dir.path().join("runtime.rocksdb")).unwrap();

    let error = store
        .promote_active_config("edge-dev", "missing-version")
        .expect_err("missing config should not become active");

    assert!(error.to_string().contains("desired config not found"));
}

#[test]
fn rocks_store_creates_missing_parent_directories() {
    let dir = tempdir().unwrap();
    let db_path = dir
        .path()
        .join("missing")
        .join("nested")
        .join("runtime.rocksdb");

    RocksEdgeRuntimeStore::open(&db_path).unwrap();

    assert!(db_path.exists());
}

#[test]
fn rocks_store_rejects_an_invalid_active_config_during_recovery() {
    let dir = tempdir().unwrap();
    let store = RocksEdgeRuntimeStore::open(dir.path().join("runtime.rocksdb")).unwrap();
    let package = EdgeConfigPackage::new("edge-dev", "invalid-active").with_collection_task(
        CollectionTask::interval(
            "task-1",
            "missing-device",
            vec!["point-1".to_string()],
            1000,
        ),
    );
    store.put_desired_config(&package).unwrap();
    store
        .promote_active_config("edge-dev", "invalid-active")
        .unwrap();

    let error = store
        .recover_active_config("edge-dev")
        .expect_err("invalid active config must not be recovered");

    assert!(error
        .to_string()
        .contains("failed to validate active config for edge `edge-dev`"));
}

#[test]
fn rocks_store_retains_only_the_latest_mqtt_acknowledgements() {
    let dir = tempdir().unwrap();
    let store = RocksEdgeRuntimeStore::open(dir.path().join("runtime.rocksdb")).unwrap();

    for index in 0..1_002 {
        let sequence = store
            .enqueue_mqtt_message(MqttPublishMessage {
                sink_id: "velamq-main".to_string(),
                broker: "mqtts://velamq.example:8883".to_string(),
                client_id: "edge-retention".to_string(),
                topic: format!("factory/edge-retention/branch/{}", index % 2),
                qos: 1,
                payload: vec![index as u8],
            })
            .unwrap();
        store.acknowledge_mqtt_message(sequence).unwrap();
    }

    let acknowledgements = store.mqtt_publish_acknowledgements(2_000).unwrap();
    assert_eq!(acknowledgements.len(), 1_000);
    assert_eq!(acknowledgements.first().unwrap().sequence, 3);
    assert_eq!(acknowledgements.last().unwrap().sequence, 1_002);
    assert_eq!(store.mqtt_outbox_len().unwrap(), 0);
}
