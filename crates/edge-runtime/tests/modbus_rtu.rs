use edge_core::{
    DataQuality, ModbusPointOptions, ModbusRegisterEncoding, ModbusWordOrder, PointAccess,
    PointAddress, ProtocolConnection, SerialConnectionSettings, TelemetryPointMapping,
    TelemetryType, TelemetryValue,
};
use edge_runtime::{
    append_modbus_rtu_crc, ModbusRtuAdapter, ProtocolAdapter, ProtocolCommandAdapter,
    ProtocolPointWrite, ScriptedSerialBus,
};

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
    let bus = ScriptedSerialBus::new(vec![response(1, &[220, 1])]);
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
    assert_eq!(requests.len(), 1);
    assert_eq!(&requests[0][..6], &[1, 0x03, 0, 0, 0, 2]);
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

#[tokio::test]
async fn modbus_rtu_adapter_decodes_low_word_first_float_with_scale_and_offset() {
    let connection = ProtocolConnection::modbus_rtu_serial(
        "meter-rs485-bus-1",
        SerialConnectionSettings::new("/dev/ttyUSB0", 9600),
    );
    let raw = 50.0_f32.to_be_bytes();
    let address =
        PointAddress::modbus_holding_register(40010).with_modbus_options(ModbusPointOptions {
            encoding: Some(ModbusRegisterEncoding::F32),
            word_order: ModbusWordOrder::LowWordFirst,
            scale: 0.2,
            offset: -1.0,
            ..Default::default()
        });
    let mappings = vec![TelemetryPointMapping::new(
        "temperature",
        "meter-1",
        "temperature",
        "meter-rs485-bus-1",
        address,
        TelemetryType::Float,
    )];
    let bus = ScriptedSerialBus::new(vec![response(
        1,
        &[
            u16::from_be_bytes([raw[2], raw[3]]),
            u16::from_be_bytes([raw[0], raw[1]]),
        ],
    )]);
    let observed = bus.clone();
    let mut adapter = ModbusRtuAdapter::new(connection, mappings, bus);

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(samples[0].value, TelemetryValue::Float(9.0));
    assert_eq!(&observed.requests()[0][..6], &[1, 0x03, 0, 9, 0, 2]);
}

#[tokio::test]
async fn modbus_rtu_adapter_reads_all_standard_point_areas() {
    let connection = ProtocolConnection::modbus_rtu_serial(
        "plc-rs485",
        SerialConnectionSettings::new("/dev/ttyUSB0", 9600),
    );
    let mappings = vec![
        mapping("coil", "00001", TelemetryType::Boolean),
        mapping("discrete_input", "10001", TelemetryType::Boolean),
        mapping("input_register", "30001", TelemetryType::Integer),
        mapping("holding_register", "40001", TelemetryType::Integer),
    ];
    let bus = ScriptedSerialBus::new(vec![
        bit_response(1, 0x01, true),
        bit_response(1, 0x02, false),
        register_response(1, 0x04, &[123]),
        register_response(1, 0x03, &[456]),
    ]);
    let observed_bus = bus.clone();
    let mut adapter = ModbusRtuAdapter::new(connection, mappings, bus);

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(samples[0].value, TelemetryValue::Boolean(true));
    assert_eq!(samples[1].value, TelemetryValue::Boolean(false));
    assert_eq!(samples[2].value, TelemetryValue::Integer(123));
    assert_eq!(samples[3].value, TelemetryValue::Integer(456));
    let functions = observed_bus
        .requests()
        .iter()
        .map(|request| request[1])
        .collect::<Vec<_>>();
    assert_eq!(functions, vec![0x01, 0x02, 0x04, 0x03]);
}

#[tokio::test]
async fn modbus_rtu_adapter_writes_coil_register_and_float_values() {
    let connection = ProtocolConnection::modbus_rtu_serial(
        "plc-rs485",
        SerialConnectionSettings::new("/dev/ttyUSB0", 9600),
    );
    let coil = mapping("coil", "00001", TelemetryType::Boolean);
    let coil = coil.with_access(PointAccess::ReadWrite);
    let register = mapping("holding_register", "40002", TelemetryType::Integer)
        .with_access(PointAccess::ReadWrite);
    let float = mapping("holding_register", "40003", TelemetryType::Float)
        .with_access(PointAccess::ReadWrite);
    let bus = ScriptedSerialBus::new(vec![
        write_response(1, 0x05, 0, 0xFF00),
        write_response(1, 0x06, 1, 321),
        write_response(1, 0x10, 2, 2),
    ]);
    let observed_bus = bus.clone();
    let mut adapter = ModbusRtuAdapter::new(connection, Vec::new(), bus);

    adapter
        .write_point(&coil, TelemetryValue::Boolean(true))
        .await
        .unwrap();
    adapter
        .write_point(&register, TelemetryValue::Integer(321))
        .await
        .unwrap();
    adapter
        .write_point(&float, TelemetryValue::Float(12.5))
        .await
        .unwrap();

    let requests = observed_bus.requests();
    assert_eq!(&requests[0][..6], &[1, 0x05, 0, 0, 0xFF, 0]);
    assert_eq!(&requests[1][..6], &[1, 0x06, 0, 1, 1, 65]);
    assert_eq!(&requests[2][..7], &[1, 0x10, 0, 2, 0, 2, 4]);
    assert_eq!(&requests[2][7..11], &12.5_f32.to_be_bytes());
}

