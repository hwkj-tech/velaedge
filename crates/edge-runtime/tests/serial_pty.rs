#![cfg(unix)]

use std::ffi::CStr;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::FromRawFd;
use std::time::Duration;

use edge_core::{
    CommandFlowConfig, CommandGraphEdge, CommandGraphNode, CommandGraphNodeKind,
    CustomSerialChecksum, CustomSerialFrameEncoding, CustomSerialPointSpec,
    CustomSerialValueEncoding, DataConfig, DataConfigCollection, DataConfigPayload,
    DataConfigPoint, DataConfigPublish, DeviceInstance, EdgeConfigPackage, Iec101ControlType,
    Iec101PointOptions, MqttUplinkConfig, PointAccess, PointAddress, ProtocolConnection,
    ProtocolType, SerialConnectionSettings, TelemetryPointMapping, TelemetryType,
};
use edge_runtime::{
    append_custom_serial_checksum, append_dlt645_checksum, append_iec101_checksum,
    append_modbus_rtu_crc, encode_custom_serial_frame, CommandExecutionStatus,
    ConfiguredEdgeRuntime, RumqttcMqttPublisher, SerialBusFactory, TokioSerialBusFactory,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};

#[tokio::test]
async fn production_serial_factory_round_trips_modbus_over_a_pty() {
    let (master, _slave_guard, slave_path) = open_raw_pty();
    let mut request = vec![1, 0x03, 0, 0, 0, 1];
    append_modbus_rtu_crc(&mut request);
    let mut response = vec![1, 0x03, 2, 0, 231];
    append_modbus_rtu_crc(&mut response);
    let expected_request = request.clone();

    let device = std::thread::spawn(move || {
        let mut master = master;
        let mut observed = vec![0_u8; expected_request.len()];
        master
            .read_exact(&mut observed)
            .expect("PTY device should receive the complete request");
        assert_eq!(observed, expected_request);
        master
            .write_all(&response)
            .expect("PTY device should write the response");
        master.flush().expect("PTY device response should flush");
        std::thread::sleep(std::time::Duration::from_millis(150));
        observed
    });

    let connection = ProtocolConnection::modbus_rtu_serial(
        "pty-modbus",
        // macOS rejects IOSSIOSPEED on pseudo terminals; baud 0 skips only that
        // hardware-specific ioctl while retaining the production serial I/O path.
        SerialConnectionSettings::new(slave_path, 0),
    );
    let mut factory = TokioSerialBusFactory;
    let mut bus = factory
        .open(&connection)
        .expect("production serial factory should open the PTY slave");
    let observed_response = bus
        .transact(&request)
        .await
        .expect("production serial transport should complete a frame round trip");

    let mut expected_response = vec![1, 0x03, 2, 0, 231];
    append_modbus_rtu_crc(&mut expected_response);
    assert_eq!(observed_response, expected_response);
    device.join().expect("PTY device thread should finish");
}

#[tokio::test]
async fn modbus_pty_runtime_publishes_qos1_data_config_payload() {
    let (master, _slave_guard, slave_path) = open_raw_pty();
    let mut request = vec![1, 0x03, 0, 0, 0, 1];
    append_modbus_rtu_crc(&mut request);
    let mut response = vec![1, 0x03, 2, 0, 231];
    append_modbus_rtu_crc(&mut response);
    let device = spawn_pty_device(master, vec![(request, response)]);
    let (broker, observed) = spawn_qos1_mqtt_broker().await;
    let connection = ProtocolConnection::modbus_rtu_serial(
        "modbus-main",
        SerialConnectionSettings::new(slave_path, 0),
    );

    let payload = run_serial_data_config(
        "edge-lab-modbus",
        "meter-1",
        connection,
        DataConfigPoint::new(
            "voltage",
            "meter.voltage",
            PointAddress::modbus_holding_register(40001),
            TelemetryType::Integer,
            "voltage",
        ),
        broker,
        observed,
        "lab/{edge_id}/{device_id}/modbus",
    )
    .await;

    assert_eq!(payload.topic, "lab/edge-lab-modbus/meter-1/modbus");
    assert_eq!(payload.qos, 1);
    assert_eq!(payload.json["values"]["voltage"], 231);
    assert_eq!(payload.json["quality"]["voltage"], "good");
    device.join().expect("Modbus PTY device should finish");
}

