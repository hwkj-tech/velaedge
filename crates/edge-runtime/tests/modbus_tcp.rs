use std::{collections::BTreeMap, net::SocketAddr, time::Duration};

use edge_core::{
    DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint, DataConfigPublish,
    DataQuality, DeviceInstance, EdgeConfigPackage, ModbusByteOrder, ModbusPointOptions,
    ModbusRegisterEncoding, ModbusWordOrder, MqttUplinkConfig, PointAccess, PointAddress,
    ProtocolConnection, TelemetryPointMapping, TelemetryType, TelemetryValue,
};
use edge_runtime::{
    ConfiguredEdgeRuntime, ModbusTcpAdapter, ModbusTcpSimulator, ModbusTcpSimulatorOptions,
    ProtocolAdapter, ProtocolCommandAdapter, ProtocolPointWrite, RocksEdgeRuntimeStore,
    RumqttcMqttPublisher, ScriptedSerialBusFactory,
};
use tempfile::tempdir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};

#[tokio::test]
async fn modbus_tcp_adapter_reads_registers_and_coils_over_a_real_socket() {
    let (endpoint, server) = spawn_simulator().await;
    let connection = ProtocolConnection::modbus_tcp("modbus-main", endpoint);
    let mappings = vec![
        TelemetryPointMapping::new(
            "pressure",
            "pump-1",
            "pump.pressure",
            "modbus-main",
            PointAddress::modbus_holding_register(40001),
            TelemetryType::Float,
        ),
        TelemetryPointMapping::new(
            "temperature",
            "pump-1",
            "pump.temperature",
            "modbus-main",
            PointAddress {
                kind: "input_register".to_string(),
                value: "30001".to_string(),
                modbus: None,
            },
            TelemetryType::Integer,
        ),
        TelemetryPointMapping::new(
            "running",
            "pump-1",
            "pump.running",
            "modbus-main",
            PointAddress {
                kind: "coil".to_string(),
                value: "00001".to_string(),
                modbus: None,
            },
            TelemetryType::Boolean,
        ),
    ];
    let mut adapter = ModbusTcpAdapter::new(connection, mappings)
        .with_timeouts(Duration::from_secs(1), Duration::from_secs(1));

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(samples.len(), 3);
    assert_eq!(samples[0].value, TelemetryValue::Float(2.5));
    assert_eq!(samples[1].value, TelemetryValue::Integer(36));
    assert_eq!(samples[2].value, TelemetryValue::Boolean(true));
    assert!(samples
        .iter()
        .all(|sample| sample.quality == DataQuality::Good));
    server.abort();
}

#[tokio::test]
async fn modbus_tcp_adapter_reads_discrete_inputs_over_a_real_socket() {
    let mut options = ModbusTcpSimulatorOptions::new("127.0.0.1:0".parse().unwrap());
    options.discrete_inputs.insert(0, true);
    let simulator = ModbusTcpSimulator::bind(options).await.unwrap();
    let endpoint = simulator.local_addr().unwrap().to_string();
    let server = tokio::spawn(simulator.run());
    let connection = ProtocolConnection::modbus_tcp("modbus-main", endpoint);
    let mappings = vec![TelemetryPointMapping::new(
        "alarm",
        "pump-1",
        "pump.alarm",
        "modbus-main",
        PointAddress {
            kind: "discrete_input".to_string(),
            value: "10001".to_string(),
            modbus: None,
        },
        TelemetryType::Boolean,
    )];
    let mut adapter = ModbusTcpAdapter::new(connection, mappings);

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].value, TelemetryValue::Boolean(true));
    server.abort();
}

