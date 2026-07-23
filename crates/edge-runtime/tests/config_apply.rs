use edge_core::{
    CollectionTask, DataConfig, DataConfigCollection, DataConfigGraphEdge, DataConfigGraphNode,
    DataConfigGraphNodeKind, DataConfigPayload, DataConfigPoint, DataConfigPublish,
    DataConfigVisualGraph, DeviceInstance, EdgeConfigPackage, MqttUplinkConfig, PointAddress,
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

#[test]
fn applying_config_accepts_multiple_connected_mqtt_outputs() {
    let mut data_config = data_config_with_pressure();
    data_config.visual_graph = DataConfigVisualGraph {
        nodes: vec![
            graph_node("point-pressure", DataConfigGraphNodeKind::Point, "pressure"),
            graph_node(
                "mqtt-primary",
                DataConfigGraphNodeKind::Mqtt,
                "primary/topic",
            ),
            graph_node(
                "mqtt-secondary",
                DataConfigGraphNodeKind::Mqtt,
                "secondary/topic",
            ),
        ],
        edges: vec![
            graph_edge("point-primary", "point-pressure", "mqtt-primary"),
            graph_edge("point-secondary", "point-pressure", "mqtt-secondary"),
        ],
    };

    AppliedEdgeConfig::apply(data_config_package().with_data_config(data_config))
        .expect("connected multi-output graph is accepted");
}

#[test]
fn applying_config_rejects_a_disconnected_mqtt_output() {
    let mut data_config = data_config_with_pressure();
    data_config.visual_graph = DataConfigVisualGraph {
        nodes: vec![
            graph_node("point-pressure", DataConfigGraphNodeKind::Point, "pressure"),
            graph_node(
                "mqtt-primary",
                DataConfigGraphNodeKind::Mqtt,
                "primary/topic",
            ),
        ],
        edges: Vec::new(),
    };

    let error = AppliedEdgeConfig::apply(data_config_package().with_data_config(data_config))
        .expect_err("disconnected output is rejected");

    assert!(error
        .to_string()
        .contains("graph MQTT output mqtt-primary is disconnected"));
}

fn data_config_with_pressure() -> DataConfig {
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
        "pressure",
    ))
}

fn graph_node(node_id: &str, kind: DataConfigGraphNodeKind, ref_id: &str) -> DataConfigGraphNode {
    DataConfigGraphNode {
        node_id: node_id.to_string(),
        kind,
        label: node_id.to_string(),
        ref_id: Some(ref_id.to_string()),
        params: Default::default(),
        x: 0,
        y: 0,
    }
}

fn graph_edge(edge_id: &str, from: &str, to: &str) -> DataConfigGraphEdge {
    DataConfigGraphEdge {
        edge_id: edge_id.to_string(),
        from: from.to_string(),
        from_port: Some("value".to_string()),
        to: to.to_string(),
        to_port: Some("payload".to_string()),
    }
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
