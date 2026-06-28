use edge_core::{
    DataQuality, PointAddress, ProtocolConnection, SerialConnectionSettings, TelemetryPointMapping,
    TelemetryType, TelemetryValue,
};
use edge_runtime::{append_modbus_rtu_crc, ModbusRtuAdapter, ProtocolAdapter, ScriptedSerialBus};

#[tokio::test]
async fn modbus_rtu_adapter_reads_holding_register_points() {
    let connection = ProtocolConnection::modbus_rtu_serial(
        "meter-rs485-bus-1",
        SerialConnectionSettings::new("/dev/ttyUSB0", 9600),
    );
    let mappings = vec![
        TelemetryPointMapping::new(
            "voltage",
            "meter-1",
            "voltage",
            "meter-rs485-bus-1",
            PointAddress::modbus_holding_register(40001),
            TelemetryType::Integer,
        ),
        TelemetryPointMapping::new(
            "running",
            "meter-1",
            "running",
            "meter-rs485-bus-1",
            PointAddress::modbus_holding_register(40002),
            TelemetryType::Boolean,
        ),
    ];
    let bus = ScriptedSerialBus::new(vec![response(1, &[220]), response(1, &[1])]);
    let observed_bus = bus.clone();
    let mut adapter = ModbusRtuAdapter::new(connection, mappings, bus);

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0].device_id, "meter-1");
    assert_eq!(samples[0].telemetry_id, "voltage");
    assert_eq!(samples[0].value, TelemetryValue::Integer(220));
    assert_eq!(samples[0].quality, DataQuality::Good);
    assert_eq!(samples[1].telemetry_id, "running");
    assert_eq!(samples[1].value, TelemetryValue::Boolean(true));

    let requests = observed_bus.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(&requests[0][..6], &[1, 0x03, 0, 0, 0, 1]);
    assert_eq!(&requests[1][..6], &[1, 0x03, 0, 1, 0, 1]);
}

#[tokio::test]
async fn modbus_rtu_adapter_decodes_float_from_two_registers() {
    let connection = ProtocolConnection::modbus_rtu_serial(
        "meter-rs485-bus-1",
        SerialConnectionSettings::new("/dev/ttyUSB0", 9600),
    );
    let mappings = vec![TelemetryPointMapping::new(
        "temperature",
        "meter-1",
        "temperature",
        "meter-rs485-bus-1",
        PointAddress::modbus_holding_register(40010),
        TelemetryType::Float,
    )];
    let bus = ScriptedSerialBus::new(vec![response(1, &[0x41C8, 0x0000])]);
    let mut adapter = ModbusRtuAdapter::new(connection, mappings, bus);

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].value, TelemetryValue::Float(25.0));
}

fn response(slave_id: u8, registers: &[u16]) -> Vec<u8> {
    let mut frame = vec![slave_id, 0x03, (registers.len() * 2) as u8];
    for register in registers {
        frame.extend(register.to_be_bytes());
    }
    append_modbus_rtu_crc(&mut frame);
    frame
}