#[tokio::test]
async fn modbus_tcp_adapter_merges_contiguous_points_into_bounded_read_windows() {
    let mut options = ModbusTcpSimulatorOptions::new("127.0.0.1:0".parse().unwrap());
    options.holding_registers.insert(0, 220);
    options.holding_registers.insert(1, 1);
    options.holding_registers.insert(2, 0x41C8);
    options.holding_registers.insert(3, 0);
    options.coils.insert(0, true);
    options.coils.insert(1, false);
    let simulator = ModbusTcpSimulator::bind(options).await.unwrap();
    let endpoint = simulator.local_addr().unwrap().to_string();
    let metrics = simulator.metrics();
    let server = tokio::spawn(simulator.run());
    let connection = ProtocolConnection::modbus_tcp("modbus-main", endpoint);
    let mappings = vec![
        tcp_mapping(
            "voltage",
            "holding_register",
            "40001",
            TelemetryType::Integer,
        ),
        tcp_mapping(
            "running",
            "holding_register",
            "40002",
            TelemetryType::Boolean,
        ),
        tcp_mapping(
            "temperature",
            "holding_register",
            "40003",
            TelemetryType::Float,
        ),
        tcp_mapping("enabled", "coil", "00001", TelemetryType::Boolean),
        tcp_mapping("alarm", "coil", "00002", TelemetryType::Boolean),
    ];
    let mut adapter = ModbusTcpAdapter::new(connection, mappings);

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(samples.len(), 5);
    assert_eq!(samples[0].value, TelemetryValue::Integer(220));
    assert_eq!(samples[1].value, TelemetryValue::Boolean(true));
    assert_eq!(samples[2].value, TelemetryValue::Float(25.0));
    assert_eq!(samples[3].value, TelemetryValue::Boolean(true));
    assert_eq!(samples[4].value, TelemetryValue::Boolean(false));
    assert_eq!(metrics.requests_total(), 2);
    server.abort();
}

#[tokio::test]
async fn modbus_tcp_adapter_decodes_per_point_register_layouts_in_one_read_window() {
    let mut options = ModbusTcpSimulatorOptions::new("127.0.0.1:0".parse().unwrap());
    options.holding_registers.insert(0, 0xFEFF);
    let raw_float = 100.0_f32.to_be_bytes();
    options
        .holding_registers
        .insert(1, u16::from_be_bytes([raw_float[2], raw_float[3]]));
    options
        .holding_registers
        .insert(2, u16::from_be_bytes([raw_float[0], raw_float[1]]));
    options.holding_registers.insert(3, 0x0020);
    let simulator = ModbusTcpSimulator::bind(options).await.unwrap();
    let endpoint = simulator.local_addr().unwrap().to_string();
    let metrics = simulator.metrics();
    let server = tokio::spawn(simulator.run());

    let signed =
        PointAddress::modbus_holding_register(40001).with_modbus_options(ModbusPointOptions {
            encoding: Some(ModbusRegisterEncoding::I16),
            byte_order: ModbusByteOrder::LittleEndian,
            ..Default::default()
        });
    let engineering =
        PointAddress::modbus_holding_register(40002).with_modbus_options(ModbusPointOptions {
            encoding: Some(ModbusRegisterEncoding::F32),
            word_order: ModbusWordOrder::LowWordFirst,
            scale: 0.1,
            offset: 1.0,
            ..Default::default()
        });
    let ready =
        PointAddress::modbus_holding_register(40004).with_modbus_options(ModbusPointOptions {
            bit_index: Some(5),
            ..Default::default()
        });
    let mappings = vec![
        TelemetryPointMapping::new(
            "signed",
            "device-1",
            "signed",
            "modbus-main",
            signed,
            TelemetryType::Integer,
        ),
        TelemetryPointMapping::new(
            "engineering",
            "device-1",
            "engineering",
            "modbus-main",
            engineering,
            TelemetryType::Float,
        ),
        TelemetryPointMapping::new(
            "ready",
            "device-1",
            "ready",
            "modbus-main",
            ready,
            TelemetryType::Boolean,
        ),
    ];
    let connection = ProtocolConnection::modbus_tcp("modbus-main", endpoint);
    let mut adapter = ModbusTcpAdapter::new(connection, mappings);

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(samples[0].value, TelemetryValue::Integer(-2));
    assert_eq!(samples[1].value, TelemetryValue::Float(11.0));
    assert_eq!(samples[2].value, TelemetryValue::Boolean(true));
    assert_eq!(metrics.requests_total(), 1);
    server.abort();
}

#[tokio::test]
async fn modbus_tcp_adapter_writes_float_over_a_real_socket() {
    let (endpoint, request, server) = spawn_write_echo_server().await;
    let connection = ProtocolConnection::modbus_tcp("modbus-main", endpoint);
    let mapping = TelemetryPointMapping::new(
        "setpoint",
        "pump-1",
        "pump.setpoint",
        "modbus-main",
        PointAddress::modbus_holding_register(40010),
        TelemetryType::Float,
    )
    .with_access(PointAccess::ReadWrite);
    let mut adapter = ModbusTcpAdapter::new(connection, Vec::new());

    let result = adapter
        .write_point(&mapping, TelemetryValue::Float(12.5))
        .await
        .unwrap();

    assert!(result.verified);
    let request = request.await.unwrap();
    assert_eq!(request[7], 0x10);
    assert_eq!(&request[8..13], &[0, 9, 0, 2, 4]);
    assert_eq!(&request[13..17], &12.5_f32.to_be_bytes());
    server.await.unwrap();
}

