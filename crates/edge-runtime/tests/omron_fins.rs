use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use edge_core::{
    OmronFinsConnectionSettings, OmronFinsTransport, PointAccess, PointAddress, ProtocolConnection,
    TelemetryPointMapping, TelemetryType, TelemetryValue,
};
use edge_runtime::{OmronFinsAdapter, ProtocolAdapter, ProtocolCommandAdapter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

#[derive(Default)]
struct PlcMemory {
    words: HashMap<(u8, u16), u16>,
    bits: HashMap<(u8, u16, u8), bool>,
    read_requests: Vec<(u8, u16, u16)>,
}

fn mapping(
    point_id: &str,
    address: &str,
    value_type: TelemetryType,
    access: PointAccess,
) -> TelemetryPointMapping {
    TelemetryPointMapping::new(
        point_id,
        "plc-1",
        point_id,
        "fins-main",
        PointAddress::omron_fins(address),
        value_type,
    )
    .with_access(access)
}

fn value<'a>(samples: &'a [edge_core::TelemetrySample], point_id: &str) -> &'a TelemetryValue {
    &samples
        .iter()
        .find(|sample| sample.telemetry_id == point_id)
        .unwrap_or_else(|| panic!("missing sample {point_id}"))
        .value
}

#[tokio::test]
async fn persistent_fins_udp_session_reads_and_writes_real_frames() {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let address = socket.local_addr().unwrap();
    let float_bits = 25.5_f32.to_bits();
    let memory = Arc::new(Mutex::new(PlcMemory {
        words: HashMap::from([
            ((0x82, 100), 0x1234),
            ((0x82, 110), float_bits as u16),
            ((0x82, 111), (float_bits >> 16) as u16),
        ]),
        bits: HashMap::from([((0x30, 0, 5), true)]),
        read_requests: Vec::new(),
    }));
    let server_memory = Arc::clone(&memory);
    let server_task = tokio::spawn(async move {
        let mut buffer = [0_u8; 2_048];
        loop {
            let (length, peer) = socket.recv_from(&mut buffer).await.unwrap();
            let request = &buffer[..length];
            assert!(request.len() >= 18, "short FINS request");
            let response = handle_request(request, &server_memory);
            socket.send_to(&response, peer).await.unwrap();
        }
    });

    let mappings = vec![
        mapping(
            "counter",
            "D100",
            TelemetryType::Integer,
            PointAccess::ReadWrite,
        ),
        mapping(
            "temperature",
            "DM110",
            TelemetryType::Float,
            PointAccess::ReadWrite,
        ),
        mapping(
            "running",
            "CIO0.5",
            TelemetryType::Boolean,
            PointAccess::ReadWrite,
        ),
    ];
    let connection = ProtocolConnection::omron_fins(
        "fins-main",
        format!("fins://{address}"),
        OmronFinsConnectionSettings {
            source_node: 1,
            destination_node: 10,
            ..Default::default()
        },
    );
    let mut adapter = OmronFinsAdapter::new(connection, mappings.clone()).unwrap();

    let samples = adapter.read_telemetry().await.unwrap();
    assert_eq!(value(&samples, "counter"), &TelemetryValue::Integer(0x1234));
    assert_eq!(value(&samples, "temperature"), &TelemetryValue::Float(25.5));
    assert_eq!(value(&samples, "running"), &TelemetryValue::Boolean(true));
    assert_eq!(adapter.connection_generation(), 1);

    adapter
        .write_point(&mappings[0], TelemetryValue::Integer(4_321))
        .await
        .unwrap();
    adapter
        .write_point(&mappings[1], TelemetryValue::Float(12.75))
        .await
        .unwrap();
    adapter
        .write_point(&mappings[2], TelemetryValue::Boolean(false))
        .await
        .unwrap();

    let samples = adapter.read_telemetry().await.unwrap();
    assert_eq!(value(&samples, "counter"), &TelemetryValue::Integer(4_321));
    assert_eq!(
        value(&samples, "temperature"),
        &TelemetryValue::Float(12.75)
    );
    assert_eq!(value(&samples, "running"), &TelemetryValue::Boolean(false));
    assert_eq!(adapter.connection_generation(), 1);

    server_task.abort();
}

