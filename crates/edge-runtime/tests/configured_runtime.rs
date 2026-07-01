use edge_core::{
    AlgorithmDsl, AlgorithmInputBinding, AlgorithmKind, AlgorithmOutput, AlgorithmReportMode,
    AlgorithmReportPolicy, AlgorithmSpec, AlgorithmStep, AlgorithmTrigger, CollectionTask,
    DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint, DataConfigPublish,
    DeviceInstance, EdgeConfigPackage, MqttUplinkConfig, PointAddress, ProtocolConnection,
    SerialConnectionSettings, TelemetryPointMapping, TelemetryType, TelemetryValue,
};
use edge_runtime::{
    append_modbus_rtu_crc, ConfiguredEdgeRuntime, RecordingMqttPublisher, ScriptedSerialBus,
    ScriptedSerialBusFactory,
};

fn modbus_package() -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", "2026.06.28-modbus")
        .with_device(DeviceInstance::new("meter-1", "power-meter"))
        .with_protocol_connection(ProtocolConnection::modbus_rtu_serial(
            "meter-rs485-bus-1",
            SerialConnectionSettings::new("/dev/ttyUSB0", 9600),
        ))
        .with_mqtt_uplink(
            MqttUplinkConfig::velamq("velamq-main", "mqtt://velamq.local:1883", "edge-dev")
                .with_topic_template("velamq/{edge_id}/{device_id}/{telemetry_id}"),
        )
        .with_point_mapping(TelemetryPointMapping::new(
            "voltage",
            "meter-1",
            "voltage",
            "meter-rs485-bus-1",
            PointAddress::modbus_holding_register(40001),
            TelemetryType::Integer,
        ))
        .with_collection_task(CollectionTask::interval(
            "meter-main",
            "meter-1",
            vec!["voltage".to_string()],
            1000,
        ))
}

#[tokio::test]
async fn configured_runtime_collects_modbus_rtu_points_from_cloud_package() {
    let bus = ScriptedSerialBus::new(vec![response(1, &[220])]);
    let observed_bus = bus.clone();
    let factory = ScriptedSerialBusFactory::new(vec![("meter-rs485-bus-1".to_string(), bus)]);
    let mut runtime = ConfiguredEdgeRuntime::new(modbus_package(), factory).unwrap();

    let report = runtime.collect_once().await.unwrap();

    assert_eq!(report.samples_collected, 1);
    assert_eq!(
        runtime.shadow("meter-1").unwrap().latest_value("voltage"),
        Some(&TelemetryValue::Integer(220))
    );
    assert_eq!(&observed_bus.requests()[0][..6], &[1, 0x03, 0, 0, 0, 1]);
}

#[test]
fn configured_runtime_rejects_collection_task_with_unknown_point() {
    let package = modbus_package().with_collection_task(CollectionTask::interval(
        "broken-task",
        "meter-1",
        vec!["missing_voltage".to_string()],
        1000,
    ));
    let factory = ScriptedSerialBusFactory::new(Vec::new());

    let error = match ConfiguredEdgeRuntime::new(package, factory) {
        Ok(_) => panic!("invalid runtime config rejected"),
        Err(error) => error,
    };

    assert!(error
        .to_string()
        .contains("collection task broken-task references missing point missing_voltage"));
}

#[tokio::test]
async fn configured_runtime_publishes_modbus_samples_to_mqtt_uplink() {
    let bus = ScriptedSerialBus::new(vec![response(1, &[220])]);
    let factory = ScriptedSerialBusFactory::new(vec![("meter-rs485-bus-1".to_string(), bus)]);
    let mut runtime = ConfiguredEdgeRuntime::new(modbus_package(), factory).unwrap();
    let mut publisher = RecordingMqttPublisher::default();

    let report = runtime
        .collect_once_and_publish_mqtt(&mut publisher)
        .await
        .unwrap();

    assert_eq!(report.collection.samples_collected, 1);
    assert_eq!(report.mqtt_messages_published, 1);
    assert_eq!(
        publisher.messages()[0].topic,
        "velamq/edge-dev/meter-1/voltage"
    );
}

