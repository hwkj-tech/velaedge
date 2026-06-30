use edge_core::{
    AlgorithmDsl, AlgorithmInputBinding, AlgorithmKind, AlgorithmOutput, AlgorithmReportMode,
    AlgorithmReportPolicy, AlgorithmSpec, AlgorithmStep, AlgorithmTrigger, CollectionTask,
    DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint, DataConfigPublish,
    DeviceInstance, EdgeConfigPackage, MqttUplinkConfig, NumberRange, PointAddress,
    ProtocolConnection, ProtocolType, SerialConnectionSettings, TelemetryPointMapping,
    TelemetryType, WindowAggregateFunction,
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
    assert_eq!(
        uplink.topic_template,
        "edge/{edge_id}/device/{device_id}/telemetry"
    );
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