#[tokio::test]
async fn modbus_tcp_adapter_applies_inverse_transform_and_word_order_on_write() {
    let (endpoint, request, server) = spawn_write_echo_server().await;
    let connection = ProtocolConnection::modbus_tcp("modbus-main", endpoint);
    let address =
        PointAddress::modbus_holding_register(40010).with_modbus_options(ModbusPointOptions {
            encoding: Some(ModbusRegisterEncoding::F32),
            word_order: ModbusWordOrder::LowWordFirst,
            scale: 0.5,
            offset: 10.0,
            ..Default::default()
        });
    let mapping = TelemetryPointMapping::new(
        "setpoint",
        "pump-1",
        "pump.setpoint",
        "modbus-main",
        address,
        TelemetryType::Float,
    )
    .with_access(PointAccess::ReadWrite);
    let mut adapter = ModbusTcpAdapter::new(connection, Vec::new());

    adapter
        .write_point(&mapping, TelemetryValue::Float(12.0))
        .await
        .unwrap();

    let request = request.await.unwrap();
    let raw = 4.0_f32.to_be_bytes();
    assert_eq!(&request[13..17], &[raw[2], raw[3], raw[0], raw[1]]);
    server.await.unwrap();
}

#[tokio::test]
async fn modbus_tcp_adapter_writes_single_coil_over_a_real_socket() {
    let (endpoint, request, server) = spawn_write_echo_server().await;
    let connection = ProtocolConnection::modbus_tcp("modbus-main", endpoint);
    let mapping = TelemetryPointMapping::new(
        "start",
        "pump-1",
        "pump.start",
        "modbus-main",
        PointAddress {
            kind: "coil".to_string(),
            value: "00001".to_string(),
            modbus: None,
        },
        TelemetryType::Boolean,
    )
    .with_access(PointAccess::ReadWrite);
    let mut adapter = ModbusTcpAdapter::new(connection, Vec::new());

    adapter
        .write_point(&mapping, TelemetryValue::Boolean(true))
        .await
        .unwrap();

    let request = request.await.unwrap();
    assert_eq!(&request[7..12], &[0x05, 0, 0, 0xFF, 0]);
    server.await.unwrap();
}

#[tokio::test]
async fn modbus_tcp_adapter_batches_contiguous_coils_and_registers() {
    let mut options = ModbusTcpSimulatorOptions::new("127.0.0.1:0".parse().unwrap());
    options.coils.insert(0, false);
    options.coils.insert(1, false);
    options.holding_registers.insert(9, 0);
    options.holding_registers.insert(10, 0);
    options.holding_registers.insert(11, 0);
    let simulator = ModbusTcpSimulator::bind(options).await.unwrap();
    let endpoint = simulator.local_addr().unwrap().to_string();
    let metrics = simulator.metrics();
    let server = tokio::spawn(simulator.run());
    let connection = ProtocolConnection::modbus_tcp("modbus-main", &endpoint);
    let coil_1 = tcp_mapping("coil_1", "coil", "00001", TelemetryType::Boolean)
        .with_access(PointAccess::ReadWrite);
    let coil_2 = tcp_mapping("coil_2", "coil", "00002", TelemetryType::Boolean)
        .with_access(PointAccess::ReadWrite);
    let register = tcp_mapping(
        "register",
        "holding_register",
        "40010",
        TelemetryType::Integer,
    )
    .with_access(PointAccess::ReadWrite);
    let float = tcp_mapping("float", "holding_register", "40011", TelemetryType::Float)
        .with_access(PointAccess::ReadWrite);
    let mut writer = ModbusTcpAdapter::new(connection.clone(), Vec::new());

    writer
        .write_points(&[
            ProtocolPointWrite::new(coil_1.clone(), TelemetryValue::Boolean(true)),
            ProtocolPointWrite::new(coil_2.clone(), TelemetryValue::Boolean(false)),
        ])
        .await
        .unwrap();
    writer
        .write_points(&[
            ProtocolPointWrite::new(register.clone(), TelemetryValue::Integer(321)),
            ProtocolPointWrite::new(float.clone(), TelemetryValue::Float(12.5)),
        ])
        .await
        .unwrap();

    assert_eq!(metrics.requests_total(), 2);
    let mut reader = ModbusTcpAdapter::new(connection, vec![coil_1, coil_2, register, float]);
    let samples = reader.read_telemetry().await.unwrap();
    assert_eq!(samples[0].value, TelemetryValue::Boolean(true));
    assert_eq!(samples[1].value, TelemetryValue::Boolean(false));
    assert_eq!(samples[2].value, TelemetryValue::Integer(321));
    assert_eq!(samples[3].value, TelemetryValue::Float(12.5));
    server.abort();
}