#[tokio::test]
async fn configured_runtime_publishes_one_mqtt_message_per_data_config() {
    let package = package_with_two_modbus_data_configs();
    let bus_factory = ScriptedSerialBusFactory::new(vec![(
        "meter-rs485-bus-1".to_string(),
        ScriptedSerialBus::new(vec![
            response(1, &[220]),
            response(1, &[1]),
            response(1, &[61]),
            response(1, &[1290]),
            response(1, &[19]),
            response(1, &[7]),
        ]),
    )]);
    let mut runtime = ConfiguredEdgeRuntime::new(package, bus_factory).unwrap();
    let mut publisher = RecordingMqttPublisher::default();

    let report = runtime
        .collect_data_configs_once_and_publish_mqtt(&mut publisher)
        .await
        .unwrap();

    assert_eq!(report.collection.samples_collected, 6);
    assert_eq!(report.mqtt_messages_published, 2);
    assert_eq!(publisher.messages().len(), 2);
    assert_eq!(
        publisher.messages()[0].topic,
        "velamq/edge-dev/meter-1/status"
    );
    assert_eq!(
        publisher.messages()[1].topic,
        "velamq/edge-dev/meter-1/energy"
    );

    let status_payload: serde_json::Value =
        serde_json::from_slice(&publisher.messages()[0].payload).unwrap();
    assert_eq!(status_payload["values"]["voltage"], 220);
    assert_eq!(status_payload["values"]["running"], true);
}

#[tokio::test]
async fn configured_runtime_publishes_algorithm_virtual_points_to_mqtt_uplink() {
    let package = EdgeConfigPackage::new("edge-dev", "2026.06.28-dsl")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_mqtt_uplink(
            MqttUplinkConfig::velamq("velamq-main", "mqtt://velamq.local:1883", "edge-dev")
                .with_topic_template("velamq/{edge_id}/{device_id}/{telemetry_id}"),
        )
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
        .with_algorithm(AlgorithmSpec::dsl(
            "pressure-change",
            "v1",
            AlgorithmKind::ChangeReport,
            AlgorithmDsl {
                inputs: vec![AlgorithmInputBinding::new("p", "pressure")],
                trigger: AlgorithmTrigger::on_sample(),
                steps: vec![AlgorithmStep::change_filter("p", 0.1)],
                outputs: vec![AlgorithmOutput::virtual_point("p", "pressure.reported")],
                report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnChange, "velamq-main"),
            },
        ));
    let mut runtime = ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(vec![]))
        .expect("runtime builds");
    let mut publisher = RecordingMqttPublisher::default();

    let report = runtime
        .collect_once_and_publish_mqtt(&mut publisher)
        .await
        .unwrap();

    assert_eq!(report.collection.samples_collected, 2);
    assert_eq!(report.mqtt_messages_published, 2);
    assert_eq!(
        publisher.messages()[1].topic,
        "velamq/edge-dev/pump-1/pressure.reported"
    );
}

#[tokio::test]
async fn configured_runtime_publishes_data_config_with_algorithm_outputs() {
    let package = EdgeConfigPackage::new("edge-dev", "2026.07.01-data-dsl")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_mqtt_uplink(
            MqttUplinkConfig::velamq("velamq-main", "mqtt://velamq.local:1883", "edge-dev")
                .with_topic_template("unused/{edge_id}/{device_id}/{telemetry_id}"),
        )
        .with_point_mapping(TelemetryPointMapping::new(
            "pressure",
            "pump-1",
            "pressure",
            "sim-main",
            PointAddress::simulated("pressure"),
            TelemetryType::Float,
        ))
        .with_data_config(
            DataConfig::new(
                "pump_status",
                "泵运行状态上报",
                "pump-1",
                "sim-main",
                DataConfigCollection::new(1000),
                DataConfigPublish::new(
                    "velamq-main",
                    "velamq/{edge_id}/{device_id}/status",
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
            .with_algorithm("pressure-change"),
        )
        .with_algorithm(AlgorithmSpec::dsl(
            "pressure-change",
            "v1",
            AlgorithmKind::ChangeReport,
            AlgorithmDsl {
                inputs: vec![AlgorithmInputBinding::new("p", "pressure")],
                trigger: AlgorithmTrigger::on_sample(),
                steps: vec![AlgorithmStep::change_filter("p", 0.1)],
                outputs: vec![AlgorithmOutput::virtual_point(
                    "pressureReported",
                    "pressure.reported",
                )],
                report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnChange, "velamq-main"),
            },
        ));
    let mut runtime = ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(vec![]))
        .expect("runtime builds");
    let mut publisher = RecordingMqttPublisher::default();

    let report = runtime
        .collect_data_configs_once_and_publish_mqtt(&mut publisher)
        .await
        .unwrap();

    assert_eq!(report.collection.samples_collected, 2);
    assert_eq!(report.mqtt_messages_published, 1);
    let payload: serde_json::Value =
        serde_json::from_slice(&publisher.messages()[0].payload).unwrap();
    assert_eq!(payload["values"]["pressure"], 1.0);
    assert_eq!(payload["values"]["pressureReported"], 1.0);
}