#[tokio::test]
async fn modbus_rtu_adapter_batches_contiguous_coils_and_registers() {
    let connection = ProtocolConnection::modbus_rtu_serial(
        "plc-rs485",
        SerialConnectionSettings::new("/dev/ttyUSB0", 9600),
    );
    let coil_1 =
        mapping("coil", "00001", TelemetryType::Boolean).with_access(PointAccess::ReadWrite);
    let coil_2 =
        mapping("coil", "00002", TelemetryType::Boolean).with_access(PointAccess::ReadWrite);
    let register = mapping("holding_register", "40010", TelemetryType::Integer)
        .with_access(PointAccess::ReadWrite);
    let float = mapping("holding_register", "40011", TelemetryType::Float)
        .with_access(PointAccess::ReadWrite);
    let bus = ScriptedSerialBus::new(vec![
        write_response(1, 0x0F, 0, 2),
        write_response(1, 0x10, 9, 3),
    ]);
    let observed_bus = bus.clone();
    let mut adapter = ModbusRtuAdapter::new(connection, Vec::new(), bus);

    let coil_results = adapter
        .write_points(&[
            ProtocolPointWrite::new(coil_1, TelemetryValue::Boolean(true)),
            ProtocolPointWrite::new(coil_2, TelemetryValue::Boolean(false)),
        ])
        .await
        .unwrap();
    let register_results = adapter
        .write_points(&[
            ProtocolPointWrite::new(register, TelemetryValue::Integer(321)),
            ProtocolPointWrite::new(float, TelemetryValue::Float(12.5)),
        ])
        .await
        .unwrap();

    assert_eq!(coil_results.len(), 2);
    assert_eq!(register_results.len(), 2);
    let requests = observed_bus.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(&requests[0][..8], &[1, 0x0F, 0, 0, 0, 2, 1, 0x01]);
    assert_eq!(&requests[1][..7], &[1, 0x10, 0, 9, 0, 3, 6]);
    assert_eq!(&requests[1][7..9], &321_u16.to_be_bytes());
    assert_eq!(&requests[1][9..13], &12.5_f32.to_be_bytes());
}

#[tokio::test]
async fn modbus_rtu_adapter_rejects_writes_to_read_only_areas() {
    let connection = ProtocolConnection::modbus_rtu_serial(
        "plc-rs485",
        SerialConnectionSettings::new("/dev/ttyUSB0", 9600),
    );
    let input = mapping("input_register", "30001", TelemetryType::Integer)
        .with_access(PointAccess::ReadWrite);
    let mut adapter =
        ModbusRtuAdapter::new(connection, Vec::new(), ScriptedSerialBus::new(Vec::new()));

    let error = adapter
        .write_point(&input, TelemetryValue::Integer(1))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("read-only"));
}

#[tokio::test]
async fn modbus_rtu_adapter_rejects_writable_area_without_point_permission() {
    let connection = ProtocolConnection::modbus_rtu_serial(
        "plc-rs485",
        SerialConnectionSettings::new("/dev/ttyUSB0", 9600),
    );
    let holding_register = mapping("holding_register", "40001", TelemetryType::Integer);
    let bus = ScriptedSerialBus::new(Vec::new());
    let observed_bus = bus.clone();
    let mut adapter = ModbusRtuAdapter::new(connection, Vec::new(), bus);

    let error = adapter
        .write_point(&holding_register, TelemetryValue::Integer(1))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("not configured as writable"));
    assert!(observed_bus.requests().is_empty());
}

fn mapping(kind: &str, value: &str, value_type: TelemetryType) -> TelemetryPointMapping {
    TelemetryPointMapping::new(
        kind,
        "plc-1",
        kind,
        "plc-rs485",
        PointAddress {
            kind: kind.to_string(),
            value: value.to_string(),
            modbus: None,
        },
        value_type,
    )
}

fn response(slave_id: u8, registers: &[u16]) -> Vec<u8> {
    register_response(slave_id, 0x03, registers)
}

fn register_response(slave_id: u8, function: u8, registers: &[u16]) -> Vec<u8> {
    let mut frame = vec![slave_id, function, (registers.len() * 2) as u8];
    for register in registers {
        frame.extend(register.to_be_bytes());
    }
    append_modbus_rtu_crc(&mut frame);
    frame
}

fn bit_response(slave_id: u8, function: u8, value: bool) -> Vec<u8> {
    let mut frame = vec![slave_id, function, 1, u8::from(value)];
    append_modbus_rtu_crc(&mut frame);
    frame
}

fn write_response(slave_id: u8, function: u8, offset: u16, value_or_quantity: u16) -> Vec<u8> {
    let mut frame = vec![slave_id, function];
    frame.extend(offset.to_be_bytes());
    frame.extend(value_or_quantity.to_be_bytes());
    append_modbus_rtu_crc(&mut frame);
    frame
}
