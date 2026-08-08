use std::time::Duration;

use edge_core::{
    CommandFlowConfig, CommandGraphEdge, CommandGraphNode, CommandGraphNodeKind, DeviceInstance,
    EdgeConfigPackage, MqttUplinkConfig, NumberRange, PointAccess, PointAddress,
    ProtocolConnection, TelemetryPointMapping, TelemetryType, TelemetryValue,
};
use edge_runtime::{
    CommandAuditState, CommandExecutionStatus, CommandWriteVerification, ConfiguredEdgeRuntime,
    ModbusTcpAdapter, ModbusTcpSimulator, ModbusTcpSimulatorOptions, MqttCommandMessage,
    ProtocolAdapter, RecordingMqttPublisher, RocksEdgeRuntimeStore, ScriptedSerialBusFactory,
};
use tempfile::tempdir;

#[tokio::test]
async fn command_flow_writes_a_real_modbus_tcp_register_and_builds_reply() {
    let mut options = ModbusTcpSimulatorOptions::new("127.0.0.1:0".parse().unwrap());
    options.holding_registers.insert(0, 0);
    options.holding_registers.insert(1, 0);
    let simulator = ModbusTcpSimulator::bind(options).await.unwrap();
    let endpoint = simulator.local_addr().unwrap().to_string();
    let server = tokio::spawn(simulator.run());
    let mapping = TelemetryPointMapping::new(
        "pressure_setpoint",
        "pump-1",
        "pump.pressure_setpoint",
        "modbus-main",
        PointAddress::modbus_holding_register(40001),
        TelemetryType::Float,
    )
    .with_access(PointAccess::ReadWrite)
    .with_range(NumberRange::new(0.0, 20.0));
    let package = base_package(&endpoint)
        .with_point_mapping(mapping.clone())
        .with_command_flow(single_write_flow("pressure_setpoint"));
    let mut runtime =
        ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(Vec::new())).unwrap();

    let report = runtime
        .execute_command_flow_message(
            "set-pressure",
            br#"{"commandId":"cmd-100","deviceId":"pump-1","pointId":"pressure_setpoint","value":12.5}"#,
        )
        .await
        .unwrap();

    assert_eq!(report.status, CommandExecutionStatus::Succeeded);
    assert_eq!(report.writes.len(), 1);
    assert_eq!(report.writes[0].value, TelemetryValue::Float(12.5));
    assert_eq!(report.replies.len(), 1);
    assert_eq!(report.replies[0].topic, "factory/edge-live/reply/cmd-100");
    let reply: serde_json::Value = serde_json::from_slice(&report.replies[0].payload).unwrap();
    assert_eq!(reply["status"], "succeeded");
    assert_eq!(reply["writes"][0]["pointId"], "pressure_setpoint");

    let mut reader = ModbusTcpAdapter::new(
        ProtocolConnection::modbus_tcp("modbus-main", endpoint),
        vec![mapping],
    )
    .with_timeouts(Duration::from_secs(1), Duration::from_secs(1));
    let samples = reader.read_telemetry().await.unwrap();
    assert_eq!(samples[0].value, TelemetryValue::Float(12.5));
    assert!(runtime.protocol_runtime_metrics()[0].connected);
    server.abort();
}