#[tokio::test]
async fn simulator_persists_writable_coils_and_registers_across_connections() {
    let mut options = ModbusTcpSimulatorOptions::new("127.0.0.1:0".parse().unwrap());
    options.coils.insert(0, false);
    options.holding_registers.insert(9, 0);
    let simulator = ModbusTcpSimulator::bind(options).await.unwrap();
    let endpoint = simulator.local_addr().unwrap().to_string();
    let server = tokio::spawn(simulator.run());
    let coil = TelemetryPointMapping::new(
        "start",
        "pump-1",
        "pump.start",
        "modbus-main",
        PointAddress {
            kind: "coil".to_string(),
            value: "00001".to_string(),
            modbus: None,
        },
        TelemetryType::Boolean,
    )
    .with_access(PointAccess::ReadWrite);
    let register = TelemetryPointMapping::new(
        "speed_setpoint",
        "pump-1",
        "pump.speed_setpoint",
        "modbus-main",
        PointAddress::modbus_holding_register(40010),
        TelemetryType::Integer,
    )
    .with_access(PointAccess::ReadWrite);
    let connection = ProtocolConnection::modbus_tcp("modbus-main", &endpoint);
    let mut writer = ModbusTcpAdapter::new(connection.clone(), Vec::new());

    writer
        .write_point(&coil, TelemetryValue::Boolean(true))
        .await
        .unwrap();
    writer
        .write_point(&register, TelemetryValue::Integer(1450))
        .await
        .unwrap();

    let mut reader = ModbusTcpAdapter::new(connection, vec![coil, register]);
    let samples = reader.read_telemetry().await.unwrap();
    assert_eq!(samples[0].value, TelemetryValue::Boolean(true));
    assert_eq!(samples[1].value, TelemetryValue::Integer(1450));
    server.abort();
}

#[tokio::test]
async fn configured_runtime_executes_modbus_tcp_package_and_records_connection_health() {
    let (endpoint, server) = spawn_simulator().await;
    let package = EdgeConfigPackage::new("edge-tcp", "v1")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::modbus_tcp("modbus-main", endpoint))
        .with_point_mapping(TelemetryPointMapping::new(
            "pressure",
            "pump-1",
            "pump.pressure",
            "modbus-main",
            PointAddress::modbus_holding_register(40001),
            TelemetryType::Float,
        ));
    let mut runtime =
        ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(Vec::new())).unwrap();

    let report = runtime.collect_once().await.unwrap();

    assert_eq!(report.samples_collected, 1);
    assert_eq!(
        runtime.shadow("pump-1").unwrap().telemetry()["pressure"].value,
        TelemetryValue::Float(2.5)
    );
    let metrics = runtime.protocol_runtime_metrics();
    assert_eq!(metrics.len(), 1);
    assert!(metrics[0].connected);
    assert_eq!(metrics[0].protocol, "Modbus TCP");
    assert_eq!(metrics[0].error_count, 0);
    server.abort();
}

#[tokio::test]
async fn modbus_tcp_adapter_surfaces_device_exception() {
    let mut options = ModbusTcpSimulatorOptions::new("127.0.0.1:0".parse().unwrap());
    options.unit_id = 2;
    let simulator = ModbusTcpSimulator::bind(options).await.unwrap();
    let endpoint = simulator.local_addr().unwrap().to_string();
    let server = tokio::spawn(simulator.run());
    let connection = ProtocolConnection::modbus_tcp("modbus-main", endpoint);
    let mappings = vec![TelemetryPointMapping::new(
        "pressure",
        "pump-1",
        "pump.pressure",
        "modbus-main",
        PointAddress::modbus_holding_register(40001),
        TelemetryType::Integer,
    )];
    let mut adapter = ModbusTcpAdapter::new(connection, mappings);

    let error = adapter.read_telemetry().await.unwrap_err();

    assert!(error.to_string().contains("exception code 3"));
    server.abort();
}

