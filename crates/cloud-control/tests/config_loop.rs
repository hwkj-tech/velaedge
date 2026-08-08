use cloud_control::{
    AuditAction, CloudControlStore, ConfigValidator, ReleaseService, ReleaseStatus,
};
use edge_core::{
    BacnetIpConnectionSettings, CollectionTask, CommandFlowConfig, CommandGraphEdge,
    CommandGraphNode, CommandGraphNodeKind, DataConfig, DataConfigCollection, DataConfigPayload,
    DataConfigPoint, DataConfigPublish, DeviceInstance, DeviceSpec, EdgeConfigPackage,
    MqttUplinkConfig, PointAccess, PointAddress, ProtocolConnection, TelemetryPoint,
    TelemetryPointMapping, TelemetryType,
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
fn validator_rejects_invalid_bacnet_point_address_before_release() {
    let mut package = valid_package();
    package.protocol_connections[0] = ProtocolConnection::bacnet_ip(
        "bacnet-main",
        Some("127.0.0.1:47808"),
        BacnetIpConnectionSettings::default(),
    );
    package.point_mappings[0].protocol_connection_id = "bacnet-main".to_string();
    package.point_mappings[0].address = PointAddress {
        kind: "bacnet_object_property".to_string(),
        value: "4194303:analog_input:1:present_value".to_string(),
        modbus: None,
    };

    let errors = ConfigValidator::validate_package(&package);

    assert_eq!(errors.len(), 1);
    assert!(errors[0]
        .message
        .contains("device and object instances must be between 0 and 4194302"));
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
fn validator_rejects_unsafe_data_config_recovery_limits() {
    let package = valid_package()
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://velamq.local:1883",
            "edge-dev",
        ))
        .with_data_config(
            DataConfig::new(
                "pump_status",
                "泵状态",
                "pump-1",
                "sim-main",
                DataConfigCollection::new(1000)
                    .with_timeout_ms(0)
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

    let errors = ConfigValidator::validate_package(&package);

    assert!(errors
        .iter()
        .any(|error| error.message.contains("collection timeout")));
    assert!(errors
        .iter()
        .any(|error| error.message.contains("collection retry count")));
}

#[test]
fn validator_rejects_incomplete_mqtt_security_configuration() {
    let mut uplink =
        edge_core::MqttUplinkConfig::velamq("velamq-main", "mqtt://velamq.local:1883", "edge-dev");
    uplink.username = Some("edge-device".to_string());
    uplink.tls_ca_path = Some("/etc/edgeops/velamq-ca.pem".to_string());
    let package = valid_package().with_mqtt_uplink(uplink);

    let errors = ConfigValidator::validate_package(&package);

    assert!(errors.iter().any(|error| error
        .message
        .contains("password environment reference must be configured together")));
    assert!(errors
        .iter()
        .any(|error| error.message.contains("requires an mqtts:// broker")));
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
fn validator_rejects_collection_of_write_only_points() {
    let mut package = valid_package();
    package.point_mappings[0].access = PointAccess::WriteOnly;

    let errors = ConfigValidator::validate_package(&package);

    assert!(errors.iter().any(|error| error
        .message
        .contains("collection task `pump-main` references write-only point `pressure`")));
}

#[test]
fn validator_rejects_command_flow_targeting_read_only_point() {
    let package = valid_package()
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://127.0.0.1:1883",
            "edge-dev",
        ))
        .with_command_flow(command_flow("pressure"));

    let errors = ConfigValidator::validate_package(&package);

    assert!(errors.iter().any(|error| error
        .message
        .contains("write node point-write references read-only point pressure")));
}

#[test]
fn validator_accepts_command_flow_targeting_writable_point() {
    let mut package = valid_package()
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://127.0.0.1:1883",
            "edge-dev",
        ))
        .with_command_flow(command_flow("pressure"));
    package.point_mappings[0].access = PointAccess::ReadWrite;

    let errors = ConfigValidator::validate_package(&package);

    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn validator_rejects_incomplete_command_rate_limit() {
    let mut safety =
        CommandGraphNode::new("safety", CommandGraphNodeKind::SafetyGate, "指令安全策略");
    safety
        .params
        .insert("max_commands".to_string(), serde_json::json!(5));

    let mut package = valid_package()
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://127.0.0.1:1883",
            "edge-dev",
        ))
        .with_command_flow(command_flow_with_safety("pressure", safety));
    package.point_mappings[0].access = PointAccess::ReadWrite;

    let errors = ConfigValidator::validate_package(&package);

    assert!(errors.iter().any(|error| error
        .message
        .contains("max_commands and window_ms must be configured together")));
}