#[tokio::test]
async fn adjacent_points_share_bounded_fins_word_reads() {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let address = socket.local_addr().unwrap();
    let float_bits = 42.25_f32.to_bits();
    let memory = Arc::new(Mutex::new(PlcMemory {
        words: HashMap::from([
            ((0x82, 100), 10),
            ((0x82, 101), 20),
            ((0x82, 102), float_bits as u16),
            ((0x82, 103), (float_bits >> 16) as u16),
        ]),
        bits: HashMap::from([((0x30, 0, 1), true), ((0x30, 0, 5), true)]),
        read_requests: Vec::new(),
    }));
    let server_memory = Arc::clone(&memory);
    let server_task = tokio::spawn(async move {
        let mut buffer = [0_u8; 2_048];
        loop {
            let (length, peer) = socket.recv_from(&mut buffer).await.unwrap();
            let response = handle_request(&buffer[..length], &server_memory);
            socket.send_to(&response, peer).await.unwrap();
        }
    });

    let mappings = vec![
        mapping(
            "temperature",
            "D102",
            TelemetryType::Float,
            PointAccess::ReadOnly,
        ),
        mapping(
            "ready",
            "CIO0.1",
            TelemetryType::Boolean,
            PointAccess::ReadOnly,
        ),
        mapping(
            "counter",
            "D100",
            TelemetryType::Integer,
            PointAccess::ReadOnly,
        ),
        mapping(
            "mode",
            "D101",
            TelemetryType::Integer,
            PointAccess::ReadOnly,
        ),
        mapping(
            "running",
            "CIO0.5",
            TelemetryType::Boolean,
            PointAccess::ReadOnly,
        ),
    ];
    let connection = ProtocolConnection::omron_fins(
        "fins-main",
        format!("fins://{address}"),
        OmronFinsConnectionSettings {
            source_node: 1,
            destination_node: 10,
            ..Default::default()
        },
    );
    let mut adapter = OmronFinsAdapter::new(connection, mappings).unwrap();

    let samples = adapter.read_telemetry().await.unwrap();
    assert_eq!(
        samples
            .iter()
            .map(|sample| sample.telemetry_id.as_str())
            .collect::<Vec<_>>(),
        vec!["temperature", "ready", "counter", "mode", "running"]
    );
    assert_eq!(
        value(&samples, "temperature"),
        &TelemetryValue::Float(42.25)
    );
    assert_eq!(value(&samples, "ready"), &TelemetryValue::Boolean(true));
    assert_eq!(value(&samples, "counter"), &TelemetryValue::Integer(10));
    assert_eq!(value(&samples, "mode"), &TelemetryValue::Integer(20));
    assert_eq!(value(&samples, "running"), &TelemetryValue::Boolean(true));

    let read_requests = memory.lock().unwrap().read_requests.clone();
    assert_eq!(read_requests.len(), 2);
    assert!(read_requests.contains(&(0xB0, 0, 1)));
    assert!(read_requests.contains(&(0x82, 100, 4)));

    server_task.abort();
}

