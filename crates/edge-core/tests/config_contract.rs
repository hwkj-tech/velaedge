use edge_core::{
    AlgorithmDsl, AlgorithmInputBinding, AlgorithmKind, AlgorithmOutput, AlgorithmReportMode,
    AlgorithmReportPolicy, AlgorithmRuntime, AlgorithmSpec, AlgorithmStep, AlgorithmTrigger,
    CollectionTask, CompareOperator, CustomSerialChecksum, CustomSerialPointSpec,
    CustomSerialValueEncoding, DataConfig, DataConfigCollection, DataConfigPayload,
    DataConfigPoint, DataConfigPublish, DeviceInstance, EdgeConfigPackage, MqttUplinkConfig,
    NumberRange, PointAddress, ProtocolConnection, ProtocolType, SerialConnectionSettings,
    TelemetryPointMapping, TelemetryType, WindowAggregateFunction,
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

#[test]
fn modbus_rtu_connection_preserves_serial_settings() {
    let serial = SerialConnectionSettings::new("/dev/ttyUSB0", 9600)
        .with_data_bits(8)
        .with_stop_bits(1)
        .with_parity("none");
    let connection = ProtocolConnection::modbus_rtu_serial("meter-rs485-bus-1", serial.clone());

    assert_eq!(connection.protocol, ProtocolType::ModbusRtu);
    assert_eq!(connection.endpoint.as_deref(), Some("/dev/ttyUSB0"));
    assert_eq!(connection.serial.as_ref(), Some(&serial));
}

#[test]
fn modbus_tcp_connection_preserves_network_endpoint() {
    let connection = ProtocolConnection::modbus_tcp("plc-main", "tcp://127.0.0.1:1502");

    assert_eq!(connection.protocol, ProtocolType::ModbusTcp);
    assert_eq!(connection.endpoint.as_deref(), Some("tcp://127.0.0.1:1502"));
    assert!(connection.serial.is_none());
}

#[test]
fn iec101_connection_and_point_preserve_serial_and_address_metadata() {
    let serial = SerialConnectionSettings::new("/dev/ttyUSB1", 9600)
        .with_data_bits(8)
        .with_stop_bits(1)
        .with_parity("even");
    let connection = ProtocolConnection::iec101_serial("substation-iec101", serial.clone());
    let address = PointAddress::iec101(1, 2, 1001);

    assert_eq!(connection.protocol, ProtocolType::Iec101);
    assert_eq!(connection.serial.as_ref(), Some(&serial));
    assert_eq!(address.kind, "iec101_ioa");
    assert_eq!(address.value, "1:2:1001");
}

#[test]
fn custom_serial_point_address_is_a_validated_structured_contract() {
    let mut spec = CustomSerialPointSpec::new("01 03 00 10", 3, CustomSerialValueEncoding::U16Be);
    spec.request_checksum = CustomSerialChecksum::ModbusCrc16;
    spec.response_checksum = CustomSerialChecksum::Sum8;
    spec.response_prefix_hex = Some("01:03".to_string());
    spec.scale = 0.1;

    edge_core::validate_custom_serial_point_spec(&spec).expect("spec is valid");
    let address = PointAddress::custom_serial(&spec).expect("address serializes");
    let decoded: CustomSerialPointSpec = serde_json::from_str(&address.value).unwrap();

    assert_eq!(address.kind, "custom_serial_frame");
    assert_eq!(decoded, spec);
    assert_eq!(decoded.value_width().unwrap(), 2);
}

#[test]
fn custom_serial_contract_rejects_unbounded_or_malformed_frames() {
    let mut malformed = CustomSerialPointSpec::new("0A1", 0, CustomSerialValueEncoding::U16Be);
    assert!(edge_core::validate_custom_serial_point_spec(&malformed)
        .unwrap_err()
        .contains("complete byte pairs"));

    malformed.request_hex = "01".to_string();
    malformed.value_offset = 4095;
    assert!(edge_core::validate_custom_serial_point_spec(&malformed)
        .unwrap_err()
        .contains("4096-byte response limit"));
}

#[test]
fn mqtt_is_modeled_as_northbound_uplink_not_device_protocol() {
    let protocol_json =
        serde_json::to_string(&ProtocolType::ModbusRtu).expect("protocol serializes");

    assert_ne!(protocol_json, "\"Mqtt\"");

    let uplink = MqttUplinkConfig::velamq(
        "velamq-main",
        "mqtts://velamq.local:8883",
        "edge-dev-runtime-dev",
    )
    .with_topic_template("edge/{edge_id}/device/{device_id}/telemetry")
    .with_qos(1);

    assert_eq!(uplink.sink_id, "velamq-main");
    assert_eq!(uplink.broker, "mqtts://velamq.local:8883");
    assert_eq!(uplink.qos, 1);
    assert_eq!(uplink.username, None);
    assert_eq!(uplink.password_env, None);
    assert_eq!(uplink.tls_ca_path, None);
    assert_eq!(
        uplink.topic_template,
        "edge/{edge_id}/device/{device_id}/telemetry"
    );
}

#[test]
fn mqtt_security_uses_secret_references_and_keeps_legacy_json_compatible() {
    let secured = MqttUplinkConfig::velamq(
        "velamq-main",
        "mqtts://velamq.local:8883",
        "edge-dev-runtime-dev",
    )
    .with_credentials_env("edge-device", "EDGEOPS_MQTT_PASSWORD")
    .with_tls_ca_path("/etc/edgeops/velamq-ca.pem");
    let json = serde_json::to_value(&secured).unwrap();

    assert_eq!(json["username"], "edge-device");
    assert_eq!(json["password_env"], "EDGEOPS_MQTT_PASSWORD");
    assert!(json.get("password").is_none());
    assert_eq!(json["tls_ca_path"], "/etc/edgeops/velamq-ca.pem");

    let legacy: MqttUplinkConfig = serde_json::from_value(serde_json::json!({
        "sink_id": "legacy",
        "broker": "mqtt://127.0.0.1:1883",
        "client_id": "legacy-client",
        "topic_template": "edge/{edge_id}/telemetry",
        "qos": 1,
        "batch_size": 100,
        "flush_interval_ms": 1000
    }))
    .unwrap();
    assert_eq!(legacy.username, None);
    assert_eq!(legacy.password_env, None);
    assert_eq!(legacy.tls_ca_path, None);
}

#[test]
fn config_package_contains_data_configs_for_grouped_mqtt_publishing() {
    let package = EdgeConfigPackage::new("edge-dev", "v1")
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtts://velamq.local:8883",
            "edge-dev-runtime",
        ))
        .with_data_config(
            DataConfig::new(
                "pump_status",
                "泵运行状态上报",
                "pump-1",
                "modbus-line-a",
                DataConfigCollection::new(1000),
                DataConfigPublish::new(
                    "velamq-main",
                    "factory/{site}/pump/{device_id}/status",
                    DataConfigPayload::object(),
                ),
            )
            .with_point(DataConfigPoint::new(
                "pressure",
                "pump.pressure",
                PointAddress::modbus_holding_register(40001),
                TelemetryType::Float,
                "pressure",
            ))
            .with_point(DataConfigPoint::new(
                "running",
                "pump.running",
                PointAddress::modbus_holding_register(40002),
                TelemetryType::Boolean,
                "running",
            )),
        );

    let json = serde_json::to_value(&package).unwrap();
    assert_eq!(json["data_configs"][0]["config_id"], "pump_status");
    assert_eq!(json["data_configs"][0]["collection"]["period_ms"], 1000);
    assert_eq!(
        json["data_configs"][0]["publish"]["topic_template"],
        "factory/{site}/pump/{device_id}/status"
    );
    assert_eq!(
        json["data_configs"][0]["points"][0]["json_field"],
        "pressure"
    );
}

