use edge_core::{
    DiscoveryRequest, ProtocolConnection, SerialConnectionSettings, TelemetryType,
    MAX_DISCOVERY_POINTS,
};
use edge_runtime::{append_modbus_rtu_crc, ModbusRtuDiscovery, ScriptedSerialBus};

#[tokio::test]
async fn modbus_discovery_reads_only_requested_holding_registers() {
    let connection = ProtocolConnection::modbus_rtu_serial(
        "meter-rs485-bus-1",
        SerialConnectionSettings::new("/dev/ttyUSB0", 9600),
    );
    let request =
        DiscoveryRequest::modbus_holding_registers("job-1", "meter-rs485-bus-1", 40001, 40002)
            .with_slave_id(3);
    let bus = ScriptedSerialBus::new(vec![response(3, 220), response(3, 1)]);
    let observed_bus = bus.clone();
    let mut discovery = ModbusRtuDiscovery::new(connection, request, bus);

    let report = discovery.run().await.unwrap();

    assert_eq!(report.job_id, "job-1");
    assert_eq!(report.discovered_points.len(), 2);
    assert_eq!(report.discovered_points[0].address.value, "40001");
    assert_eq!(
        report.discovered_points[0].value_type,
        TelemetryType::Integer
    );
    assert_eq!(report.discovered_points[0].sample_values, vec!["220"]);
    assert_eq!(report.discovered_points[1].address.value, "40002");
    assert!(report.suggestions.is_empty());
    let requests = observed_bus.requests();
    assert_eq!(&requests[0][..6], &[3, 0x03, 0, 0, 0, 1]);
    assert_eq!(&requests[1][..6], &[3, 0x03, 0, 1, 0, 1]);
}

#[tokio::test]
async fn modbus_discovery_skips_exception_responses_without_inventing_points() {
    let connection = ProtocolConnection::modbus_rtu_serial(
        "meter-rs485-bus-1",
        SerialConnectionSettings::new("/dev/ttyUSB0", 9600),
    );
    let request =
        DiscoveryRequest::modbus_holding_registers("job-2", "meter-rs485-bus-1", 40001, 40002);
    let bus = ScriptedSerialBus::new(vec![exception(1, 2), response(1, 9)]);
    let mut discovery = ModbusRtuDiscovery::new(connection, request, bus);

    let report = discovery.run().await.unwrap();

    assert_eq!(report.discovered_points.len(), 1);
    assert_eq!(report.discovered_points[0].address.value, "40002");
    assert_eq!(report.discovered_points[0].sample_values, vec!["9"]);
}

#[test]
fn discovery_request_enforces_range_and_slave_safety_limits() {
    let oversized = DiscoveryRequest::modbus_holding_registers(
        "job-3",
        "meter-rs485-bus-1",
        40001,
        40001 + u32::from(MAX_DISCOVERY_POINTS),
    );
    assert!(oversized.validate().unwrap_err().contains("safety limit"));

    let broadcast =
        DiscoveryRequest::modbus_holding_registers("job-4", "meter-rs485-bus-1", 40001, 40001)
            .with_slave_id(0);
    assert!(broadcast
        .validate()
        .unwrap_err()
        .contains("between 1 and 247"));
}

fn response(slave_id: u8, register: u16) -> Vec<u8> {
    let mut frame = vec![slave_id, 0x03, 2];
    frame.extend(register.to_be_bytes());
    append_modbus_rtu_crc(&mut frame);
    frame
}

fn exception(slave_id: u8, code: u8) -> Vec<u8> {
    let mut frame = vec![slave_id, 0x83, code];
    append_modbus_rtu_crc(&mut frame);
    frame
}
