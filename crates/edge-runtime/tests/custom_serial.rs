use edge_core::{
    CollectionTask, CustomSerialChecksum, CustomSerialPointSpec, CustomSerialValueEncoding,
    DeviceInstance, EdgeConfigPackage, PointAddress, ProtocolConnection, ProtocolType,
    SerialConnectionSettings, TelemetryPointMapping, TelemetryType, TelemetryValue,
};
use edge_runtime::{
    append_custom_serial_checksum, ConfiguredEdgeRuntime, CustomSerialAdapter, ProtocolAdapter,
    ScriptedSerialBus, ScriptedSerialBusFactory,
};

fn connection() -> ProtocolConnection {
    ProtocolConnection {
        connection_id: "vendor-rs485".to_string(),
        protocol: ProtocolType::CustomSerial,
        endpoint: Some("/dev/ttyUSB0".to_string()),
        serial: Some(SerialConnectionSettings::new("/dev/ttyUSB0", 9600)),
    }
}

fn point_spec() -> CustomSerialPointSpec {
    let mut spec = CustomSerialPointSpec::new("10 02", 1, CustomSerialValueEncoding::U16Be);
    spec.request_checksum = CustomSerialChecksum::Sum8;
    spec.response_checksum = CustomSerialChecksum::Sum8;
    spec.response_prefix_hex = Some("AA".to_string());
    spec.scale = 0.1;
    spec
}

fn mapping(spec: &CustomSerialPointSpec) -> TelemetryPointMapping {
    TelemetryPointMapping::new(
        "temperature",
        "sensor-1",
        "temperature",
        "vendor-rs485",
        PointAddress::custom_serial(spec).unwrap(),
        TelemetryType::Float,
    )
}

#[tokio::test]
async fn custom_serial_adapter_executes_frame_dsl_and_scales_value() {
    let bus = ScriptedSerialBus::new(vec![vec![0xAA, 0x01, 0x2C, 0xD7]]);
    let observed_bus = bus.clone();
    let mut adapter = CustomSerialAdapter::new(connection(), vec![mapping(&point_spec())], bus);

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(observed_bus.requests(), vec![vec![0x10, 0x02, 0x12]]);
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].value, TelemetryValue::Float(30.0));
}

#[tokio::test]
async fn custom_serial_adapter_fails_closed_on_bad_checksum() {
    let bus = ScriptedSerialBus::new(vec![vec![0xAA, 0x01, 0x2C, 0x00]]);
    let mut adapter = CustomSerialAdapter::new(connection(), vec![mapping(&point_spec())], bus);

    let error = adapter.read_telemetry().await.unwrap_err();

    assert!(error.to_string().contains("sum8 mismatch"));
}

#[test]
fn custom_serial_checksum_supports_modbus_crc16() {
    let mut frame = vec![0x01, 0x03, 0x00, 0x00, 0x00, 0x01];
    append_custom_serial_checksum(&mut frame, CustomSerialChecksum::ModbusCrc16);

    assert_eq!(frame, vec![0x01, 0x03, 0x00, 0x00, 0x00, 0x01, 0x84, 0x0A]);
}

#[tokio::test]
async fn configured_runtime_executes_custom_serial_mapping_from_cloud_package() {
    let spec = point_spec();
    let package = EdgeConfigPackage::new("edge-custom", "v1")
        .with_device(DeviceInstance::new("sensor-1", "vendor-sensor"))
        .with_protocol_connection(connection())
        .with_point_mapping(mapping(&spec))
        .with_collection_task(CollectionTask::interval(
            "vendor-telemetry",
            "sensor-1",
            vec!["temperature".to_string()],
            1000,
        ));
    let bus = ScriptedSerialBus::new(vec![vec![0xAA, 0x01, 0x2C, 0xD7]]);
    let factory = ScriptedSerialBusFactory::new(vec![("vendor-rs485".to_string(), bus)]);
    let mut runtime = ConfiguredEdgeRuntime::new(package, factory).unwrap();

    let report = runtime.collect_once().await.unwrap();

    assert_eq!(report.samples_collected, 1);
    assert_eq!(
        runtime
            .shadow("sensor-1")
            .unwrap()
            .latest_value("temperature"),
        Some(&TelemetryValue::Float(30.0))
    );
}
