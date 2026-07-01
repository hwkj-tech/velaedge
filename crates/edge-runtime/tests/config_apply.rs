use edge_core::{
    CollectionTask, DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint,
    DataConfigPublish, DeviceInstance, EdgeConfigPackage, MqttUplinkConfig, PointAddress,
    ProtocolConnection, TelemetryPointMapping, TelemetryType, TelemetryValue,
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

#[test]
fn applying_config_rejects_data_configs_with_unknown_points() {
    let package = data_config_package().with_data_config(
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
            "missing_pressure",
            "pump.missing_pressure",
            PointAddress::simulated("missing_pressure"),
            TelemetryType::Float,
            "pressure",
        )),
    );

    let error = AppliedEdgeConfig::apply(package).expect_err("invalid data config is rejected");

    assert!(error
        .to_string()
        .contains("data config pump_status references missing point missing_pressure"));
}

#[test]
fn applying_config_rejects_duplicate_data_config_json_fields() {
    let package = data_config_package().with_data_config(
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
            "running",
            "pump.running",
            PointAddress::simulated("running"),
            TelemetryType::Boolean,
            "value",
        )),
    );

    let error = AppliedEdgeConfig::apply(package).expect_err("invalid data config is rejected");

    assert!(error
        .to_string()
        .contains("data config pump_status has duplicate json field value"));
}

#[test]
fn applying_config_rejects_collection_tasks_with_unknown_points() {
    let package = package().with_collection_task(CollectionTask::interval(
        "broken-task",
        "pump-1",
        vec!["missing_pressure".to_string()],
        1000,
    ));

    let error = AppliedEdgeConfig::apply(package).expect_err("invalid collection task is rejected");

    assert!(error
        .to_string()
        .contains("collection task broken-task references missing point missing_pressure"));
}

#[test]
fn applying_config_rejects_data_config_points_from_other_connections() {
    let package = data_config_package()
        .with_protocol_connection(ProtocolConnection::simulated("sim-secondary"))
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

    let error = AppliedEdgeConfig::apply(package).expect_err("invalid data config is rejected");

    assert!(error.to_string().contains(
        "data config pump_status point secondary_pressure uses protocol connection sim-secondary, expected sim-main"
    ));
}

fn data_config_package() -> EdgeConfigPackage {
    package()
        .with_mqtt_uplink(
            MqttUplinkConfig::velamq("velamq-main", "mqtt://velamq.local:1883", "edge-dev")
                .with_topic_template("factory/{edge_id}/{device_id}/telemetry"),
        )
        .with_point_mapping(TelemetryPointMapping::new(
            "running",
            "pump-1",
            "running",
            "sim-main",
            PointAddress::simulated("running"),
            TelemetryType::Boolean,
        ))
}
