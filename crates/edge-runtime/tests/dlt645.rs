use edge_core::{
    CollectionTask, DataQuality, DataQualityCode, DeviceInstance, EdgeConfigPackage, PointAddress,
    ProtocolConnection, SerialConnectionSettings, TelemetryPointMapping, TelemetryType,
    TelemetryValue,
};
use edge_runtime::{
    append_dlt645_checksum, ConfiguredEdgeRuntime, Dlt645Adapter, ProtocolAdapter,
    ScriptedSerialBus, ScriptedSerialBusFactory,
};

const METER_ADDRESS: [u8; 6] = [0x12, 0x90, 0x78, 0x56, 0x34, 0x12];
const SECOND_METER_ADDRESS: [u8; 6] = [0x21, 0x43, 0x65, 0x87, 0x09, 0x21];
const VOLTAGE_DI: u32 = 0x0201_0100;
const VENDOR_DI: u32 = 0xF001_0203;

#[tokio::test]
async fn dlt645_adapter_reads_scaled_bcd_telemetry() {
    let connection = dlt645_connection();
    let mappings = vec![TelemetryPointMapping::new(
        "voltage",
        "meter-1",
        "electric.voltage",
        "meter-rs485-bus-1",
        PointAddress::dlt645_scaled("123456789012", "02010100", 1),
        TelemetryType::Float,
    )];
    let response = read_response(METER_ADDRESS, VOLTAGE_DI, &[0x05, 0x22]);
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
    response.extend(read_response(
        METER_ADDRESS,
        0x0400_0401,
        &[0x56, 0x34, 0x12, 0x90, 0x78, 0x56],
    ));
    let mut adapter = Dlt645Adapter::new(
        dlt645_connection(),
        mappings,
        ScriptedSerialBus::new(vec![response]),
    );

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(
        samples[0].value,
        TelemetryValue::Text("567890123456".to_string())
    );
}

#[tokio::test]
async fn dlt645_adapter_reads_vendor_data_identifier_with_explicit_length_contract() {
    let mappings = vec![TelemetryPointMapping::new(
        "vendor_energy",
        "meter-1",
        "vendor.energy",
        "meter-rs485-bus-1",
        PointAddress::dlt645_vendor("123456789012", "F0010203", 2, 4),
        TelemetryType::Float,
    )];
    let response = read_response(METER_ADDRESS, VENDOR_DI, &[0x78, 0x56, 0x34, 0x12]);
    let mut adapter = Dlt645Adapter::new(
        dlt645_connection(),
        mappings,
        ScriptedSerialBus::new(vec![response]),
    );

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].value, TelemetryValue::Float(123456.78));
}

#[tokio::test]
async fn dlt645_adapter_rejects_vendor_response_with_unexpected_value_length() {
    let mappings = vec![TelemetryPointMapping::new(
        "vendor_energy",
        "meter-1",
        "vendor.energy",
        "meter-rs485-bus-1",
        PointAddress::dlt645_vendor("123456789012", "F0010203", 2, 4),
        TelemetryType::Float,
    )];
    let response = read_response(METER_ADDRESS, VENDOR_DI, &[0x78, 0x56, 0x34]);
    let mut adapter = Dlt645Adapter::new(
        dlt645_connection(),
        mappings,
        ScriptedSerialBus::new(vec![response]),
    );

    let error = adapter.read_telemetry().await.unwrap_err();

    assert!(error
        .to_string()
        .contains("data identifier F0010203 expects 4 value bytes, received 3"));
}

#[tokio::test]
async fn dlt645_adapter_rejects_standard_identifier_length_override() {
    let mappings = vec![TelemetryPointMapping::new(
        "voltage",
        "meter-1",
        "electric.voltage",
        "meter-rs485-bus-1",
        PointAddress::dlt645_vendor("123456789012", "02010100", 1, 3),
        TelemetryType::Float,
    )];
    let mut adapter = Dlt645Adapter::new(
        dlt645_connection(),
        mappings,
        ScriptedSerialBus::new(Vec::new()),
    );

    let error = adapter.read_telemetry().await.unwrap_err();

    assert!(error.to_string().contains("standard response length 2"));
}