#[tokio::test]
async fn dlt645_pty_runtime_publishes_qos1_data_config_payload() {
    const METER: [u8; 6] = [0x12, 0x90, 0x78, 0x56, 0x34, 0x12];
    const VOLTAGE_DI: u32 = 0x0201_0100;

    let (master, _slave_guard, slave_path) = open_raw_pty();
    let device = spawn_pty_device(
        master,
        vec![(
            dlt645_read_request(METER, VOLTAGE_DI),
            dlt645_read_response(METER, VOLTAGE_DI, &[0x05, 0x22]),
        )],
    );
    let (broker, observed) = spawn_qos1_mqtt_broker().await;
    let connection = ProtocolConnection::dlt645_serial(
        "dlt645-main",
        SerialConnectionSettings::new(slave_path, 0),
    );

    let payload = run_serial_data_config(
        "edge-lab-dlt645",
        "meter-1",
        connection,
        DataConfigPoint::new(
            "voltage",
            "meter.voltage",
            PointAddress::dlt645_scaled("123456789012", "02010100", 1),
            TelemetryType::Float,
            "voltage",
        ),
        broker,
        observed,
        "lab/{edge_id}/{device_id}/dlt645",
    )
    .await;

    assert_eq!(payload.topic, "lab/edge-lab-dlt645/meter-1/dlt645");
    assert_eq!(payload.qos, 1);
    assert_eq!(payload.json["values"]["voltage"], 220.5);
    assert_eq!(payload.json["quality"]["voltage"], "good");
    device.join().expect("DL/T 645 PTY device should finish");
}

#[tokio::test]
async fn custom_serial_v2_pty_runtime_decodes_slip_and_publishes_qos1_payload() {
    let (master, _slave_guard, slave_path) = open_raw_pty();
    let mut spec = CustomSerialPointSpec::new("10 02", 1, CustomSerialValueEncoding::U16Be);
    spec.schema_version = 2;
    spec.frame_encoding = CustomSerialFrameEncoding::Slip;
    spec.request_checksum = CustomSerialChecksum::Crc16CcittFalse;
    spec.response_checksum = CustomSerialChecksum::Crc16CcittFalse;
    spec.response_prefix_hex = Some("AA".to_string());
    spec.scale = 0.1;

    let mut request = vec![0x10, 0x02];
    append_custom_serial_checksum(&mut request, spec.request_checksum);
    let request = encode_custom_serial_frame(&request, spec.frame_encoding).unwrap();
    let mut response = vec![0xAA, 0x01, 0x2C];
    append_custom_serial_checksum(&mut response, spec.response_checksum);
    let response = encode_custom_serial_frame(&response, spec.frame_encoding).unwrap();
    let device = spawn_pty_device(master, vec![(request, response)]);
    let (broker, observed) = spawn_qos1_mqtt_broker().await;
    let connection = ProtocolConnection {
        connection_id: "custom-serial-main".to_string(),
        protocol: ProtocolType::CustomSerial,
        endpoint: Some(slave_path.clone()),
        serial: Some(SerialConnectionSettings::new(slave_path, 0)),
        iec101: None,
        iec104: None,
        opc_ua: None,
        bacnet_ip: None,
        siemens_s7: None,
        omron_fins: None,
        circuit_breaker: Default::default(),
    };

    let payload = run_serial_data_config(
        "edge-lab-custom",
        "sensor-1",
        connection,
        DataConfigPoint::new(
            "temperature",
            "sensor.temperature",
            PointAddress::custom_serial(&spec).unwrap(),
            TelemetryType::Float,
            "temperature",
        ),
        broker,
        observed,
        "lab/{edge_id}/{device_id}/custom-serial",
    )
    .await;

    assert_eq!(payload.topic, "lab/edge-lab-custom/sensor-1/custom-serial");
    assert_eq!(payload.qos, 1);
    assert_eq!(payload.json["values"]["temperature"], 30.0);
    assert_eq!(payload.json["quality"]["temperature"], "good");
    device
        .join()
        .expect("custom serial PTY device should finish");
}

