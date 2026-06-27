use edge_core::{DeviceInstance, EdgeConfigPackage, ProtocolConnection};
use edge_runtime::RocksEdgeRuntimeStore;
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