#[tokio::test]
async fn dlt645_adapter_serializes_multi_meter_reads_and_deduplicates_queries() {
    let mappings = vec![
        TelemetryPointMapping::new(
            "voltage_a",
            "meter-1",
            "electric.voltage.a",
            "meter-rs485-bus-1",
            PointAddress::dlt645_scaled("123456789012", "02010100", 1),
            TelemetryType::Float,
        ),
        TelemetryPointMapping::new(
            "voltage_a_alias",
            "meter-1",
            "electric.voltage.a.raw",
            "meter-rs485-bus-1",
            PointAddress::dlt645_scaled("123456789012", "02010100", 1),
            TelemetryType::Float,
        ),
        TelemetryPointMapping::new(
            "current_a",
            "meter-2",
            "electric.current.a",
            "meter-rs485-bus-1",
            PointAddress::dlt645_scaled("210987654321", "02020100", 3),
            TelemetryType::Float,
        ),
    ];
    let bus = ScriptedSerialBus::new(vec![
        read_response(METER_ADDRESS, VOLTAGE_DI, &[0x05, 0x22]),
        read_response(SECOND_METER_ADDRESS, 0x0202_0100, &[0x34, 0x12, 0x00]),
    ]);
    let observed_bus = bus.clone();
    let mut adapter = Dlt645Adapter::new(dlt645_connection(), mappings, bus);

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(samples.len(), 3);
    assert_eq!(samples[0].value, TelemetryValue::Float(220.5));
    assert_eq!(samples[1].value, TelemetryValue::Float(220.5));
    assert_eq!(samples[2].value, TelemetryValue::Float(1.234));
    assert_eq!(observed_bus.requests().len(), 2);
    assert_eq!(
        observed_bus.requests()[0],
        read_request(METER_ADDRESS, VOLTAGE_DI)
    );
    assert_eq!(
        observed_bus.requests()[1],
        read_request(SECOND_METER_ADDRESS, 0x0202_0100)
    );
}

#[tokio::test]
async fn dlt645_adapter_isolates_failed_meter_and_continues_other_meters() {
    let mappings = multi_meter_mappings();
    let mut invalid_response = read_response(METER_ADDRESS, VOLTAGE_DI, &[0x05, 0x22]);
    let checksum_index = invalid_response.len() - 2;
    invalid_response[checksum_index] = invalid_response[checksum_index].wrapping_add(1);
    let bus = ScriptedSerialBus::new(vec![
        invalid_response,
        read_response(SECOND_METER_ADDRESS, 0x0202_0100, &[0x34, 0x12, 0x00]),
    ]);
    let observed_bus = bus.clone();
    let mut adapter = Dlt645Adapter::new(dlt645_connection(), mappings, bus);

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].device_id, "meter-bank");
    assert_eq!(samples[0].telemetry_id, "current_a");
    assert_eq!(samples[0].value, TelemetryValue::Float(1.234));
    assert_eq!(adapter.read_failures().len(), 1);
    assert_eq!(adapter.read_failures()[0].meter_address, "123456789012");
    assert_eq!(
        adapter.read_failures()[0].quality_code,
        DataQualityCode::BadProtocol
    );
    assert_eq!(adapter.read_failures()[0].point_count, 1);
    assert_eq!(observed_bus.requests().len(), 2);
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
    let mut response = read_response(METER_ADDRESS, 0, &[0x42, 0x00, 0x00, 0x00]);
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
            PointAddress::dlt645_scaled("123456789012", "02010100", 1),
            TelemetryType::Float,
        ))
        .with_collection_task(CollectionTask::interval(
            "meter-main",
            "meter-1",
            vec!["voltage".to_string()],
            1000,
        ));
    let bus = ScriptedSerialBus::new(vec![read_response(
        METER_ADDRESS,
        VOLTAGE_DI,
        &[0x05, 0x22],
    )]);
    let factory = ScriptedSerialBusFactory::new(vec![("meter-rs485-bus-1".to_string(), bus)]);
    let mut runtime = ConfiguredEdgeRuntime::new(package, factory).unwrap();

    let report = runtime.collect_once().await.unwrap();

    assert_eq!(report.samples_collected, 1);
    assert_eq!(
        runtime.shadow("meter-1").unwrap().latest_value("voltage"),
        Some(&TelemetryValue::Float(220.5))
    );
}