#[tokio::test]
async fn iec101_pty_runtime_publishes_qos1_data_config_payload() {
    let (master, _slave_guard, slave_path) = open_raw_pty();
    let reset = vec![0x10, 0x40, 0x01, 0x41, 0x16];
    let read = iec101_read_request(1, 2, 1001);
    let response = iec101_monitoring_response(1, 2, 1001, 1, &[0x01]);
    let device = spawn_pty_device(master, vec![(reset, vec![0xE5]), (read, response)]);
    let (broker, observed) = spawn_qos1_mqtt_broker().await;
    let connection = ProtocolConnection::iec101_serial(
        "iec101-main",
        SerialConnectionSettings::new(slave_path, 0).with_parity("even"),
    );

    let payload = run_serial_data_config(
        "edge-lab-iec101",
        "bay-1",
        connection,
        DataConfigPoint::new(
            "breaker_closed",
            "breaker.closed",
            PointAddress::iec101(1, 2, 1001),
            TelemetryType::Boolean,
            "breaker_closed",
        ),
        broker,
        observed,
        "lab/{edge_id}/{device_id}/iec101",
    )
    .await;

    assert_eq!(payload.topic, "lab/edge-lab-iec101/bay-1/iec101");
    assert_eq!(payload.qos, 1);
    assert_eq!(payload.json["values"]["breaker_closed"], true);
    assert_eq!(payload.json["quality"]["breaker_closed"], "good");
    device.join().expect("IEC 101 PTY device should finish");
}

#[tokio::test]
async fn iec101_pty_runtime_executes_sbo_command_flow_over_production_serial_io() {
    let (master, _slave_guard, slave_path) = open_raw_pty();
    let reset = vec![0x10, 0x40, 0x01, 0x41, 0x16];
    let select = iec101_control_request(0x53, 1, 7, 1201, 45, &[0x81]);
    let select_confirmation = iec101_command_confirmation(1, 7, 1201, 45, &[0x81]);
    let execute = iec101_control_request(0x73, 1, 7, 1201, 45, &[0x01]);
    let execute_confirmation = iec101_command_confirmation(1, 7, 1201, 45, &[0x01]);
    let device = spawn_pty_device(
        master,
        vec![
            (reset, vec![0xE5]),
            (select, select_confirmation),
            (execute, execute_confirmation),
        ],
    );
    let connection = ProtocolConnection::iec101_serial(
        "iec101-main",
        SerialConnectionSettings::new(slave_path, 0).with_parity("even"),
    );
    let mapping = TelemetryPointMapping::new(
        "breaker_close",
        "bay-1",
        "breaker.close",
        "iec101-main",
        PointAddress::iec101(1, 7, 1201),
        TelemetryType::Boolean,
    )
    .with_access(PointAccess::ReadWrite)
    .with_iec101_options(
        Iec101PointOptions::new(Iec101ControlType::SingleCommand).with_select_before_operate(true),
    );
    let package = EdgeConfigPackage::new("edge-lab-iec101", "lab-iec101-control-v1")
        .with_device(DeviceInstance::new("bay-1", "substation-bay"))
        .with_protocol_connection(connection)
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "lab-mqtt",
            "mqtt://127.0.0.1:1883",
            "iec101-command-runtime",
        ))
        .with_point_mapping(mapping)
        .with_command_flow(iec101_command_flow());
    let mut runtime = ConfiguredEdgeRuntime::new(package, TokioSerialBusFactory)
        .expect("production runtime should accept IEC 101 writable point config");

    let report = runtime
        .execute_command_flow_message(
            "breaker-control",
            br#"{"commandId":"cmd-iec101-pty","value":true}"#,
        )
        .await
        .expect("IEC 101 PTY command flow should succeed");

    assert_eq!(report.status, CommandExecutionStatus::Succeeded);
    assert_eq!(report.writes.len(), 1);
    assert!(report.writes[0].verified);
    assert_eq!(
        report.replies[0].topic,
        "lab/edge-lab-iec101/reply/cmd-iec101-pty"
    );
    device.join().expect("IEC 101 PTY device should finish");
}

