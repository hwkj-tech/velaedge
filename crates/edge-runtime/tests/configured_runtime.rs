use edge_core::{
    AlgorithmDsl, AlgorithmInputBinding, AlgorithmKind, AlgorithmOutput, AlgorithmReportMode,
    AlgorithmReportPolicy, AlgorithmSpec, AlgorithmStep, AlgorithmTrigger, CollectionTask,
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

fn response(slave_id: u8, registers: &[u16]) -> Vec<u8> {
    let mut frame = vec![slave_id, 0x03, (registers.len() * 2) as u8];
    for register in registers {
        frame.extend(register.to_be_bytes());
    }
    append_modbus_rtu_crc(&mut frame);
    frame
}