#[tokio::test]
async fn command_flow_can_verify_a_real_modbus_tcp_write_by_reading_it_back() {
    let mut options = ModbusTcpSimulatorOptions::new("127.0.0.1:0".parse().unwrap());
    options.holding_registers.insert(0, 0);
    options.holding_registers.insert(1, 0);
    let simulator = ModbusTcpSimulator::bind(options).await.unwrap();
    let endpoint = simulator.local_addr().unwrap().to_string();
    let server = tokio::spawn(simulator.run());
    let mapping = TelemetryPointMapping::new(
        "pressure_setpoint",
        "pump-1",
        "pump.pressure_setpoint",
        "modbus-main",
        PointAddress::modbus_holding_register(40001),
        TelemetryType::Float,
    )
    .with_access(PointAccess::ReadWrite);
    let package = base_package(&endpoint)
        .with_point_mapping(mapping)
        .with_command_flow(single_write_flow_with_verification(
            "pressure_setpoint",
            "readback",
        ));
    let mut runtime =
        ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(Vec::new())).unwrap();

    let report = runtime
        .execute_command_flow_message(
            "set-pressure",
            br#"{"commandId":"cmd-readback","value":12.3}"#,
        )
        .await
        .unwrap();

    assert_eq!(report.status, CommandExecutionStatus::Succeeded);
    assert!(report.writes[0].verified);
    assert_eq!(
        report.writes[0].verification,
        CommandWriteVerification::Readback
    );
    assert!(matches!(
        report.writes[0].readback_value,
        Some(TelemetryValue::Float(value)) if (value - 12.3).abs() <= 1.0e-6
    ));
    server.abort();
}

#[tokio::test]
async fn command_flow_rejects_an_unknown_write_verification_mode_before_device_io() {
    let package = base_package("127.0.0.1:502")
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_point_mapping(
            TelemetryPointMapping::new(
                "speed_setpoint",
                "pump-1",
                "pump.speed_setpoint",
                "sim-main",
                PointAddress::simulated("speed"),
                TelemetryType::Integer,
            )
            .with_access(PointAccess::ReadWrite),
        )
        .with_command_flow(single_write_flow_with_verification(
            "speed_setpoint",
            "unverified",
        ));
    let error = match ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(Vec::new()))
    {
        Ok(_) => panic!("unsupported verification mode must be rejected"),
        Err(error) => error,
    };

    assert!(error
        .to_string()
        .contains("verification mode is unsupported"));
}

#[tokio::test]
async fn command_safety_gate_audits_and_replies_to_an_unauthorized_source_without_writing() {
    let package = base_package("127.0.0.1:502")
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
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
        .with_command_flow(safe_write_flow("start_command", &["scada"], None));
    let mut runtime =
        ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(Vec::new())).unwrap();
    let directory = tempdir().unwrap();
    let store = RocksEdgeRuntimeStore::open(directory.path().join("runtime.rocksdb")).unwrap();
    let mut publisher = RecordingMqttPublisher::default();
    let message = MqttCommandMessage {
        sink_id: "velamq-main".to_string(),
        topic: "factory/edge-live/command".to_string(),
        payload: br#"{"commandId":"cmd-unauthorized","requestedBy":"web-console","value":true}"#
            .to_vec(),
        flow_ids: vec!["set-pressure".to_string()],
    };

    let reports = runtime
        .execute_mqtt_command_message_with_store(&message, &store, &mut publisher)
        .await
        .unwrap();

    assert_eq!(reports[0].status, CommandExecutionStatus::Failed);
    assert_eq!(reports[0].source.as_deref(), Some("web-console"));
    assert!(reports[0].writes.is_empty());
    assert!(reports[0]
        .error
        .as_deref()
        .unwrap()
        .contains("rejected command source web-console"));
    assert_eq!(publisher.messages().len(), 1);
    let reply: serde_json::Value =
        serde_json::from_slice(&publisher.messages()[0].payload).unwrap();
    assert_eq!(reply["status"], "failed");
    assert_eq!(reply["source"], "web-console");
    let audit = store
        .command_audit("edge-live", "set-pressure", "cmd-unauthorized")
        .unwrap()
        .unwrap();
    assert_eq!(audit.state, CommandAuditState::Failed);
    assert_eq!(audit.source.as_deref(), Some("web-console"));
    assert!(runtime
        .shadow("pump-1")
        .unwrap()
        .latest_value("start_command")
        .is_none());
}