#[tokio::test]
async fn persistent_fins_tcp_session_negotiates_nodes_and_reuses_the_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let float_bits = 25.5_f32.to_bits();
    let memory = Arc::new(Mutex::new(PlcMemory {
        words: HashMap::from([
            ((0x82, 100), 0x1234),
            ((0x82, 110), float_bits as u16),
            ((0x82, 111), (float_bits >> 16) as u16),
        ]),
        bits: HashMap::from([((0x30, 0, 5), true)]),
        read_requests: Vec::new(),
    }));
    let server_memory = Arc::clone(&memory);
    let server_task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let (command, error, requested_node) = read_tcp_frame(&mut stream).await;
        assert_eq!(command, 0);
        assert_eq!(error, 0);
        assert_eq!(requested_node, 0_u32.to_be_bytes());
        let mut assigned_nodes = Vec::new();
        assigned_nodes.extend_from_slice(&11_u32.to_be_bytes());
        assigned_nodes.extend_from_slice(&42_u32.to_be_bytes());
        write_tcp_frame(&mut stream, 1, 0, &assigned_nodes).await;

        for _ in 0..9 {
            let (command, error, request) = read_tcp_frame(&mut stream).await;
            assert_eq!(command, 2);
            assert_eq!(error, 0);
            assert_eq!(request[4], 42, "server-assigned destination node is used");
            assert_eq!(request[7], 11, "server-assigned source node is used");
            let response = handle_request(&request, &server_memory);
            write_tcp_frame(&mut stream, 3, 0, &response).await;
        }
    });

    let mappings = vec![
        mapping(
            "counter",
            "D100",
            TelemetryType::Integer,
            PointAccess::ReadWrite,
        ),
        mapping(
            "temperature",
            "DM110",
            TelemetryType::Float,
            PointAccess::ReadWrite,
        ),
        mapping(
            "running",
            "CIO0.5",
            TelemetryType::Boolean,
            PointAccess::ReadWrite,
        ),
    ];
    let connection = ProtocolConnection::omron_fins(
        "fins-main",
        format!("fins://{address}"),
        OmronFinsConnectionSettings {
            transport: OmronFinsTransport::Tcp,
            source_node: 0,
            destination_node: 0,
            ..Default::default()
        },
    );
    let mut adapter = OmronFinsAdapter::new(connection, mappings.clone()).unwrap();

    let samples = adapter.read_telemetry().await.unwrap();
    assert_eq!(value(&samples, "counter"), &TelemetryValue::Integer(0x1234));
    assert_eq!(value(&samples, "temperature"), &TelemetryValue::Float(25.5));
    assert_eq!(value(&samples, "running"), &TelemetryValue::Boolean(true));
    adapter
        .write_point(&mappings[0], TelemetryValue::Integer(4_321))
        .await
        .unwrap();
    adapter
        .write_point(&mappings[1], TelemetryValue::Float(12.75))
        .await
        .unwrap();
    adapter
        .write_point(&mappings[2], TelemetryValue::Boolean(false))
        .await
        .unwrap();
    let samples = adapter.read_telemetry().await.unwrap();
    assert_eq!(value(&samples, "counter"), &TelemetryValue::Integer(4_321));
    assert_eq!(
        value(&samples, "temperature"),
        &TelemetryValue::Float(12.75)
    );
    assert_eq!(value(&samples, "running"), &TelemetryValue::Boolean(false));
    assert_eq!(adapter.connection_generation(), 1);

    server_task.await.unwrap();
}

#[tokio::test]
async fn fins_tcp_reconnects_and_repeats_node_handshake_after_transport_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let memory = Arc::new(Mutex::new(PlcMemory {
        words: HashMap::from([((0x82, 100), 7)]),
        ..Default::default()
    }));
    let server_memory = Arc::clone(&memory);
    let server_task = tokio::spawn(async move {
        for generation in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let (command, error, requested_node) = read_tcp_frame(&mut stream).await;
            assert_eq!((command, error), (0, 0));
            assert_eq!(requested_node, 0_u32.to_be_bytes());
            let mut assigned_nodes = Vec::new();
            assigned_nodes.extend_from_slice(&21_u32.to_be_bytes());
            assigned_nodes.extend_from_slice(&31_u32.to_be_bytes());
            write_tcp_frame(&mut stream, 1, 0, &assigned_nodes).await;

            let (command, error, request) = read_tcp_frame(&mut stream).await;
            assert_eq!((command, error), (2, 0));
            if generation == 0 {
                continue;
            }
            let response = handle_request(&request, &server_memory);
            write_tcp_frame(&mut stream, 3, 0, &response).await;
        }
    });

    let connection = ProtocolConnection::omron_fins(
        "fins-main",
        format!("fins://{address}"),
        OmronFinsConnectionSettings {
            transport: OmronFinsTransport::Tcp,
            source_node: 0,
            destination_node: 0,
            timeout_ms: 500,
            ..Default::default()
        },
    );
    let mappings = vec![mapping(
        "counter",
        "D100",
        TelemetryType::Integer,
        PointAccess::ReadOnly,
    )];
    let mut adapter = OmronFinsAdapter::new(connection, mappings).unwrap();

    assert!(adapter.read_telemetry().await.is_err());
    let samples = adapter.read_telemetry().await.unwrap();
    assert_eq!(value(&samples, "counter"), &TelemetryValue::Integer(7));
    assert_eq!(adapter.connection_generation(), 2);
    server_task.await.unwrap();
}

