use chrono::{Duration, TimeZone, Timelike, Utc};
use edge_core::{
    CollectionTask, DataQuality, DeviceInstance, EdgeConfigPackage, Iec101ConnectionSettings,
    Iec101ControlType, Iec101PointOptions, PointAccess, PointAddress, ProtocolConnection,
    SerialConnectionSettings, TelemetryPointMapping, TelemetryType, TelemetryValue,
};
use edge_runtime::{
    append_iec101_checksum, ConfiguredEdgeRuntime, Iec101Adapter, ProtocolAdapter,
    ProtocolCommandAdapter, ScriptedSerialBus, ScriptedSerialBusFactory,
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
async fn iec101_adapter_uses_cp56_timestamp_for_short_float_samples() {
    let mappings = vec![TelemetryPointMapping::new(
        "line_voltage",
        "bay-1",
        "electric.voltage",
        "substation-iec101",
        PointAddress::iec101(1, 2, 2002),
        TelemetryType::Float,
    )];
    let mut information = 110.5_f32.to_le_bytes().to_vec();
    information.push(0x00);
    information.extend(cp56_time(2026, 7, 16, 8, 9, 10, 250, false));
    let bus = ScriptedSerialBus::new(vec![
        vec![0xE5],
        monitoring_response(1, 2, 2002, 36, &information),
    ]);
    let connection = iec101_connection().with_iec101_settings(
        Iec101ConnectionSettings::default().with_cp56_timezone_offset_minutes(480),
    );
    let mut adapter = Iec101Adapter::new(connection, mappings, bus);

    let samples = adapter.read_telemetry().await.unwrap();

    let expected = Utc
        .with_ymd_and_hms(2026, 7, 16, 0, 9, 10)
        .single()
        .unwrap()
        + Duration::milliseconds(250);
    assert_eq!(samples[0].timestamp, expected);
    assert_eq!(samples[0].quality, DataQuality::Good);
}

#[tokio::test]
async fn iec101_adapter_reconstructs_cp24_timestamp_in_the_nearest_hour() {
    let mappings = vec![TelemetryPointMapping::new(
        "breaker_closed",
        "bay-1",
        "breaker.closed",
        "substation-iec101",
        PointAddress::iec101(1, 2, 1002),
        TelemetryType::Boolean,
    )];
    let mut information = vec![0x01];
    information.extend(cp24_time(9, 10, 250, false));
    let bus = ScriptedSerialBus::new(vec![
        vec![0xE5],
        monitoring_response(1, 2, 1002, 2, &information),
    ]);
    let mut adapter = Iec101Adapter::new(iec101_connection(), mappings, bus);

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(samples[0].timestamp.minute(), 9);
    assert_eq!(samples[0].timestamp.second(), 10);
    assert_eq!(samples[0].timestamp.timestamp_subsec_millis(), 250);
    assert!((samples[0].timestamp - Utc::now()).num_minutes().abs() <= 31);
}

#[tokio::test]
async fn iec101_adapter_marks_value_uncertain_when_cp56_timestamp_is_invalid() {
    let mappings = vec![TelemetryPointMapping::new(
        "breaker_closed",
        "bay-1",
        "breaker.closed",
        "substation-iec101",
        PointAddress::iec101(1, 2, 1003),
        TelemetryType::Boolean,
    )];
    let mut information = vec![0x01];
    information.extend(cp56_time(2026, 7, 16, 8, 9, 10, 250, true));
    let bus = ScriptedSerialBus::new(vec![
        vec![0xE5],
        monitoring_response(1, 2, 1003, 30, &information),
    ]);
    let mut adapter = Iec101Adapter::new(iec101_connection(), mappings, bus);

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(samples[0].value, TelemetryValue::Boolean(true));
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
async fn iec101_adapter_executes_single_command_after_positive_confirmation() {
    let mapping = writable_mapping(
        "breaker_close",
        PointAddress::iec101(1, 2, 1201),
        TelemetryType::Boolean,
        Iec101ControlType::SingleCommand,
        false,
    );
    let bus = ScriptedSerialBus::new(vec![
        vec![0xE5],
        command_confirmation(1, 2, 1201, 45, false, &[0x01]),
    ]);
    let observed_bus = bus.clone();
    let mut adapter = Iec101Adapter::new(iec101_connection(), vec![mapping.clone()], bus);

    let result = adapter
        .write_point(&mapping, TelemetryValue::Boolean(true))
        .await
        .unwrap();

    assert!(result.verified);
    assert_eq!(result.value, TelemetryValue::Boolean(true));
    assert_eq!(
        observed_bus.requests(),
        vec![
            vec![0x10, 0x40, 0x01, 0x41, 0x16],
            control_request(0x53, 1, 2, 1201, 45, &[0x01]),
        ]
    );
}

#[tokio::test]
async fn iec101_adapter_executes_select_before_operate_with_class_1_polling() {
    let mapping = writable_mapping(
        "breaker_position",
        PointAddress::iec101(1, 2, 1202),
        TelemetryType::Integer,
        Iec101ControlType::DoubleCommand,
        true,
    );
    let bus = ScriptedSerialBus::new(vec![
        vec![0xE5],
        vec![0xE5],
        command_confirmation(1, 2, 1202, 46, false, &[0x82]),
        vec![0xE5],
        command_confirmation(1, 2, 1202, 46, false, &[0x02]),
    ]);
    let observed_bus = bus.clone();
    let mut adapter = Iec101Adapter::new(iec101_connection(), vec![mapping.clone()], bus);

    let result = adapter
        .write_point(&mapping, TelemetryValue::Integer(2))
        .await
        .unwrap();

    assert!(result.verified);
    assert_eq!(
        observed_bus.requests(),
        vec![
            vec![0x10, 0x40, 0x01, 0x41, 0x16],
            control_request(0x53, 1, 2, 1202, 46, &[0x82]),
            vec![0x10, 0x7A, 0x01, 0x7B, 0x16],
            control_request(0x73, 1, 2, 1202, 46, &[0x02]),
            vec![0x10, 0x5A, 0x01, 0x5B, 0x16],
        ]
    );
}

#[tokio::test]
async fn iec101_adapter_rejects_negative_activation_confirmation() {
    let mapping = writable_mapping(
        "active_power_setpoint",
        PointAddress::iec101(1, 2, 1203),
        TelemetryType::Float,
        Iec101ControlType::SetpointFloat,
        false,
    );
    let bus = ScriptedSerialBus::new(vec![
        vec![0xE5],
        command_confirmation(1, 2, 1203, 50, true, &[0, 0, 0, 0, 0]),
    ]);
    let mut adapter = Iec101Adapter::new(iec101_connection(), vec![mapping.clone()], bus);

    let error = adapter
        .write_point(&mapping, TelemetryValue::Float(42.5))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("rejected by the station"));
}

#[tokio::test]
async fn iec101_adapter_executes_short_float_setpoint_after_positive_confirmation() {
    let mapping = writable_mapping(
        "active_power_setpoint",
        PointAddress::iec101(1, 2, 1203),
        TelemetryType::Float,
        Iec101ControlType::SetpointFloat,
        false,
    );
    let mut information = 42.5_f32.to_le_bytes().to_vec();
    information.push(0);
    let bus = ScriptedSerialBus::new(vec![
        vec![0xE5],
        command_confirmation(1, 2, 1203, 50, false, &information),
    ]);
    let observed_bus = bus.clone();
    let mut adapter = Iec101Adapter::new(iec101_connection(), vec![mapping.clone()], bus);

    let result = adapter
        .write_point(&mapping, TelemetryValue::Float(42.5))
        .await
        .unwrap();

    assert!(result.verified);
    assert_eq!(result.value, TelemetryValue::Float(42.5));
    assert_eq!(
        observed_bus.requests(),
        vec![
            vec![0x10, 0x40, 0x01, 0x41, 0x16],
            control_request(0x53, 1, 2, 1203, 50, &information),
        ]
    );
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

fn writable_mapping(
    point_id: &str,
    address: PointAddress,
    value_type: TelemetryType,
    control_type: Iec101ControlType,
    select_before_operate: bool,
) -> TelemetryPointMapping {
    TelemetryPointMapping::new(
        point_id,
        "bay-1",
        point_id,
        "substation-iec101",
        address,
        value_type,
    )
    .with_access(PointAccess::ReadWrite)
    .with_iec101_options(
        Iec101PointOptions::new(control_type).with_select_before_operate(select_before_operate),
    )
}

fn control_request(
    control: u8,
    link_address: u8,
    common_address: u16,
    ioa: u32,
    type_id: u8,
    information: &[u8],
) -> Vec<u8> {
    let mut body = vec![control, link_address, type_id, 1, 6, 0];
    body.extend(common_address.to_le_bytes());
    body.extend([ioa as u8, (ioa >> 8) as u8, (ioa >> 16) as u8]);
    body.extend_from_slice(information);
    variable_frame(body)
}

fn command_confirmation(
    link_address: u8,
    common_address: u16,
    ioa: u32,
    type_id: u8,
    negative: bool,
    information: &[u8],
) -> Vec<u8> {
    let cause = 7 | if negative { 0x40 } else { 0 };
    let mut body = vec![0x08, link_address, type_id, 1, cause, 0];
    body.extend(common_address.to_le_bytes());
    body.extend([ioa as u8, (ioa >> 8) as u8, (ioa >> 16) as u8]);
    body.extend_from_slice(information);
    variable_frame(body)
}

fn variable_frame(body: Vec<u8>) -> Vec<u8> {
    let length = body.len() as u8;
    let mut frame = vec![0x68, length, length, 0x68];
    frame.extend(body);
    append_iec101_checksum(&mut frame, 4);
    frame.push(0x16);
    frame
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

fn cp24_time(minute: u8, second: u8, millisecond: u16, invalid: bool) -> [u8; 3] {
    let milliseconds = u16::from(second) * 1_000 + millisecond;
    let [low, high] = milliseconds.to_le_bytes();
    [low, high, minute | if invalid { 0x80 } else { 0 }]
}

#[allow(clippy::too_many_arguments)]
fn cp56_time(
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    millisecond: u16,
    invalid: bool,
) -> [u8; 7] {
    assert!((2000..=2099).contains(&year));
    let [low, high] = (u16::from(second) * 1_000 + millisecond).to_le_bytes();
    [
        low,
        high,
        minute | if invalid { 0x80 } else { 0 },
        hour,
        day,
        month,
        (year - 2000) as u8,
    ]
}