#[tokio::test]
async fn command_safety_gate_rate_limits_distinct_commands_before_point_writes() {
    let package = base_package("127.0.0.1:502")
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
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
        .with_command_flow(safe_write_flow(
            "start_command",
            &["scada"],
            Some((1, 60_000)),
        ));
    let mut runtime =
        ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(Vec::new())).unwrap();

    let accepted = runtime
        .execute_command_flow_message(
            "set-pressure",
            br#"{"commandId":"cmd-rate-1","requestedBy":"scada","value":true}"#,
        )
        .await
        .unwrap();
    let rejected = runtime
        .execute_command_flow_message(
            "set-pressure",
            br#"{"commandId":"cmd-rate-2","requestedBy":"scada","value":false}"#,
        )
        .await
        .unwrap();

    assert_eq!(accepted.status, CommandExecutionStatus::Succeeded);
    assert_eq!(accepted.writes.len(), 1);
    assert_eq!(rejected.status, CommandExecutionStatus::Failed);
    assert!(rejected.writes.is_empty());
    assert!(rejected
        .error
        .as_deref()
        .unwrap()
        .contains("rate limit exceeded"));
    assert_eq!(rejected.replies.len(), 1);
    assert_eq!(
        runtime
            .shadow("pump-1")
            .unwrap()
            .latest_value("start_command"),
        Some(&TelemetryValue::Boolean(true))
    );
}

#[tokio::test]
async fn persisted_command_rate_limit_survives_runtime_restart() {
    let package = base_package("127.0.0.1:502")
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
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
        .with_command_flow(safe_write_flow(
            "start_command",
            &["scada"],
            Some((1, 60_000)),
        ));
    let directory = tempdir().unwrap();
    let database_path = directory.path().join("runtime.rocksdb");
    let store = RocksEdgeRuntimeStore::open(&database_path).unwrap();
    let mut publisher = RecordingMqttPublisher::default();
    let first_message = MqttCommandMessage {
        sink_id: "velamq-main".to_string(),
        topic: "factory/edge-live/command".to_string(),
        payload: br#"{"commandId":"cmd-persisted-rate-1","requestedBy":"scada","value":true}"#
            .to_vec(),
        flow_ids: vec!["set-pressure".to_string()],
    };
    let second_message = MqttCommandMessage {
        payload: br#"{"commandId":"cmd-persisted-rate-2","requestedBy":"scada","value":false}"#
            .to_vec(),
        ..first_message.clone()
    };

    let mut first_runtime =
        ConfiguredEdgeRuntime::new(package.clone(), ScriptedSerialBusFactory::new(Vec::new()))
            .unwrap();
    let first = first_runtime
        .execute_mqtt_command_message_with_store(&first_message, &store, &mut publisher)
        .await
        .unwrap();
    drop(first_runtime);
    drop(store);

    let store = RocksEdgeRuntimeStore::open(&database_path).unwrap();
    let mut restarted_runtime =
        ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(Vec::new())).unwrap();
    let second = restarted_runtime
        .execute_mqtt_command_message_with_store(&second_message, &store, &mut publisher)
        .await
        .unwrap();

    assert_eq!(first[0].status, CommandExecutionStatus::Succeeded);
    assert_eq!(second[0].status, CommandExecutionStatus::Failed);
    assert!(second[0].writes.is_empty());
    assert!(second[0]
        .error
        .as_deref()
        .unwrap()
        .contains("rate limit exceeded"));
    let audit = store
        .command_audit("edge-live", "set-pressure", "cmd-persisted-rate-2")
        .unwrap()
        .unwrap();
    assert_eq!(audit.state, CommandAuditState::Failed);
    assert_eq!(audit.source.as_deref(), Some("scada"));
}