struct PublishedPayload {
    topic: String,
    qos: u8,
    json: serde_json::Value,
}

async fn run_serial_data_config(
    edge_id: &str,
    device_id: &str,
    connection: ProtocolConnection,
    point: DataConfigPoint,
    broker: String,
    observed: oneshot::Receiver<ObservedPublish>,
    topic_template: &str,
) -> PublishedPayload {
    let connection_id = connection.connection_id.clone();
    let point_mapping = TelemetryPointMapping::new(
        point.point_id.clone(),
        device_id,
        point.semantic_id.clone(),
        connection_id.clone(),
        point.address.clone(),
        point.value_type,
    );
    let uplink =
        MqttUplinkConfig::velamq("lab-mqtt", broker, format!("{edge_id}-runtime")).with_qos(1);
    let package = EdgeConfigPackage::new(edge_id, "lab-serial-v1")
        .with_device(DeviceInstance::new(device_id, "lab-device"))
        .with_protocol_connection(connection)
        .with_mqtt_uplink(uplink.clone())
        .with_point_mapping(point_mapping)
        .with_data_config(
            DataConfig::new(
                "lab-pipeline",
                "Lab serial acceptance",
                device_id,
                connection_id,
                DataConfigCollection::new(1000),
                DataConfigPublish::new("lab-mqtt", topic_template, DataConfigPayload::object())
                    .with_qos(1),
            )
            .with_point(point),
        );
    let mut runtime = ConfiguredEdgeRuntime::new(package, TokioSerialBusFactory)
        .expect("production runtime should accept the lab package");
    let mut publisher =
        RumqttcMqttPublisher::connect_from_uplink_with_ack_timeout(&uplink, Duration::from_secs(2))
            .expect("MQTT publisher should connect to the lab broker");

    let report = runtime
        .collect_data_configs_once_and_publish_mqtt(&mut publisher)
        .await
        .expect("serial collection and acknowledged MQTT publish should succeed");
    assert_eq!(report.collection.samples_collected, 1);
    assert_eq!(report.mqtt_messages_published, 1);

    let broker_payload = observed
        .await
        .expect("lab broker should capture the published payload");
    PublishedPayload {
        topic: broker_payload.topic,
        qos: broker_payload.qos,
        json: serde_json::from_slice(&broker_payload.payload)
            .expect("published MQTT payload should be JSON"),
    }
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

fn spawn_pty_device(
    master: File,
    exchanges: Vec<(Vec<u8>, Vec<u8>)>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut master = master;
        for (expected_request, response) in exchanges {
            let mut observed = vec![0_u8; expected_request.len()];
            master
                .read_exact(&mut observed)
                .expect("PTY device should receive a complete protocol request");
            assert_eq!(observed, expected_request);
            master
                .write_all(&response)
                .expect("PTY device should write the protocol response");
            master.flush().expect("PTY response should flush");
        }
        std::thread::sleep(Duration::from_millis(150));
    })
}

