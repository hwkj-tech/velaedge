use edge_core::{
    CollectionTask, CommandFlowConfig, CommandGraphEdge, CommandGraphNode, CommandGraphNodeKind,
    DataConfig, DataConfigCollection, DataConfigGraphEdge, DataConfigGraphNode,
    DataConfigGraphNodeKind, DataConfigPayload, DataConfigPoint, DataConfigPublish,
    DataConfigVisualGraph, DeviceInstance, EdgeConfigPackage, MqttUplinkConfig, PointAccess,
    PointAddress, ProtocolCircuitBreakerConfig, ProtocolConnection, TelemetryPointMapping,
    TelemetryType, TelemetryValue,
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
fn applying_config_rejects_unsafe_data_config_recovery_limits() {
    let package = data_config_package().with_data_config(
        DataConfig::new(
            "pump_status",
            "泵状态上报",
            "pump-1",
            "sim-main",
            DataConfigCollection::new(1000)
                .with_timeout_ms(edge_core::MAX_DATA_CONFIG_TIMEOUT_MS + 1)
                .with_retry_count(edge_core::MAX_DATA_CONFIG_RETRY_COUNT + 1),
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
        )),
    );

    let error = AppliedEdgeConfig::apply(package).expect_err("unsafe recovery limits are rejected");

    assert!(error.to_string().contains("collection timeout"));
}

#[test]
fn applying_config_rejects_unsafe_protocol_circuit_breaker_limits() {
    let mut package = package();
    package.protocol_connections[0].circuit_breaker = ProtocolCircuitBreakerConfig {
        failure_threshold: 0,
        ..Default::default()
    };

    let error = AppliedEdgeConfig::apply(package).expect_err("unsafe breaker is rejected");

    assert!(error.to_string().contains("failure threshold"));
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
fn applying_config_rejects_writable_modbus_input_areas() {
    let mut package = package();
    package.point_mappings[0].address = PointAddress {
        kind: "input_register".to_string(),
        value: "30001".to_string(),
        modbus: None,
    };
    package.point_mappings[0].access = PointAccess::ReadWrite;

    let error = AppliedEdgeConfig::apply(package).expect_err("unsafe point access is rejected");

    assert!(error
        .to_string()
        .contains("input_register points are protocol-level read-only"));
}

#[test]
fn applying_config_accepts_a_branched_command_flow_for_writable_points() {
    let package = package()
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://127.0.0.1:1883",
            "edge-dev",
        ))
        .with_point_mapping(
            TelemetryPointMapping::new(
                "start_command",
                "pump-1",
                "pump.start",
                "sim-main",
                PointAddress::simulated("start"),
                TelemetryType::Boolean,
            )
            .with_access(PointAccess::WriteOnly),
        )
        .with_point_mapping(
            TelemetryPointMapping::new(
                "pressure_setpoint",
                "pump-1",
                "pump.pressure_setpoint",
                "sim-main",
                PointAddress::simulated("pressure_setpoint"),
                TelemetryType::Float,
            )
            .with_access(PointAccess::ReadWrite),
        )
        .with_command_flow(branched_command_flow());

    AppliedEdgeConfig::apply(package).expect("branched command flow is accepted");
}

#[test]
fn applying_config_rejects_command_flow_for_another_protocol_connection() {
    let package = package()
        .with_protocol_connection(ProtocolConnection::simulated("sim-secondary"))
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://127.0.0.1:1883",
            "edge-dev",
        ))
        .with_point_mapping(
            TelemetryPointMapping::new(
                "start_command",
                "pump-1",
                "pump.start",
                "sim-secondary",
                PointAddress::simulated("start"),
                TelemetryType::Boolean,
            )
            .with_access(PointAccess::WriteOnly),
        )
        .with_command_flow(
            CommandFlowConfig::new(
                "pump-command",
                "泵控制",
                "velamq-main",
                "factory/pump/command",
                "factory/pump/reply/{command_id}",
            )
            .with_protocol_connection("sim-main")
            .with_node(CommandGraphNode::new(
                "mqtt-input",
                CommandGraphNodeKind::MqttInput,
                "MQTT 指令",
            ))
            .with_node(
                CommandGraphNode::new("point-write", CommandGraphNodeKind::PointWrite, "写入点位")
                    .with_ref("start_command"),
            )
            .with_node(CommandGraphNode::new(
                "mqtt-reply",
                CommandGraphNodeKind::MqttReply,
                "MQTT 回执",
            ))
            .with_edge(CommandGraphEdge::new(
                "input-write",
                "mqtt-input",
                "point-write",
            ))
            .with_edge(CommandGraphEdge::new(
                "write-reply",
                "point-write",
                "mqtt-reply",
            )),
        );

    let error =
        AppliedEdgeConfig::apply(package).expect_err("cross-connection command target is rejected");

    assert!(error.to_string().contains(
        "write node point-write uses protocol connection sim-secondary, expected sim-main"
    ));
}