#[tokio::test]
async fn command_flow_supports_one_input_with_multiple_writes_and_replies() {
    let package = base_package("127.0.0.1:502")
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_point_mapping(
            TelemetryPointMapping::new(
                "speed_setpoint",
                "pump-1",
                "pump.speed_setpoint",
                "sim-main",
                PointAddress::simulated("speed"),
                TelemetryType::Integer,
            )
            .with_access(PointAccess::ReadWrite),
        )
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
        .with_command_flow(branched_flow());
    let mut runtime =
        ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(Vec::new())).unwrap();

    let report = runtime
        .execute_command_flow_message(
            "start-with-speed",
            br#"{"commandId":"cmd-200","deviceId":"pump-1","execute":true,"values":{"speed_setpoint":1450,"start_command":true}}"#,
        )
        .await
        .unwrap();

    assert_eq!(report.status, CommandExecutionStatus::Succeeded);
    assert_eq!(report.writes.len(), 2);
    assert_eq!(report.replies.len(), 2);
    assert_eq!(
        runtime
            .shadow("pump-1")
            .unwrap()
            .latest_value("speed_setpoint"),
        Some(&TelemetryValue::Integer(1450))
    );
    assert_eq!(
        runtime
            .shadow("pump-1")
            .unwrap()
            .latest_value("start_command"),
        Some(&TelemetryValue::Boolean(true))
    );
}

#[tokio::test]
async fn command_flow_maps_custom_nested_json_fields_to_fixed_write_points() {
    let mut flow = branched_flow();
    flow.nodes
        .iter_mut()
        .find(|node| node.node_id == "write-speed")
        .unwrap()
        .params
        .insert(
            "value_path".to_string(),
            serde_json::json!("payload.control.speed"),
        );
    flow.nodes
        .iter_mut()
        .find(|node| node.node_id == "write-start")
        .unwrap()
        .params
        .insert(
            "value_path".to_string(),
            serde_json::json!("payload.control.start"),
        );
    let package = base_package("127.0.0.1:502")
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_point_mapping(
            TelemetryPointMapping::new(
                "speed_setpoint",
                "pump-1",
                "pump.speed_setpoint",
                "sim-main",
                PointAddress::simulated("speed"),
                TelemetryType::Integer,
            )
            .with_access(PointAccess::ReadWrite),
        )
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
        .with_command_flow(flow);
    let mut runtime =
        ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(Vec::new())).unwrap();

    let report = runtime
        .execute_command_flow_message(
            "start-with-speed",
            br#"{"commandId":"cmd-custom-path","execute":true,"payload":{"control":{"speed":1380,"start":true}}}"#,
        )
        .await
        .unwrap();

    assert_eq!(report.status, CommandExecutionStatus::Succeeded);
    assert_eq!(report.writes.len(), 2);
    assert_eq!(
        runtime
            .shadow("pump-1")
            .unwrap()
            .latest_value("speed_setpoint"),
        Some(&TelemetryValue::Integer(1380))
    );
    assert_eq!(
        runtime
            .shadow("pump-1")
            .unwrap()
            .latest_value("start_command"),
        Some(&TelemetryValue::Boolean(true))
    );
}

