use cloud_control::{
    AuditAction, CloudControlStore, ConfigValidator, ReleaseService, ReleaseStatus,
};
use edge_core::{
    CollectionTask, DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint,
    DataConfigPublish, DeviceInstance, DeviceSpec, EdgeConfigPackage, PointAddress,
    ProtocolConnection, TelemetryPoint, TelemetryPointMapping, TelemetryType,
};

fn valid_package() -> EdgeConfigPackage {
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

#[test]
fn store_keeps_edges_models_and_config_packages() {
    let mut store = CloudControlStore::default();
    let model = DeviceSpec::new("pump", "1.0.0")
        .with_telemetry(vec![TelemetryPoint::new("pressure", TelemetryType::Float)]);

    store.upsert_device_model(model.clone());
    store.upsert_config_package(valid_package());

    assert_eq!(store.device_model("pump").unwrap(), &model);
    assert_eq!(
        store
            .config_package("edge-dev", "2026.06.26-001")
            .unwrap()
            .edge_id,
        "edge-dev"
    );
}

#[test]
fn validator_rejects_point_mapping_with_missing_connection() {
    let mut package = valid_package();
    package.point_mappings[0].protocol_connection_id = "missing".to_string();

    let errors = ConfigValidator::validate_package(&package);

    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("missing protocol connection"));
}

#[test]
fn validator_rejects_data_config_with_missing_mqtt_sink() {
    let package = EdgeConfigPackage::new("edge-dev", "v1")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-line"))
        .with_data_config(DataConfig::new(
            "pump_status",
            "泵运行状态上报",
            "pump-1",
            "sim-line",
            DataConfigCollection::new(1000),
            DataConfigPublish::new(
                "missing-sink",
                "factory/{edge_id}/{device_id}/status",
                DataConfigPayload::object(),
            ),
        ));

    let errors = ConfigValidator::validate_package(&package);

    assert_eq!(errors.len(), 2);
    assert!(errors
        .iter()
        .any(|error| error.message.contains("missing-sink")));
}

#[test]
fn validator_rejects_data_config_with_missing_point_and_duplicate_json_field() {
    let package = valid_package()
        .with_mqtt_uplink(edge_core::MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://velamq.local:1883",
            "edge-dev",
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "running",
            "pump-1",
            "running",
            "sim-main",
            PointAddress::simulated("running"),
            TelemetryType::Boolean,
        ))
        .with_data_config(
            DataConfig::new(
                "pump_status",
                "泵状态上报",
                "pump-1",
                "sim-main",
                DataConfigCollection::new(1000),
                DataConfigPublish::new(
                    "velamq-main",
                    "factory/{edge_id}/{device_id}/status",
                    DataConfigPayload::object(),
                ),
            )
            .with_point(DataConfigPoint::new(
                "pressure",
                "pump.pressure",
                PointAddress::simulated("pressure"),
                TelemetryType::Float,
                "value",
            ))
            .with_point(DataConfigPoint::new(
                "missing_point",
                "pump.missing",
                PointAddress::simulated("missing_point"),
                TelemetryType::Float,
                "value",
            )),
        );

    let errors = ConfigValidator::validate_package(&package);

    assert!(errors
        .iter()
        .any(|error| error.message.contains("missing point `missing_point`")));
    assert!(errors
        .iter()
        .any(|error| error.message.contains("duplicate json field `value`")));
}

#[test]
fn validator_rejects_collection_task_with_missing_point() {
    let package = valid_package().with_collection_task(CollectionTask::interval(
        "broken-task",
        "pump-1",
        vec!["missing_pressure".to_string()],
        1000,
    ));

    let errors = ConfigValidator::validate_package(&package);

    assert!(errors.iter().any(|error| error
        .message
        .contains("collection task `broken-task` references missing point `missing_pressure`")));
}

#[test]
fn validator_rejects_data_config_point_from_other_connection() {
    let package = valid_package()
        .with_protocol_connection(ProtocolConnection::simulated("sim-secondary"))
        .with_mqtt_uplink(edge_core::MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://velamq.local:1883",
            "edge-dev",
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "secondary_pressure",
            "pump-1",
            "pressure",
            "sim-secondary",
            PointAddress::simulated("secondary_pressure"),
            TelemetryType::Float,
        ))
        .with_data_config(
            DataConfig::new(
                "pump_status",
                "泵状态上报",
                "pump-1",
                "sim-main",
                DataConfigCollection::new(1000),
                DataConfigPublish::new(
                    "velamq-main",
                    "factory/{edge_id}/{device_id}/status",
                    DataConfigPayload::object(),
                ),
            )
            .with_point(DataConfigPoint::new(
                "secondary_pressure",
                "pump.secondary_pressure",
                PointAddress::simulated("secondary_pressure"),
                TelemetryType::Float,
                "pressure",
            )),
        );

    let errors = ConfigValidator::validate_package(&package);

    assert!(errors.iter().any(|error| error.message.contains(
        "data config `pump_status` point `secondary_pressure` uses protocol connection `sim-secondary`, expected `sim-main`"
    )));
}

#[test]
fn release_service_tracks_desired_and_reported_versions() {
    let mut store = CloudControlStore::default();
    let release = ReleaseService::create_release(&mut store, valid_package()).unwrap();

    assert_eq!(release.edge_id, "edge-dev");
    assert_eq!(release.desired_version, "2026.06.26-001");
    assert_eq!(release.status, ReleaseStatus::Pending);

    let applied =
        ReleaseService::mark_reported(&mut store, release.release_id, "2026.06.26-001").unwrap();

    assert_eq!(applied.reported_version.as_deref(), Some("2026.06.26-001"));
    assert_eq!(applied.status, ReleaseStatus::Applied);
    assert_eq!(store.audit_records()[0].action, AuditAction::CreateRelease);
}
