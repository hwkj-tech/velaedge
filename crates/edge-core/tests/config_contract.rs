use edge_core::{
    CollectionTask, DeviceInstance, EdgeConfigPackage, NumberRange, PointAddress,
    ProtocolConnection, ProtocolType, TelemetryPointMapping, TelemetryType,
};

#[test]
fn config_package_contains_edge_targets_and_point_mappings() {
    let package = EdgeConfigPackage::new("edge-dev", "2026.06.26-001")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_point_mapping(
            TelemetryPointMapping::new(
                "pressure",
                "pump-1",
                "pressure",
                "sim-main",
                PointAddress::simulated("pressure"),
                TelemetryType::Float,
            )
            .with_unit("MPa")
            .with_range(NumberRange::new(0.0, 20.0))
            .with_interval_ms(1000),
        )
        .with_collection_task(CollectionTask::interval(
            "pump-main-collection",
            "pump-1",
            vec!["pressure".to_string()],
            1000,
        ));

    assert_eq!(package.edge_id, "edge-dev");
    assert_eq!(package.version, "2026.06.26-001");
    assert_eq!(package.point_mappings[0].point_id, "pressure");
    assert_eq!(
        package.protocol_connections[0].protocol,
        ProtocolType::Simulated
    );
    assert_eq!(package.collection_tasks[0].point_ids, vec!["pressure"]);
}

#[test]
fn modbus_point_address_preserves_register_metadata() {
    let address = PointAddress::modbus_holding_register(40001);

    assert_eq!(address.kind, "holding_register");
    assert_eq!(address.value, "40001");
}