#[tokio::test]
async fn command_flow_batches_contiguous_modbus_writes_into_one_device_request() {
    let mut options = ModbusTcpSimulatorOptions::new("127.0.0.1:0".parse().unwrap());
    options.holding_registers.insert(0, 0);
    options.holding_registers.insert(1, 0);
    let simulator = ModbusTcpSimulator::bind(options).await.unwrap();
    let endpoint = simulator.local_addr().unwrap().to_string();
    let metrics = simulator.metrics();
    let server = tokio::spawn(simulator.run());
    let package = base_package(&endpoint)
        .with_point_mapping(
            TelemetryPointMapping::new(
                "speed_setpoint",
                "pump-1",
                "pump.speed_setpoint",
                "modbus-main",
                PointAddress::modbus_holding_register(40001),
                TelemetryType::Integer,
            )
            .with_access(PointAccess::ReadWrite),
        )
        .with_point_mapping(
            TelemetryPointMapping::new(
                "start_command",
                "pump-1",
                "pump.start",
                "modbus-main",
                PointAddress::modbus_holding_register(40002),
                TelemetryType::Boolean,
            )
            .with_access(PointAccess::WriteOnly),
        )
        .with_command_flow(branched_flow());
    let mut runtime =
        ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(Vec::new())).unwrap();

    let report = runtime
        .execute_command_flow_message(
            "start-with-speed",
            br#"{"commandId":"cmd-batch","execute":true,"values":{"speed_setpoint":1450,"start_command":true}}"#,
        )
        .await
        .unwrap();

    assert_eq!(report.status, CommandExecutionStatus::Succeeded);
    assert_eq!(report.writes.len(), 2);
    assert_eq!(metrics.requests_total(), 1);
    assert_eq!(
        runtime
            .shadow("pump-1")
            .unwrap()
            .latest_value("speed_setpoint"),
        Some(&TelemetryValue::Integer(1450))
    );
    assert_eq!(
        runtime
            .shadow("pump-1")
            .unwrap()
            .latest_value("start_command"),
        Some(&TelemetryValue::Boolean(true))
    );
    server.abort();
}

#[tokio::test]
async fn command_flow_rejects_values_outside_point_range_before_device_io() {
    let package = base_package("127.0.0.1:502")
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_point_mapping(
            TelemetryPointMapping::new(
                "speed_setpoint",
                "pump-1",
                "pump.speed_setpoint",
                "sim-main",
                PointAddress::simulated("speed"),
                TelemetryType::Integer,
            )
            .with_access(PointAccess::ReadWrite)
            .with_range(NumberRange::new(0.0, 3_000.0)),
        )
        .with_command_flow(single_write_flow("speed_setpoint"));
    let mut runtime =
        ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(Vec::new())).unwrap();

    let error = runtime
        .execute_command_flow_message(
            "set-pressure",
            br#"{"commandId":"cmd-300","pointId":"speed_setpoint","value":4000}"#,
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("outside [0, 3000]"));
    assert!(runtime
        .shadow("pump-1")
        .unwrap()
        .latest_value("speed_setpoint")
        .is_none());
}

#[tokio::test]
async fn mqtt_command_message_executes_the_matched_flow_and_publishes_its_reply() {
    let package = base_package("127.0.0.1:502")
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
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
        .with_command_flow(single_write_flow("start_command"));
    let mut runtime =
        ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(Vec::new())).unwrap();
    let mut publisher = RecordingMqttPublisher::default();
    let message = MqttCommandMessage {
        sink_id: "velamq-main".to_string(),
        topic: "factory/edge-live/command".to_string(),
        payload: br#"{"commandId":"cmd-400","value":true}"#.to_vec(),
        flow_ids: vec!["set-pressure".to_string()],
    };

    let reports = runtime
        .execute_mqtt_command_message(&message, &mut publisher)
        .await
        .unwrap();

    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].status, CommandExecutionStatus::Succeeded);
    assert_eq!(publisher.messages().len(), 1);
    assert_eq!(
        publisher.messages()[0].topic,
        "factory/edge-live/reply/cmd-400"
    );
}