#[tokio::test]
async fn simulator_dynamic_float_changes_between_protocol_reads() {
    let mut options = ModbusTcpSimulatorOptions::new("127.0.0.1:0".parse().unwrap());
    options.dynamic_holding_floats.insert(
        0,
        edge_runtime::DynamicFloatPoint::new(2.4, 0.2, Duration::from_millis(80)),
    );
    let simulator = ModbusTcpSimulator::bind(options).await.unwrap();
    let endpoint = simulator.local_addr().unwrap().to_string();
    let server = tokio::spawn(simulator.run());
    let mapping = || {
        TelemetryPointMapping::new(
            "pressure",
            "pump-1",
            "pump.pressure",
            "modbus-main",
            PointAddress::modbus_holding_register(40001),
            TelemetryType::Float,
        )
    };
    let mut first = ModbusTcpAdapter::new(
        ProtocolConnection::modbus_tcp("modbus-main", &endpoint),
        vec![mapping()],
    );
    let first_value = first.read_telemetry().await.unwrap()[0].value.clone();
    tokio::time::sleep(Duration::from_millis(20)).await;
    let mut second = ModbusTcpAdapter::new(
        ProtocolConnection::modbus_tcp("modbus-main", endpoint),
        vec![mapping()],
    );
    let second_value = second.read_telemetry().await.unwrap()[0].value.clone();

    assert_ne!(first_value, second_value);
    server.abort();
}

#[tokio::test]
async fn modbus_tcp_runtime_publishes_qos1_payload_and_records_rocksdb_acknowledgement() {
    let (endpoint, modbus_server) = spawn_simulator().await;
    let (broker, observed) = spawn_qos1_mqtt_broker().await;
    let uplink = MqttUplinkConfig::velamq("lab-mqtt", broker, "edge-tcp-runtime").with_qos(1);
    let package = EdgeConfigPackage::new("edge-tcp", "lab-modbus-tcp-v1")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::modbus_tcp("modbus-main", endpoint))
        .with_mqtt_uplink(uplink.clone())
        .with_point_mapping(TelemetryPointMapping::new(
            "pressure",
            "pump-1",
            "pump.pressure",
            "modbus-main",
            PointAddress::modbus_holding_register(40001),
            TelemetryType::Float,
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "running",
            "pump-1",
            "pump.running",
            "modbus-main",
            PointAddress {
                kind: "coil".to_string(),
                value: "00001".to_string(),
                modbus: None,
            },
            TelemetryType::Boolean,
        ))
        .with_data_config(
            DataConfig::new(
                "pump-status",
                "Pump status",
                "pump-1",
                "modbus-main",
                DataConfigCollection::new(1000),
                DataConfigPublish::new(
                    "lab-mqtt",
                    "lab/{edge_id}/{device_id}/status",
                    DataConfigPayload::object(),
                )
                .with_qos(1),
            )
            .with_point(DataConfigPoint::new(
                "pressure",
                "pump.pressure",
                PointAddress::modbus_holding_register(40001),
                TelemetryType::Float,
                "pressure",
            ))
            .with_point(DataConfigPoint::new(
                "running",
                "pump.running",
                PointAddress {
                    kind: "coil".to_string(),
                    value: "00001".to_string(),
                    modbus: None,
                },
                TelemetryType::Boolean,
                "running",
            )),
        );
    let mut runtime =
        ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(Vec::new())).unwrap();
    let mut publisher =
        RumqttcMqttPublisher::connect_from_uplink_with_ack_timeout(&uplink, Duration::from_secs(2))
            .unwrap();
    let dir = tempdir().unwrap();
    let store = RocksEdgeRuntimeStore::open(dir.path().join("runtime.rocksdb")).unwrap();

    let report = runtime
        .collect_data_configs_once_and_publish_mqtt_with_outbox(&store, &mut publisher)
        .await
        .unwrap();
    let published = observed.await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&published.payload).unwrap();

    assert_eq!(report.collection.samples_collected, 2);
    assert_eq!(report.mqtt_messages_published, 1);
    assert_eq!(published.topic, "lab/edge-tcp/pump-1/status");
    assert_eq!(published.qos, 1);
    assert_eq!(payload["values"]["pressure"], 2.5);
    assert_eq!(payload["values"]["running"], true);
    assert_eq!(payload["quality"]["pressure"], "good");
    assert_eq!(store.mqtt_outbox_len().unwrap(), 0);
    let acknowledgements = store.mqtt_publish_acknowledgements(10).unwrap();
    assert_eq!(acknowledgements.len(), 1);
    assert_eq!(acknowledgements[0].sink_id, "lab-mqtt");
    assert_eq!(acknowledgements[0].topic, published.topic);
    assert_eq!(acknowledgements[0].qos, 1);
    assert!(acknowledgements[0].payload_bytes > 0);
    modbus_server.abort();
}

