use std::{collections::BTreeMap, net::SocketAddr, time::Duration};

use edge_core::{
    DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint, DataConfigPublish,
    DataQuality, DeviceInstance, EdgeConfigPackage, MqttUplinkConfig, PointAddress,
    ProtocolConnection, TelemetryPointMapping, TelemetryType, TelemetryValue,
};
use edge_runtime::{
    ConfiguredEdgeRuntime, ModbusTcpAdapter, ModbusTcpSimulator, ModbusTcpSimulatorOptions,
    ProtocolAdapter, RocksEdgeRuntimeStore, RumqttcMqttPublisher, ScriptedSerialBusFactory,
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