#[test]
fn algorithm_dsl_binds_point_inputs_and_virtual_outputs() {
    let algorithm = AlgorithmSpec::dsl(
        "pressure-window-summary",
        "v1",
        AlgorithmKind::WindowAggregate,
        AlgorithmDsl {
            inputs: vec![AlgorithmInputBinding::new("p", "pressure")],
            trigger: AlgorithmTrigger::window(60_000),
            steps: vec![AlgorithmStep::window_aggregate(
                "p",
                vec![WindowAggregateFunction::Avg {
                    output: "pressure_avg".to_string(),
                }],
            )],
            outputs: vec![AlgorithmOutput::virtual_point(
                "pressure_avg",
                "pressure.avg_1m",
            )],
            report: AlgorithmReportPolicy::new(AlgorithmReportMode::WindowResult, "velamq-main"),
        },
    );

    let package = EdgeConfigPackage::new("edge-dev", "2026.06.28-001")
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
        .with_algorithm(algorithm.clone());

    assert_eq!(package.algorithms[0].kind, AlgorithmKind::WindowAggregate);
    assert_eq!(package.algorithms[0].inputs(), vec!["pressure"]);
    assert_eq!(package.algorithms[0].outputs(), vec!["pressure.avg_1m"]);

    let json = serde_json::to_value(&algorithm).expect("algorithm serializes");
    assert_eq!(json["kind"], "WindowAggregate");
    assert_eq!(json["dsl"]["inputs"][0]["pointId"], "pressure");
    assert_eq!(json["dsl"]["outputs"][0]["pointId"], "pressure.avg_1m");
}