fn response(slave_id: u8, registers: &[u16]) -> Vec<u8> {
    let mut frame = vec![slave_id, 0x03, (registers.len() * 2) as u8];
    for register in registers {
        frame.extend(register.to_be_bytes());
    }
    append_modbus_rtu_crc(&mut frame);
    frame
}

fn package_with_two_modbus_data_configs() -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", "2026.06.30-data-config")
        .with_device(DeviceInstance::new("meter-1", "power-meter"))
        .with_protocol_connection(ProtocolConnection::modbus_rtu_serial(
            "meter-rs485-bus-1",
            SerialConnectionSettings::new("/dev/ttyUSB0", 9600),
        ))
        .with_mqtt_uplink(
            MqttUplinkConfig::velamq("velamq-main", "mqtt://velamq.local:1883", "edge-dev")
                .with_topic_template("unused/{edge_id}/{device_id}/{telemetry_id}"),
        )
        .with_point_mapping(TelemetryPointMapping::new(
            "voltage",
            "meter-1",
            "meter.voltage",
            "meter-rs485-bus-1",
            PointAddress::modbus_holding_register(40001),
            TelemetryType::Integer,
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "running",
            "meter-1",
            "meter.running",
            "meter-rs485-bus-1",
            PointAddress::modbus_holding_register(40002),
            TelemetryType::Boolean,
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "load",
            "meter-1",
            "meter.load",
            "meter-rs485-bus-1",
            PointAddress::modbus_holding_register(40003),
            TelemetryType::Integer,
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "energy_total",
            "meter-1",
            "meter.energy_total",
            "meter-rs485-bus-1",
            PointAddress::modbus_holding_register(40101),
            TelemetryType::Integer,
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "current_a",
            "meter-1",
            "meter.current_a",
            "meter-rs485-bus-1",
            PointAddress::modbus_holding_register(40102),
            TelemetryType::Integer,
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "current_b",
            "meter-1",
            "meter.current_b",
            "meter-rs485-bus-1",
            PointAddress::modbus_holding_register(40103),
            TelemetryType::Integer,
        ))
        .with_data_config(
            DataConfig::new(
                "meter_status",
                "电表状态",
                "meter-1",
                "meter-rs485-bus-1",
                DataConfigCollection::new(1000),
                DataConfigPublish::new(
                    "velamq-main",
                    "velamq/{edge_id}/{device_id}/status",
                    DataConfigPayload::object(),
                ),
            )
            .with_point(DataConfigPoint::new(
                "voltage",
                "meter.voltage",
                PointAddress::modbus_holding_register(40001),
                TelemetryType::Integer,
                "voltage",
            ))
            .with_point(DataConfigPoint::new(
                "running",
                "meter.running",
                PointAddress::modbus_holding_register(40002),
                TelemetryType::Boolean,
                "running",
            ))
            .with_point(DataConfigPoint::new(
                "load",
                "meter.load",
                PointAddress::modbus_holding_register(40003),
                TelemetryType::Integer,
                "load",
            )),
        )
        .with_data_config(
            DataConfig::new(
                "meter_energy",
                "电表能耗",
                "meter-1",
                "meter-rs485-bus-1",
                DataConfigCollection::new(5000),
                DataConfigPublish::new(
                    "velamq-main",
                    "velamq/{edge_id}/{device_id}/energy",
                    DataConfigPayload::object(),
                ),
            )
            .with_point(DataConfigPoint::new(
                "energy_total",
                "meter.energy_total",
                PointAddress::modbus_holding_register(40101),
                TelemetryType::Integer,
                "energyTotal",
            ))
            .with_point(DataConfigPoint::new(
                "current_a",
                "meter.current_a",
                PointAddress::modbus_holding_register(40102),
                TelemetryType::Integer,
                "currentA",
            ))
            .with_point(DataConfigPoint::new(
                "current_b",
                "meter.current_b",
                PointAddress::modbus_holding_register(40103),
                TelemetryType::Integer,
                "currentB",
            )),
        )
}
