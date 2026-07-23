#![cfg(unix)]

use std::ffi::CStr;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::FromRawFd;
use std::time::Duration;

use edge_core::{
    DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint, DataConfigPublish,
    DeviceInstance, EdgeConfigPackage, MqttUplinkConfig, PointAddress, ProtocolConnection,
    SerialConnectionSettings, TelemetryPointMapping, TelemetryType,
};
use edge_runtime::{
    append_dlt645_checksum, append_iec101_checksum, append_modbus_rtu_crc, ConfiguredEdgeRuntime,
    RumqttcMqttPublisher, SerialBusFactory, TokioSerialBusFactory,
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
    const VOLTAGE_DI: u32 = 0x0001_0000;

    let (master, _slave_guard, slave_path) = open_raw_pty();
    let device = spawn_pty_device(
        master,
        vec![(
            dlt645_read_request(METER, VOLTAGE_DI),
            dlt645_read_response(METER, VOLTAGE_DI, &[0x50, 0x20, 0x02]),
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
            PointAddress::dlt645_scaled("123456789012", "00010000", 2),
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