#[tokio::test]
async fn mqtt_command_store_prevents_duplicate_device_writes_and_replays_reply() {
    let package = base_package("127.0.0.1:502")
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
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
        .with_command_flow(single_write_flow("start_command"));
    let mut runtime =
        ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(Vec::new())).unwrap();
    let directory = tempdir().unwrap();
    let database_path = directory.path().join("runtime.rocksdb");
    let store = RocksEdgeRuntimeStore::open(&database_path).unwrap();
    let mut publisher = RecordingMqttPublisher::default();
    let message = MqttCommandMessage {
        sink_id: "velamq-main".to_string(),
        topic: "factory/edge-live/command".to_string(),
        payload: br#"{"commandId":"cmd-idempotent","value":true}"#.to_vec(),
        flow_ids: vec!["set-pressure".to_string()],
    };

    let first = runtime
        .execute_mqtt_command_message_with_store(&message, &store, &mut publisher)
        .await
        .unwrap();
    let replay = runtime
        .execute_mqtt_command_message_with_store(&message, &store, &mut publisher)
        .await
        .unwrap();

    assert!(!first[0].duplicate);
    assert!(replay[0].duplicate);
    assert_eq!(first[0].writes, replay[0].writes);
    assert_eq!(publisher.messages().len(), 2);
    assert_eq!(store.mqtt_outbox_len().unwrap(), 0);
    let audit = store
        .command_audit("edge-live", "set-pressure", "cmd-idempotent")
        .unwrap()
        .unwrap();
    assert_eq!(audit.state, CommandAuditState::Succeeded);
    assert_eq!(audit.writes.len(), 1);

    drop(store);
    let reopened = RocksEdgeRuntimeStore::open(&database_path).unwrap();
    assert_eq!(
        reopened
            .command_audit("edge-live", "set-pressure", "cmd-idempotent")
            .unwrap()
            .unwrap()
            .state,
        CommandAuditState::Succeeded
    );
}

#[tokio::test]
async fn mqtt_command_store_rejects_command_id_reuse_with_a_different_payload() {
    let package = base_package("127.0.0.1:502")
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
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
        .with_command_flow(single_write_flow("start_command"));
    let mut runtime =
        ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(Vec::new())).unwrap();
    let directory = tempdir().unwrap();
    let store = RocksEdgeRuntimeStore::open(directory.path().join("runtime.rocksdb")).unwrap();
    let mut publisher = RecordingMqttPublisher::default();
    let mut message = MqttCommandMessage {
        sink_id: "velamq-main".to_string(),
        topic: "factory/edge-live/command".to_string(),
        payload: br#"{"commandId":"cmd-conflict","value":true}"#.to_vec(),
        flow_ids: vec!["set-pressure".to_string()],
    };
    runtime
        .execute_mqtt_command_message_with_store(&message, &store, &mut publisher)
        .await
        .unwrap();
    message.payload = br#"{"commandId":"cmd-conflict","value":false}"#.to_vec();

    let error = runtime
        .execute_mqtt_command_message_with_store(&message, &store, &mut publisher)
        .await
        .unwrap_err();

    assert!(error
        .to_string()
        .contains("reused with a different payload"));
    assert_eq!(publisher.messages().len(), 1);
    assert_eq!(
        runtime
            .shadow("pump-1")
            .unwrap()
            .latest_value("start_command"),
        Some(&TelemetryValue::Boolean(true))
    );
}

fn base_package(modbus_endpoint: &str) -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-live", "command-v1")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::modbus_tcp(
            "modbus-main",
            modbus_endpoint,
        ))
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://127.0.0.1:1883",
            "runtime-command-test",
        ))
}

fn single_write_flow(point_id: &str) -> CommandFlowConfig {
    single_write_flow_with_optional_verification(point_id, None)
}

fn single_write_flow_with_verification(point_id: &str, verification: &str) -> CommandFlowConfig {
    single_write_flow_with_optional_verification(point_id, Some(verification))
}

fn single_write_flow_with_optional_verification(
    point_id: &str,
    verification: Option<&str>,
) -> CommandFlowConfig {
    let mut write = CommandGraphNode::new("write", CommandGraphNodeKind::PointWrite, "写点位")
        .with_ref(point_id);
    if let Some(verification) = verification {
        write
            .params
            .insert("verification".to_string(), serde_json::json!(verification));
    }
    CommandFlowConfig::new(
        "set-pressure",
        "设置压力",
        "velamq-main",
        "factory/edge-live/command",
        "factory/{edge_id}/reply/{command_id}",
    )
    .with_node(CommandGraphNode::new(
        "input",
        CommandGraphNodeKind::MqttInput,
        "MQTT 输入",
    ))
    .with_node(write)
    .with_node(CommandGraphNode::new(
        "reply",
        CommandGraphNodeKind::MqttReply,
        "MQTT 回执",
    ))
    .with_edge(CommandGraphEdge::new("input-write", "input", "write"))
    .with_edge(CommandGraphEdge::new("write-reply", "write", "reply"))
}

