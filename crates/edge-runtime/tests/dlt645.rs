use edge_core::{
    CollectionTask, DataQuality, DeviceInstance, EdgeConfigPackage, PointAddress,
    ProtocolConnection, SerialConnectionSettings, TelemetryPointMapping, TelemetryType,
    TelemetryValue,
};
use edge_runtime::{
    append_dlt645_checksum, ConfiguredEdgeRuntime, Dlt645Adapter, ProtocolAdapter,
    ScriptedSerialBus, ScriptedSerialBusFactory,
};

const METER_ADDRESS: [u8; 6] = [0x12, 0x90, 0x78, 0x56, 0x34, 0x12];
const VOLTAGE_DI: u32 = 0x0001_0000;

#[tokio::test]
async fn dlt645_adapter_reads_scaled_bcd_telemetry() {
    let connection = dlt645_connection();
    let mappings = vec![TelemetryPointMapping::new(
        "voltage",
        "meter-1",
        "electric.voltage",
        "meter-rs485-bus-1",
        PointAddress::dlt645_scaled("123456789012", "00010000", 2),
        TelemetryType::Float,
    )];
    let response = read_response(VOLTAGE_DI, &[0x50, 0x20, 0x02]);
    let bus = ScriptedSerialBus::new(vec![response]);
    let observed_bus = bus.clone();
    let mut adapter = Dlt645Adapter::new(connection, mappings, bus);

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].device_id, "meter-1");
    assert_eq!(samples[0].telemetry_id, "voltage");
    assert_eq!(samples[0].quality, DataQuality::Good);
    assert_eq!(samples[0].value, TelemetryValue::Float(220.5));
    assert_eq!(
        observed_bus.requests()[0],
        read_request(METER_ADDRESS, VOLTAGE_DI)
    );
}

#[tokio::test]
async fn dlt645_adapter_accepts_wakeup_bytes_and_preserves_text_digits() {
    let mappings = vec![TelemetryPointMapping::new(
        "serial_number",
        "meter-1",
        "meter.serial_number",
        "meter-rs485-bus-1",
        PointAddress::dlt645("123456789012", "04000401"),
        TelemetryType::Text,
    )];
    let mut response = vec![0xFE, 0xFE, 0xFE, 0xFE];
    response.extend(read_response(0x0400_0401, &[0x56, 0x34, 0x12]));
    let mut adapter = Dlt645Adapter::new(
        dlt645_connection(),
        mappings,
        ScriptedSerialBus::new(vec![response]),
    );

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(samples[0].value, TelemetryValue::Text("123456".to_string()));
}

#[tokio::test]
async fn dlt645_adapter_rejects_invalid_checksum() {
    let mappings = vec![TelemetryPointMapping::new(
        "energy",
        "meter-1",
        "electric.energy",
        "meter-rs485-bus-1",
        PointAddress::dlt645("123456789012", "00000000"),
        TelemetryType::Integer,
    )];
    let mut response = read_response(0, &[0x42]);
    let checksum_index = response.len() - 2;
    response[checksum_index] = response[checksum_index].wrapping_add(1);
    let mut adapter = Dlt645Adapter::new(
        dlt645_connection(),
        mappings,
        ScriptedSerialBus::new(vec![response]),
    );

    let error = adapter.read_telemetry().await.unwrap_err();

    assert!(error.to_string().contains("DL/T 645 checksum mismatch"));
}

#[tokio::test]
async fn configured_runtime_executes_dlt645_cloud_config() {
    let package = EdgeConfigPackage::new("edge-meter", "2026.07.15-dlt645")
        .with_device(DeviceInstance::new("meter-1", "power-meter"))
        .with_protocol_connection(dlt645_connection())
        .with_point_mapping(TelemetryPointMapping::new(
            "voltage",
            "meter-1",
            "electric.voltage",
            "meter-rs485-bus-1",
            PointAddress::dlt645_scaled("123456789012", "00010000", 2),
            TelemetryType::Float,
        ))
        .with_collection_task(CollectionTask::interval(
            "meter-main",
            "meter-1",
            vec!["voltage".to_string()],
            1000,
        ));
    let bus = ScriptedSerialBus::new(vec![read_response(VOLTAGE_DI, &[0x50, 0x20, 0x02])]);
    let factory = ScriptedSerialBusFactory::new(vec![("meter-rs485-bus-1".to_string(), bus)]);
    let mut runtime = ConfiguredEdgeRuntime::new(package, factory).unwrap();

    let report = runtime.collect_once().await.unwrap();

    assert_eq!(report.samples_collected, 1);
    assert_eq!(
        runtime.shadow("meter-1").unwrap().latest_value("voltage"),
        Some(&TelemetryValue::Float(220.5))
    );
}

fn dlt645_connection() -> ProtocolConnection {
    ProtocolConnection::dlt645_serial(
        "meter-rs485-bus-1",
        SerialConnectionSettings::new("/dev/ttyUSB0", 2400),
    )
}

fn read_request(meter: [u8; 6], data_identifier: u32) -> Vec<u8> {
    let mut frame = vec![0x68];
    frame.extend(meter);
    frame.extend([0x68, 0x11, 0x04]);
    frame.extend(
        data_identifier
            .to_le_bytes()
            .into_iter()
            .map(|byte| byte.wrapping_add(0x33)),
    );
    append_dlt645_checksum(&mut frame);
    frame.push(0x16);
    frame
}

fn read_response(data_identifier: u32, value: &[u8]) -> Vec<u8> {
    let mut decoded = data_identifier.to_le_bytes().to_vec();
    decoded.extend_from_slice(value);
    let mut frame = vec![0x68];
    frame.extend(METER_ADDRESS);
    frame.extend([0x68, 0x91, decoded.len() as u8]);
    frame.extend(decoded.into_iter().map(|byte| byte.wrapping_add(0x33)));
    append_dlt645_checksum(&mut frame);
    frame.push(0x16);
    frame
}