#[test]
fn fins_adapter_rejects_mappings_from_another_connection() {
    let mut foreign = mapping(
        "counter",
        "D100",
        TelemetryType::Integer,
        PointAccess::ReadOnly,
    );
    foreign.protocol_connection_id = "other".to_string();
    let connection = ProtocolConnection::omron_fins(
        "fins-main",
        "127.0.0.1:9600",
        OmronFinsConnectionSettings::default(),
    );
    assert!(OmronFinsAdapter::new(connection, vec![foreign]).is_err());
}

fn handle_request(request: &[u8], memory: &Arc<Mutex<PlcMemory>>) -> Vec<u8> {
    let mut response = vec![
        0xC0,
        request[1],
        request[2],
        request[6],
        request[7],
        request[8],
        request[3],
        request[4],
        request[5],
        request[9],
        request[10],
        request[11],
        0x00,
        0x00,
    ];
    let command = (request[10], request[11]);
    let area = request[12];
    let word = u16::from_be_bytes([request[13], request[14]]);
    let bit = request[15];
    let count = u16::from_be_bytes([request[16], request[17]]) as usize;
    let mut memory = memory.lock().unwrap();
    if command == (0x01, 0x01) {
        memory.read_requests.push((area, word, count as u16));
    }
    match command {
        (0x01, 0x01) if matches!(area, 0x30..=0x33) => {
            response.push(u8::from(
                memory
                    .bits
                    .get(&(area, word, bit))
                    .copied()
                    .unwrap_or(false),
            ));
        }
        (0x01, 0x01) => {
            for offset in 0..count {
                let current_word = word + offset as u16;
                let mut value = memory
                    .words
                    .get(&(area, current_word))
                    .copied()
                    .unwrap_or_default();
                if let Some(bit_area) = bit_area_for_word_area(area) {
                    for bit in 0..16 {
                        match memory.bits.get(&(bit_area, current_word, bit)) {
                            Some(true) => value |= 1_u16 << bit,
                            Some(false) => value &= !(1_u16 << bit),
                            None => {}
                        }
                    }
                }
                response.extend_from_slice(&value.to_be_bytes());
            }
        }
        (0x01, 0x02) if matches!(area, 0x30..=0x33) => {
            memory.bits.insert((area, word, bit), request[18] != 0);
        }
        (0x01, 0x02) => {
            for offset in 0..count {
                let start = 18 + offset * 2;
                let value = u16::from_be_bytes([request[start], request[start + 1]]);
                memory.words.insert((area, word + offset as u16), value);
            }
        }
        _ => panic!("unsupported FINS command {command:?}"),
    }
    response
}

fn bit_area_for_word_area(area: u8) -> Option<u8> {
    match area {
        0xB0 => Some(0x30),
        0xB1 => Some(0x31),
        0xB2 => Some(0x32),
        0xB3 => Some(0x33),
        _ => None,
    }
}

async fn read_tcp_frame(stream: &mut TcpStream) -> (u32, u32, Vec<u8>) {
    let mut prefix = [0_u8; 8];
    stream.read_exact(&mut prefix).await.unwrap();
    assert_eq!(&prefix[..4], b"FINS");
    let body_length = u32::from_be_bytes(prefix[4..].try_into().unwrap()) as usize;
    let mut body = vec![0_u8; body_length];
    stream.read_exact(&mut body).await.unwrap();
    (
        u32::from_be_bytes(body[..4].try_into().unwrap()),
        u32::from_be_bytes(body[4..8].try_into().unwrap()),
        body[8..].to_vec(),
    )
}

async fn write_tcp_frame(stream: &mut TcpStream, command: u32, error: u32, payload: &[u8]) {
    let mut frame = Vec::new();
    frame.extend_from_slice(b"FINS");
    frame.extend_from_slice(&u32::try_from(payload.len() + 8).unwrap().to_be_bytes());
    frame.extend_from_slice(&command.to_be_bytes());
    frame.extend_from_slice(&error.to_be_bytes());
    frame.extend_from_slice(payload);
    stream.write_all(&frame).await.unwrap();
}
