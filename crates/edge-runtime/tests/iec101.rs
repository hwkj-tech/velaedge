use edge_core::{
    CollectionTask, DataQuality, DeviceInstance, EdgeConfigPackage, PointAddress,
    ProtocolConnection, SerialConnectionSettings, TelemetryPointMapping, TelemetryType,
    TelemetryValue,
};
use edge_runtime::{
    append_iec101_checksum, ConfiguredEdgeRuntime, Iec101Adapter, ProtocolAdapter,
    ScriptedSerialBus, ScriptedSerialBusFactory,
};

#[tokio::test]
async fn iec101_adapter_resets_link_reads_point_and_polls_class_2_data() {
    let mappings = vec![TelemetryPointMapping::new(
        "breaker_closed",
        "bay-1",
        "breaker.closed",
        "substation-iec101",
        PointAddress::iec101(1, 2, 1001),
        TelemetryType::Boolean,
    )];
    let bus = ScriptedSerialBus::new(vec![
        vec![0xE5],
        vec![0xE5],
        monitoring_response(1, 2, 1001, 1, &[0x01]),
    ]);
    let observed_bus = bus.clone();
    let mut adapter = Iec101Adapter::new(iec101_connection(), mappings, bus);

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].device_id, "bay-1");
    assert_eq!(samples[0].telemetry_id, "breaker_closed");
    assert_eq!(samples[0].value, TelemetryValue::Boolean(true));
    assert_eq!(samples[0].quality, DataQuality::Good);

    let requests = observed_bus.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0], vec![0x10, 0x40, 0x01, 0x41, 0x16]);
    assert_eq!(requests[1][0], 0x68);
    assert_eq!(requests[1][4], 0x53);
    assert_eq!(requests[1][6], 102);
    assert_eq!(&requests[1][12..15], &[0xE9, 0x03, 0x00]);
    assert_eq!(requests[2], vec![0x10, 0x7B, 0x01, 0x7C, 0x16]);
}

#[tokio::test]
async fn iec101_adapter_decodes_short_float_direct_response_and_quality() {
    let mappings = vec![TelemetryPointMapping::new(
        "line_voltage",
        "bay-1",
        "electric.voltage",
        "substation-iec101",
        PointAddress::iec101(1, 2, 2001),
        TelemetryType::Float,
    )];
    let mut information = 110.5_f32.to_le_bytes().to_vec();
    information.push(0x20);
    let bus = ScriptedSerialBus::new(vec![
        vec![0xE5],
        monitoring_response(1, 2, 2001, 13, &information),
    ]);
    let mut adapter = Iec101Adapter::new(iec101_connection(), mappings, bus);

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(samples[0].value, TelemetryValue::Float(110.5));
    assert_eq!(samples[0].quality, DataQuality::Uncertain);
}

#[tokio::test]
async fn iec101_adapter_rejects_corrupt_ft12_checksum() {
    let mappings = vec![TelemetryPointMapping::new(
        "active_power",
        "bay-1",
        "electric.active_power",
        "substation-iec101",
        PointAddress::iec101(1, 2, 3001),
        TelemetryType::Integer,
    )];
    let mut response = monitoring_response(1, 2, 3001, 11, &[0x2A, 0x00, 0x00]);
    let checksum_index = response.len() - 2;
    response[checksum_index] = response[checksum_index].wrapping_add(1);
    let bus = ScriptedSerialBus::new(vec![vec![0xE5], response]);
    let mut adapter = Iec101Adapter::new(iec101_connection(), mappings, bus);

    let error = adapter.read_telemetry().await.unwrap_err();

    assert!(error
        .to_string()
        .contains("IEC 101 variable frame checksum mismatch"));
}

#[tokio::test]
async fn configured_runtime_executes_iec101_cloud_config() {
    let package = EdgeConfigPackage::new("edge-substation", "2026.07.15-iec101")
        .with_device(DeviceInstance::new("bay-1", "substation-bay"))
        .with_protocol_connection(iec101_connection())
        .with_point_mapping(TelemetryPointMapping::new(
            "breaker_closed",
            "bay-1",
            "breaker.closed",
            "substation-iec101",
            PointAddress::iec101(1, 2, 1001),
            TelemetryType::Boolean,
        ))
        .with_collection_task(CollectionTask::interval(
            "bay-status",
            "bay-1",
            vec!["breaker_closed".to_string()],
            1000,
        ));
    let bus = ScriptedSerialBus::new(vec![
        vec![0xE5],
        monitoring_response(1, 2, 1001, 1, &[0x01]),
    ]);
    let factory = ScriptedSerialBusFactory::new(vec![("substation-iec101".to_string(), bus)]);
    let mut runtime = ConfiguredEdgeRuntime::new(package, factory).unwrap();

    let report = runtime.collect_once().await.unwrap();

    assert_eq!(report.samples_collected, 1);
    assert_eq!(
        runtime
            .shadow("bay-1")
            .unwrap()
            .latest_value("breaker_closed"),
        Some(&TelemetryValue::Boolean(true))
    );
}

fn iec101_connection() -> ProtocolConnection {
    ProtocolConnection::iec101_serial(
        "substation-iec101",
        SerialConnectionSettings::new("/dev/ttyUSB1", 9600).with_parity("even"),
    )
}

fn monitoring_response(
    link_address: u8,
    common_address: u16,
    information_object_address: u32,
    type_id: u8,
    information: &[u8],
) -> Vec<u8> {
    let mut body = vec![0x08, link_address, type_id, 1, 5, 0];
    body.extend(common_address.to_le_bytes());
    body.extend([
        information_object_address as u8,
        (information_object_address >> 8) as u8,
        (information_object_address >> 16) as u8,
    ]);
    body.extend_from_slice(information);

    let length = body.len() as u8;
    let mut frame = vec![0x68, length, length, 0x68];
    frame.extend(body);
    append_iec101_checksum(&mut frame, 4);
    frame.push(0x16);
    frame
}