fn safe_write_flow(
    point_id: &str,
    allowed_sources: &[&str],
    rate_limit: Option<(u32, u64)>,
) -> CommandFlowConfig {
    let mut safety = CommandGraphNode::new("safety", CommandGraphNodeKind::SafetyGate, "安全策略");
    safety.params.insert(
        "allowed_sources".to_string(),
        serde_json::json!(allowed_sources),
    );
    if let Some((max_commands, window_ms)) = rate_limit {
        safety
            .params
            .insert("max_commands".to_string(), serde_json::json!(max_commands));
        safety
            .params
            .insert("window_ms".to_string(), serde_json::json!(window_ms));
    }
    CommandFlowConfig::new(
        "set-pressure",
        "设置压力",
        "velamq-main",
        "factory/edge-live/command",
        "factory/{edge_id}/reply/{command_id}",
    )
    .with_node(CommandGraphNode::new(
        "input",
        CommandGraphNodeKind::MqttInput,
        "MQTT 输入",
    ))
    .with_node(safety)
    .with_node(
        CommandGraphNode::new("write", CommandGraphNodeKind::PointWrite, "写点位")
            .with_ref(point_id),
    )
    .with_node(CommandGraphNode::new(
        "reply",
        CommandGraphNodeKind::MqttReply,
        "MQTT 回执",
    ))
    .with_edge(CommandGraphEdge::new("input-safety", "input", "safety"))
    .with_edge(CommandGraphEdge::new("safety-write", "safety", "write"))
    .with_edge(CommandGraphEdge::new("write-reply", "write", "reply"))
}

fn branched_flow() -> CommandFlowConfig {
    let mut condition =
        CommandGraphNode::new("condition", CommandGraphNodeKind::Condition, "执行条件");
    condition
        .params
        .insert("path".to_string(), serde_json::json!("execute"));
    condition
        .params
        .insert("operator".to_string(), serde_json::json!("eq"));
    condition
        .params
        .insert("value".to_string(), serde_json::json!(true));

    CommandFlowConfig::new(
        "start-with-speed",
        "启动并设置转速",
        "velamq-main",
        "factory/edge-live/start",
        "factory/{edge_id}/reply/{command_id}",
    )
    .with_node(CommandGraphNode::new(
        "input",
        CommandGraphNodeKind::MqttInput,
        "MQTT 输入",
    ))
    .with_node(condition)
    .with_node(
        CommandGraphNode::new("write-speed", CommandGraphNodeKind::PointWrite, "设置转速")
            .with_ref("speed_setpoint"),
    )
    .with_node(
        CommandGraphNode::new("write-start", CommandGraphNodeKind::PointWrite, "启动")
            .with_ref("start_command"),
    )
    .with_node(CommandGraphNode::new(
        "reply-speed",
        CommandGraphNodeKind::MqttReply,
        "转速回执",
    ))
    .with_node(CommandGraphNode::new(
        "reply-start",
        CommandGraphNodeKind::MqttReply,
        "启动回执",
    ))
    .with_edge(CommandGraphEdge::new(
        "input-condition",
        "input",
        "condition",
    ))
    .with_edge(
        CommandGraphEdge::new("condition-speed", "condition", "write-speed")
            .with_ports("true", "input"),
    )
    .with_edge(
        CommandGraphEdge::new("condition-start", "condition", "write-start")
            .with_ports("true", "input"),
    )
    .with_edge(CommandGraphEdge::new(
        "speed-reply",
        "write-speed",
        "reply-speed",
    ))
    .with_edge(CommandGraphEdge::new(
        "start-reply",
        "write-start",
        "reply-start",
    ))
}