#[test]
fn applying_config_rejects_command_flow_with_missing_protocol_connection() {
    let package = package()
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://127.0.0.1:1883",
            "edge-dev",
        ))
        .with_command_flow(branched_command_flow().with_protocol_connection("missing-connection"));

    let error = AppliedEdgeConfig::apply(package)
        .expect_err("missing command protocol connection is rejected");

    assert!(error.to_string().contains(
        "command flow pump-command references missing protocol connection missing-connection"
    ));
}

#[test]
fn applying_config_rejects_an_invalid_command_value_path() {
    let mut flow = branched_command_flow();
    flow.nodes
        .iter_mut()
        .find(|node| node.node_id == "write-pressure")
        .unwrap()
        .params
        .insert(
            "value_path".to_string(),
            serde_json::json!("payload..pressure"),
        );
    let package = package()
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://127.0.0.1:1883",
            "edge-dev",
        ))
        .with_point_mapping(
            TelemetryPointMapping::new(
                "start_command",
                "pump-1",
                "pump.start",
                "sim-main",
                PointAddress::simulated("start"),
                TelemetryType::Boolean,
            )
            .with_access(PointAccess::WriteOnly),
        )
        .with_point_mapping(
            TelemetryPointMapping::new(
                "pressure_setpoint",
                "pump-1",
                "pump.pressure_setpoint",
                "sim-main",
                PointAddress::simulated("pressure_setpoint"),
                TelemetryType::Float,
            )
            .with_access(PointAccess::ReadWrite),
        )
        .with_command_flow(flow);

    let error = AppliedEdgeConfig::apply(package).expect_err("invalid value path is rejected");

    assert!(error
        .to_string()
        .contains("value_path must be a non-empty dot-separated JSON field path"));
}

#[test]
fn applying_config_rejects_command_flow_targeting_a_read_only_point() {
    let package = package()
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://127.0.0.1:1883",
            "edge-dev",
        ))
        .with_command_flow(
            CommandFlowConfig::new(
                "pump-command",
                "泵控制",
                "velamq-main",
                "factory/pump/command",
                "factory/pump/reply/{command_id}",
            )
            .with_node(CommandGraphNode::new(
                "mqtt-input",
                CommandGraphNodeKind::MqttInput,
                "MQTT 指令",
            ))
            .with_node(
                CommandGraphNode::new("write-pressure", CommandGraphNodeKind::PointWrite, "写压力")
                    .with_ref("pressure"),
            )
            .with_node(CommandGraphNode::new(
                "mqtt-reply",
                CommandGraphNodeKind::MqttReply,
                "MQTT 回执",
            ))
            .with_edge(CommandGraphEdge::new(
                "input-write",
                "mqtt-input",
                "write-pressure",
            ))
            .with_edge(CommandGraphEdge::new(
                "write-reply",
                "write-pressure",
                "mqtt-reply",
            )),
        );

    let error =
        AppliedEdgeConfig::apply(package).expect_err("read-only command target is rejected");

    assert!(error
        .to_string()
        .contains("write node write-pressure references read-only point pressure"));
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

fn branched_command_flow() -> CommandFlowConfig {
    CommandFlowConfig::new(
        "pump-command",
        "泵控制",
        "velamq-main",
        "factory/pump/command",
        "factory/pump/reply/{command_id}",
    )
    .with_node(CommandGraphNode::new(
        "mqtt-input",
        CommandGraphNodeKind::MqttInput,
        "MQTT 指令",
    ))
    .with_node(CommandGraphNode::new(
        "condition",
        CommandGraphNodeKind::Condition,
        "按命令路由",
    ))
    .with_node(
        CommandGraphNode::new("write-pressure", CommandGraphNodeKind::PointWrite, "写压力")
            .with_ref("pressure_setpoint"),
    )
    .with_node(
        CommandGraphNode::new("write-start", CommandGraphNodeKind::PointWrite, "启动泵")
            .with_ref("start_command"),
    )
    .with_node(CommandGraphNode::new(
        "mqtt-reply",
        CommandGraphNodeKind::MqttReply,
        "MQTT 回执",
    ))
    .with_edge(CommandGraphEdge::new(
        "input-condition",
        "mqtt-input",
        "condition",
    ))
    .with_edge(
        CommandGraphEdge::new("condition-pressure", "condition", "write-pressure")
            .with_ports("setpoint", "value"),
    )
    .with_edge(
        CommandGraphEdge::new("condition-start", "condition", "write-start")
            .with_ports("start", "value"),
    )
    .with_edge(CommandGraphEdge::new(
        "pressure-reply",
        "write-pressure",
        "mqtt-reply",
    ))
    .with_edge(CommandGraphEdge::new(
        "start-reply",
        "write-start",
        "mqtt-reply",
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
