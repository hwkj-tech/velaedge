use edge_core::{
    CollectionTask, DeviceInstance, EdgeConfigPackage, EdgeHealth, PointAddress,
    ProtocolConnection, TelemetryPointMapping, TelemetryType,
};
use edge_runtime::{AppliedEdgeConfig, SimulatedRuntimeMetricsCollector};

fn package() -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", "2026.06.26-002")
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
fn simulated_metrics_collector_reports_runtime_health_from_applied_config() {
    let applied = AppliedEdgeConfig::apply(package()).unwrap();
    let collector = SimulatedRuntimeMetricsCollector::new("runtime-a", applied);

    let snapshot = collector.snapshot();

    assert_eq!(snapshot.edge_id, "edge-dev");
    assert_eq!(snapshot.runtime_id, "runtime-a");
    assert_eq!(snapshot.config_version, "2026.06.26-002");
    assert_eq!(snapshot.health, EdgeHealth::Healthy);
    assert_eq!(snapshot.collection.active_task_count, 1);
    assert_eq!(snapshot.protocols[0].connection_id, "sim-main");
    assert!(snapshot.protocols[0].connected);
    assert!(snapshot.cloud_sync.connected);
    assert_eq!(snapshot.cloud_sync.reported_version, "2026.06.26-002");
}
