use edge_core::{
    CollectionTask, DeviceInstance, EdgeConfigPackage, PointAddress, ProtocolConnection,
    TelemetryPointMapping, TelemetryType, TelemetryValue,
};
use edge_runtime::{AppliedEdgeConfig, ConfiguredSimulatedRuntime};

fn package() -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", "2026.06.26-001")
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
async fn applying_config_reports_version_and_collects_named_points() {
    let applied = AppliedEdgeConfig::apply(package()).unwrap();
    let mut runtime = ConfiguredSimulatedRuntime::new(applied);

    let report = runtime.collect_once().await.unwrap();

    assert_eq!(runtime.reported_version(), "2026.06.26-001");
    assert_eq!(report.samples_collected, 1);
    assert_eq!(
        runtime.shadow("pump-1").unwrap().latest_value("pressure"),
        Some(&TelemetryValue::Float(1.0))
    );
}