#[test]
fn validator_rejects_invalid_command_source_allowlist() {
    let mut safety =
        CommandGraphNode::new("safety", CommandGraphNodeKind::SafetyGate, "指令安全策略");
    safety.params.insert(
        "allowed_sources".to_string(),
        serde_json::json!(["scada", ""]),
    );

    let mut package = valid_package()
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://127.0.0.1:1883",
            "edge-dev",
        ))
        .with_command_flow(command_flow_with_safety("pressure", safety));
    package.point_mappings[0].access = PointAccess::ReadWrite;

    let errors = ConfigValidator::validate_package(&package);

    assert!(errors.iter().any(|error| error
        .message
        .contains("allowed_sources requires non-empty strings")));
}

#[test]
fn validator_rejects_writable_modbus_input_area() {
    let mut package = valid_package();
    package.point_mappings[0].address = PointAddress {
        kind: "input_register".to_string(),
        value: "30001".to_string(),
        modbus: None,
    };
    package.point_mappings[0].access = PointAccess::ReadWrite;

    let errors = ConfigValidator::validate_package(&package);

    assert!(errors.iter().any(|error| error
        .message
        .contains("input_register points are protocol-level read-only")));
}

fn command_flow(point_id: &str) -> CommandFlowConfig {
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
        CommandGraphNode::new("point-write", CommandGraphNodeKind::PointWrite, "写入点位")
            .with_ref(point_id),
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
    ))
}

fn command_flow_with_safety(point_id: &str, safety: CommandGraphNode) -> CommandFlowConfig {
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
    .with_node(safety)
    .with_node(
        CommandGraphNode::new("point-write", CommandGraphNodeKind::PointWrite, "写入点位")
            .with_ref(point_id),
    )
    .with_node(CommandGraphNode::new(
        "mqtt-reply",
        CommandGraphNodeKind::MqttReply,
        "MQTT 回执",
    ))
    .with_edge(CommandGraphEdge::new(
        "input-safety",
        "mqtt-input",
        "safety",
    ))
    .with_edge(CommandGraphEdge::new(
        "safety-write",
        "safety",
        "point-write",
    ))
    .with_edge(CommandGraphEdge::new(
        "write-reply",
        "point-write",
        "mqtt-reply",
    ))
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
fn validator_rejects_command_flow_point_from_other_connection() {
    let package = valid_package()
        .with_protocol_connection(ProtocolConnection::simulated("sim-secondary"))
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://velamq.local:1883",
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
        .with_command_flow(command_flow("start_command").with_protocol_connection("sim-main"));

    let errors = ConfigValidator::validate_package(&package);

    assert!(errors.iter().any(|error| error.message.contains(
        "write node point-write uses protocol connection sim-secondary, expected sim-main"
    )));
}

#[test]
fn validator_rejects_command_flow_with_missing_connection() {
    let package = valid_package()
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://velamq.local:1883",
            "edge-dev",
        ))
        .with_command_flow(command_flow("pressure").with_protocol_connection("missing"));

    let errors = ConfigValidator::validate_package(&package);

    assert!(errors.iter().any(|error| error
        .message
        .contains("references missing protocol connection `missing`")));
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