#[tokio::test]
async fn configured_runtime_records_partial_dlt645_failure_without_publishing_fake_value() {
    let mappings = multi_meter_mappings();
    let mut invalid_response = read_response(METER_ADDRESS, VOLTAGE_DI, &[0x05, 0x22]);
    let checksum_index = invalid_response.len() - 2;
    invalid_response[checksum_index] = invalid_response[checksum_index].wrapping_add(1);
    let package = EdgeConfigPackage::new("edge-meter", "2026.08.04-dlt645-isolation")
        .with_device(DeviceInstance::new("meter-bank", "power-meter-bank"))
        .with_protocol_connection(dlt645_connection())
        .with_point_mapping(mappings[0].clone())
        .with_point_mapping(mappings[1].clone())
        .with_collection_task(CollectionTask::interval(
            "meter-bank-main",
            "meter-bank",
            vec!["voltage_a".to_string(), "current_a".to_string()],
            1000,
        ));
    let bus = ScriptedSerialBus::new(vec![
        invalid_response,
        read_response(SECOND_METER_ADDRESS, 0x0202_0100, &[0x34, 0x12, 0x00]),
    ]);
    let factory = ScriptedSerialBusFactory::new(vec![("meter-rs485-bus-1".to_string(), bus)]);
    let mut runtime = ConfiguredEdgeRuntime::new(package, factory).unwrap();

    let report = runtime.collect_once().await.unwrap();

    assert_eq!(report.samples_collected, 1);
    let shadow = runtime.shadow("meter-bank").unwrap();
    assert_eq!(shadow.latest_value("voltage_a"), None);
    assert_eq!(
        shadow.latest_value("current_a"),
        Some(&TelemetryValue::Float(1.234))
    );
    let metrics = runtime.protocol_runtime_metrics();
    assert_eq!(metrics[0].collection_success_count, 1);
    assert_eq!(metrics[0].good_value_count, 1);
    assert_eq!(metrics[0].bad_value_count, 1);
    assert_eq!(metrics[0].error_count, 1);
    assert_eq!(
        metrics[0].last_quality_code,
        Some(DataQualityCode::BadProtocol)
    );
}

fn dlt645_connection() -> ProtocolConnection {
    ProtocolConnection::dlt645_serial(
        "meter-rs485-bus-1",
        SerialConnectionSettings::new("/dev/ttyUSB0", 2400),
    )
}

fn multi_meter_mappings() -> Vec<TelemetryPointMapping> {
    vec![
        TelemetryPointMapping::new(
            "voltage_a",
            "meter-bank",
            "electric.voltage.a",
            "meter-rs485-bus-1",
            PointAddress::dlt645_scaled("123456789012", "02010100", 1),
            TelemetryType::Float,
        ),
        TelemetryPointMapping::new(
            "current_a",
            "meter-bank",
            "electric.current.a",
            "meter-rs485-bus-1",
            PointAddress::dlt645_scaled("210987654321", "02020100", 3),
            TelemetryType::Float,
        ),
    ]
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

fn read_response(meter: [u8; 6], data_identifier: u32, value: &[u8]) -> Vec<u8> {
    let mut decoded = data_identifier.to_le_bytes().to_vec();
    decoded.extend_from_slice(value);
    let mut frame = vec![0x68];
    frame.extend(meter);
    frame.extend([0x68, 0x91, decoded.len() as u8]);
    frame.extend(decoded.into_iter().map(|byte| byte.wrapping_add(0x33)));
    append_dlt645_checksum(&mut frame);
    frame.push(0x16);
    frame
}