#[derive(Debug)]
struct ObservedPublish {
    topic: String,
    qos: u8,
    payload: Vec<u8>,
}

async fn spawn_qos1_mqtt_broker() -> (String, oneshot::Receiver<ObservedPublish>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let broker = format!("mqtt://{}", listener.local_addr().unwrap());
    let (observed_tx, observed_rx) = oneshot::channel();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let (connect_header, _) = read_mqtt_packet(&mut stream).await;
        assert_eq!(connect_header >> 4, 1);
        stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();

        let (publish_header, body) = read_mqtt_packet(&mut stream).await;
        assert_eq!(publish_header >> 4, 3);
        let qos = (publish_header >> 1) & 0x03;
        assert_eq!(qos, 1);
        let topic_len = usize::from(u16::from_be_bytes([body[0], body[1]]));
        let topic = String::from_utf8(body[2..2 + topic_len].to_vec()).unwrap();
        let packet_id_start = 2 + topic_len;
        let packet_id = u16::from_be_bytes([body[packet_id_start], body[packet_id_start + 1]]);
        let observed = ObservedPublish {
            topic,
            qos,
            payload: body[packet_id_start + 2..].to_vec(),
        };
        stream
            .write_all(&[0x40, 0x02, (packet_id >> 8) as u8, packet_id as u8])
            .await
            .unwrap();
        observed_tx.send(observed).ok();
        tokio::time::sleep(Duration::from_millis(100)).await;
    });

    (broker, observed_rx)
}

async fn read_mqtt_packet(stream: &mut TcpStream) -> (u8, Vec<u8>) {
    let header = stream.read_u8().await.unwrap();
    let mut multiplier = 1usize;
    let mut remaining_len = 0usize;
    loop {
        let encoded = stream.read_u8().await.unwrap();
        remaining_len += usize::from(encoded & 0x7f) * multiplier;
        if encoded & 0x80 == 0 {
            break;
        }
        multiplier *= 128;
    }
    let mut body = vec![0; remaining_len];
    stream.read_exact(&mut body).await.unwrap();
    (header, body)
}

fn tcp_mapping(
    point_id: &str,
    kind: &str,
    value: &str,
    value_type: TelemetryType,
) -> TelemetryPointMapping {
    TelemetryPointMapping::new(
        point_id,
        "device-1",
        point_id,
        "modbus-main",
        PointAddress {
            kind: kind.to_string(),
            value: value.to_string(),
            modbus: None,
        },
        value_type,
    )
}

async fn spawn_simulator() -> (String, tokio::task::JoinHandle<anyhow::Result<()>>) {
    let mut options = ModbusTcpSimulatorOptions::new(
        "127.0.0.1:0"
            .parse::<SocketAddr>()
            .expect("valid loopback address"),
    );
    let pressure = 2.5_f32.to_be_bytes();
    options.holding_registers = BTreeMap::from([
        (0, u16::from_be_bytes([pressure[0], pressure[1]])),
        (1, u16::from_be_bytes([pressure[2], pressure[3]])),
    ]);
    options.input_registers.insert(0, 36);
    options.coils.insert(0, true);
    let simulator = ModbusTcpSimulator::bind(options).await.unwrap();
    let endpoint = simulator.local_addr().unwrap().to_string();
    let server = tokio::spawn(simulator.run());
    (endpoint, server)
}

async fn spawn_write_echo_server() -> (
    String,
    oneshot::Receiver<Vec<u8>>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = listener.local_addr().unwrap().to_string();
    let (request_tx, request_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut header = [0_u8; 7];
        stream.read_exact(&mut header).await.unwrap();
        let body_len = u16::from_be_bytes([header[4], header[5]]) as usize - 1;
        let mut body = vec![0_u8; body_len];
        stream.read_exact(&mut body).await.unwrap();
        let mut request = header.to_vec();
        request.extend(&body);
        let response_pdu = body[..5].to_vec();
        let mut response = Vec::new();
        response.extend([header[0], header[1]]);
        response.extend(0_u16.to_be_bytes());
        response.extend(((response_pdu.len() + 1) as u16).to_be_bytes());
        response.push(header[6]);
        response.extend(response_pdu);
        stream.write_all(&response).await.unwrap();
        request_tx.send(request).unwrap();
    });
    (endpoint, request_rx, server)
}