#[test]
fn duration_condition_dsl_preserves_cloud_runtime_wire_contract() {
    let algorithm = AlgorithmSpec::dsl(
        "pressure-high-duration",
        "v1",
        AlgorithmKind::DurationRule,
        AlgorithmDsl {
            inputs: vec![AlgorithmInputBinding::new("p", "pressure")],
            trigger: AlgorithmTrigger::on_sample(),
            steps: vec![AlgorithmStep::DurationCondition {
                source: "p".to_string(),
                operator: CompareOperator::Gte,
                threshold: 10.0,
                duration_ms: 5_000,
                output: "value".to_string(),
            }],
            outputs: vec![AlgorithmOutput::virtual_point("value", "pressure.high_5s")],
            report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnOutput, "velamq-main"),
        },
    );

    let json = serde_json::to_value(&algorithm).expect("algorithm serializes");
    assert_eq!(json["kind"], "DurationRule");
    assert_eq!(json["dsl"]["steps"][0]["type"], "durationCondition");
    assert_eq!(json["dsl"]["steps"][0]["operator"], "Gte");
    assert_eq!(json["dsl"]["steps"][0]["durationMs"], 5_000);

    let decoded: AlgorithmSpec =
        serde_json::from_value(json).expect("algorithm deserializes from EdgeLink JSON");
    assert_eq!(decoded.kind, AlgorithmKind::DurationRule);
    assert!(matches!(
        decoded.dsl.steps.as_slice(),
        [AlgorithmStep::DurationCondition {
            duration_ms: 5_000,
            ..
        }]
    ));
}

#[test]
fn controlled_field_preflight_fixture_matches_the_runtime_config_contract() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/fixtures/field-preflight-config.json");
    let json = std::fs::read_to_string(&fixture).expect("preflight fixture is readable");
    let package: EdgeConfigPackage =
        serde_json::from_str(&json).expect("preflight fixture matches EdgeConfigPackage");

    assert_eq!(package.edge_id, "edge-preflight");
    assert_eq!(package.protocol_connections.len(), 1);
    assert_eq!(
        package.protocol_connections[0].protocol,
        ProtocolType::ModbusRtu
    );
    assert_eq!(package.data_configs.len(), 1);
    assert_eq!(package.data_configs[0].points.len(), 1);
    assert_eq!(package.mqtt_uplinks[0].qos, 1);
}

#[test]
fn legacy_algorithm_runtime_values_migrate_to_the_dsl_engine() {
    for legacy in ["Rule", "Wasm", "Onnx", "Python"] {
        let runtime: AlgorithmRuntime =
            serde_json::from_str(&format!("\"{legacy}\"")).expect("legacy value migrates");
        assert_eq!(runtime, AlgorithmRuntime::Rule);
        assert_eq!(serde_json::to_string(&runtime).unwrap(), "\"Rule\"");
    }

    assert!(serde_json::from_str::<AlgorithmRuntime>("\"NativePlugin\"").is_err());
}