fn dlt645_read_request(meter: [u8; 6], data_identifier: u32) -> Vec<u8> {
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

fn dlt645_read_response(meter: [u8; 6], data_identifier: u32, value: &[u8]) -> Vec<u8> {
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

fn iec101_read_request(link_address: u8, common_address: u16, ioa: u32) -> Vec<u8> {
    let mut body = vec![0x53, link_address, 102, 1, 5, 0];
    body.extend(common_address.to_le_bytes());
    body.extend([ioa as u8, (ioa >> 8) as u8, (ioa >> 16) as u8]);
    iec101_variable_frame(body)
}

fn iec101_monitoring_response(
    link_address: u8,
    common_address: u16,
    ioa: u32,
    type_id: u8,
    information: &[u8],
) -> Vec<u8> {
    let mut body = vec![0x08, link_address, type_id, 1, 5, 0];
    body.extend(common_address.to_le_bytes());
    body.extend([ioa as u8, (ioa >> 8) as u8, (ioa >> 16) as u8]);
    body.extend_from_slice(information);
    iec101_variable_frame(body)
}

fn iec101_control_request(
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
    iec101_variable_frame(body)
}

fn iec101_command_confirmation(
    link_address: u8,
    common_address: u16,
    ioa: u32,
    type_id: u8,
    information: &[u8],
) -> Vec<u8> {
    let mut body = vec![0x08, link_address, type_id, 1, 7, 0];
    body.extend(common_address.to_le_bytes());
    body.extend([ioa as u8, (ioa >> 8) as u8, (ioa >> 16) as u8]);
    body.extend_from_slice(information);
    iec101_variable_frame(body)
}

fn iec101_command_flow() -> CommandFlowConfig {
    CommandFlowConfig::new(
        "breaker-control",
        "断路器控制",
        "lab-mqtt",
        "lab/edge-lab-iec101/command",
        "lab/{edge_id}/reply/{command_id}",
    )
    .with_node(CommandGraphNode::new(
        "input",
        CommandGraphNodeKind::MqttInput,
        "MQTT 输入",
    ))
    .with_node(
        CommandGraphNode::new("write", CommandGraphNodeKind::PointWrite, "写断路器")
            .with_ref("breaker_close"),
    )
    .with_node(CommandGraphNode::new(
        "reply",
        CommandGraphNodeKind::MqttReply,
        "MQTT 回执",
    ))
    .with_edge(CommandGraphEdge::new("input-write", "input", "write"))
    .with_edge(CommandGraphEdge::new("write-reply", "write", "reply"))
}

fn iec101_variable_frame(body: Vec<u8>) -> Vec<u8> {
    let length = body.len() as u8;
    let mut frame = vec![0x68, length, length, 0x68];
    frame.extend(body);
    append_iec101_checksum(&mut frame, 4);
    frame.push(0x16);
    frame
}

fn open_raw_pty() -> (File, File, String) {
    let mut master_fd = -1;
    let mut slave_fd = -1;
    let result = unsafe {
        libc::openpty(
            &mut master_fd,
            &mut slave_fd,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(result, 0, "openpty should create a pseudo terminal");

    let mut settings = unsafe { std::mem::zeroed::<libc::termios>() };
    assert_eq!(unsafe { libc::tcgetattr(slave_fd, &mut settings) }, 0);
    unsafe { libc::cfmakeraw(&mut settings) };
    settings.c_cc[libc::VMIN] = 1;
    settings.c_cc[libc::VTIME] = 0;
    assert_eq!(
        unsafe { libc::tcsetattr(slave_fd, libc::TCSANOW, &settings) },
        0
    );

    let mut path = [0 as libc::c_char; 1024];
    assert_eq!(
        unsafe { libc::ttyname_r(slave_fd, path.as_mut_ptr(), path.len()) },
        0,
        "PTY slave path should be available"
    );
    let path = unsafe { CStr::from_ptr(path.as_ptr()) }
        .to_str()
        .expect("PTY slave path should be UTF-8")
        .to_string();
    let master = unsafe { File::from_raw_fd(master_fd) };
    let slave = unsafe { File::from_raw_fd(slave_fd) };
    (master, slave, path)
}
