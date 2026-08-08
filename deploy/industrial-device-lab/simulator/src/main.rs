use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use bytes::Bytes;
use clap::{Parser, Subcommand};
use futures::{SinkExt, StreamExt};
use snap7_server::{area, DataStore, S7Server, ServerConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::time::{interval, timeout, MissedTickBehavior};
use tokio_util::codec::Framed;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;
use voltage_iec104::{
    Apci, Apdu, Asdu, AsduHeader, Cot, Iec104Codec, InformationObject, Ioa, TypeId, UFunction,
};

mod bacnet;

#[derive(Debug, Parser)]
#[command(name = "protocol-device-sim")]
#[command(about = "Container-oriented industrial protocol device simulator")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    ServeS7 {
        #[arg(long, default_value = "0.0.0.0:1102")]
        bind: SocketAddr,
        #[arg(long, default_value_t = 250)]
        update_interval_ms: u64,
    },
    ServeFins {
        #[arg(long, default_value = "0.0.0.0:9600")]
        bind: SocketAddr,
        #[arg(long, default_value_t = 250)]
        update_interval_ms: u64,
        #[arg(long, default_value_t = 42)]
        server_node: u8,
    },
    ServeIec104 {
        #[arg(long, default_value = "0.0.0.0:2404")]
        bind: SocketAddr,
        #[arg(long, default_value_t = 1)]
        common_address: u16,
        #[arg(long, default_value_t = 500)]
        update_interval_ms: u64,
    },
    ServeBacnet {
        #[arg(long, default_value = "0.0.0.0:47808")]
        bind: SocketAddr,
        #[arg(long, default_value_t = 42)]
        device_instance: u32,
        #[arg(long, default_value_t = 500)]
        update_interval_ms: u64,
    },
    CheckBacnet {
        #[arg(long)]
        address: SocketAddr,
        #[arg(long, default_value_t = 42)]
        device_instance: u32,
        #[arg(long, default_value_t = 2_000)]
        timeout_ms: u64,
    },
    CheckTcp {
        #[arg(long)]
        address: SocketAddr,
        #[arg(long, default_value_t = 2_000)]
        timeout_ms: u64,
    },
}

#[derive(Default)]
struct FinsMemory {
    words: HashMap<(u8, u16), u16>,
    bits: HashMap<(u8, u16, u8), bool>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .compact()
        .init();

    match Cli::parse().command {
        Command::ServeS7 {
            bind,
            update_interval_ms,
        } => serve_s7(bind, update_interval_ms).await,
        Command::ServeFins {
            bind,
            update_interval_ms,
            server_node,
        } => serve_fins(bind, update_interval_ms, server_node).await,
        Command::ServeIec104 {
            bind,
            common_address,
            update_interval_ms,
        } => serve_iec104(bind, common_address, update_interval_ms).await,
        Command::ServeBacnet {
            bind,
            device_instance,
            update_interval_ms,
        } => bacnet::serve(bind, device_instance, update_interval_ms).await,
        Command::CheckBacnet {
            address,
            device_instance,
            timeout_ms,
        } => bacnet::check(address, device_instance, timeout_ms).await,
        Command::CheckTcp {
            address,
            timeout_ms,
        } => check_tcp(address, timeout_ms).await,
    }
}

#[derive(Clone, Copy)]
struct Iec104State {
    pressure: f32,
    running: bool,
    breaker_closed: bool,
    breaker_position: u8,
    setpoint: f32,
}

impl Default for Iec104State {
    fn default() -> Self {
        Self {
            pressure: 2.4,
            running: true,
            breaker_closed: false,
            breaker_position: 1,
            setpoint: 10.0,
        }
    }
}

async fn serve_iec104(
    bind: SocketAddr,
    common_address: u16,
    update_interval_ms: u64,
) -> Result<()> {
    if common_address == 0 {
        bail!("IEC 104 common address must be greater than zero");
    }
    let listener = TcpListener::bind(bind)
        .await
        .with_context(|| format!("failed to bind IEC 104 simulator on {bind}"))?;
    let state = Arc::new(Mutex::new(Iec104State::default()));
    let update_state = Arc::clone(&state);
    let update_period = Duration::from_millis(update_interval_ms.max(100));
    let updater = tokio::spawn(async move {
        let mut ticker = interval(update_period);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let started = Instant::now();
        loop {
            ticker.tick().await;
            let elapsed = started.elapsed().as_secs_f32();
            let mut state = update_state.lock().expect("IEC 104 state mutex poisoned");
            state.pressure = 2.4 + 0.2 * (elapsed * std::f32::consts::TAU / 20.0).sin();
            state.running = state.breaker_closed;
        }
    });
    info!(%bind, common_address, "IEC 104 simulated RTU is ready");

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, peer) = accepted.context("IEC 104 accept failed")?;
                let connection_state = Arc::clone(&state);
                tokio::spawn(async move {
                    if let Err(error) = serve_iec104_connection(
                        stream,
                        connection_state,
                        common_address,
                        update_period,
                    ).await {
                        warn!(%peer, %error, "IEC 104 client disconnected");
                    }
                });
            }
            result = tokio::signal::ctrl_c() => {
                result.context("failed to wait for shutdown signal")?;
                break;
            }
        }
    }

    updater.abort();
    let _ = updater.await;
    Ok(())
}

async fn serve_iec104_connection(
    stream: TcpStream,
    state: Arc<Mutex<Iec104State>>,
    common_address: u16,
    update_period: Duration,
) -> Result<()> {
    let mut framed = Framed::new(stream, Iec104Codec::new());
    let mut send_sequence = 0_u16;
    let mut receive_sequence = 0_u16;
    let mut active = false;
    let mut spontaneous = interval(update_period);
    spontaneous.set_missed_tick_behavior(MissedTickBehavior::Skip);
    spontaneous.tick().await;

    loop {
        tokio::select! {
            frame = framed.next() => {
                let frame = frame.context("IEC 104 client closed the connection")??;
                match frame.apci {
                    Apci::UFrame { function: UFunction::StartDtAct } => {
                        active = true;
                        framed.send(Apdu::u_frame(UFunction::StartDtCon)).await?;
                    }
                    Apci::UFrame { function: UFunction::StopDtAct } => {
                        active = false;
                        framed.send(Apdu::u_frame(UFunction::StopDtCon)).await?;
                    }
                    Apci::IFrame { send_seq, .. } if active => {
                        receive_sequence = next_iec104_sequence(send_seq);
                        if let Some(asdu) = frame.asdu {
                            handle_iec104_request(
                                &mut framed,
                                &state,
                                common_address,
                                &mut send_sequence,
                                receive_sequence,
                                asdu,
                            ).await?;
                        }
                    }
                    _ => {}
                }
            }
            _ = spontaneous.tick(), if active => {
                let snapshot = *state.lock().expect("IEC 104 state mutex poisoned");
                for asdu in iec104_spontaneous_asdus(common_address, snapshot) {
                    send_iec104_asdu(
                        &mut framed,
                        &mut send_sequence,
                        receive_sequence,
                        asdu,
                    ).await?;
                }
            }
        }
    }
}

async fn handle_iec104_request(
    framed: &mut Framed<TcpStream, Iec104Codec>,
    state: &Arc<Mutex<Iec104State>>,
    common_address: u16,
    send_sequence: &mut u16,
    receive_sequence: u16,
    asdu: Asdu,
) -> Result<()> {
    if asdu.header.common_address != common_address {
        bail!(
            "IEC 104 request targets common address {} instead of {}",
            asdu.header.common_address,
            common_address
        );
    }
    match asdu.header.type_id {
        TypeId::InterrogationCommand => {
            let snapshot = *state.lock().expect("IEC 104 state mutex poisoned");
            for response in iec104_interrogation_asdus(common_address, snapshot) {
                send_iec104_asdu(framed, send_sequence, receive_sequence, response).await?;
            }
        }
        TypeId::SingleCommand | TypeId::DoubleCommand | TypeId::SetpointFloat => {
            let (ioa, payload) = parse_iec104_command(&asdu)?;
            let select = match asdu.header.type_id {
                TypeId::SetpointFloat => payload[4] & 0x80 != 0,
                _ => payload[0] & 0x80 != 0,
            };
            if !select {
                let mut state = state.lock().expect("IEC 104 state mutex poisoned");
                match (asdu.header.type_id, ioa) {
                    (TypeId::SingleCommand, 1201) => {
                        state.breaker_closed = payload[0] & 0x01 != 0;
                    }
                    (TypeId::DoubleCommand, 1202) => {
                        let value = payload[0] & 0x03;
                        if !matches!(value, 1 | 2) {
                            bail!("invalid IEC 104 double command value {value}");
                        }
                        state.breaker_position = value;
                    }
                    (TypeId::SetpointFloat, 1203) => {
                        state.setpoint = f32::from_le_bytes(payload[..4].try_into()?);
                    }
                    (type_id, ioa) => {
                        bail!("unsupported IEC 104 command {type_id:?} for IOA {ioa}")
                    }
                }
            }
            let confirmation = iec104_object_asdu(
                asdu.header.type_id,
                Cot::ActivationConfirm,
                common_address,
                ioa,
                Bytes::copy_from_slice(payload),
            );
            send_iec104_asdu(framed, send_sequence, receive_sequence, confirmation).await?;
        }
        type_id => bail!("unsupported IEC 104 request type {type_id:?}"),
    }
    Ok(())
}

fn parse_iec104_command(asdu: &Asdu) -> Result<(u32, &[u8])> {
    let minimum_payload = match asdu.header.type_id {
        TypeId::SetpointFloat => 5,
        TypeId::SingleCommand | TypeId::DoubleCommand => 1,
        _ => bail!("ASDU is not an IEC 104 command"),
    };
    if asdu.raw_data.len() < 3 + minimum_payload {
        bail!("short IEC 104 command payload");
    }
    let ioa = Ioa::from_bytes(&asdu.raw_data[..3])?.value();
    Ok((ioa, &asdu.raw_data[3..3 + minimum_payload]))
}

fn iec104_interrogation_asdus(common_address: u16, state: Iec104State) -> Vec<Asdu> {
    vec![
        iec104_float_asdu(
            Cot::InterrogatedByStation,
            common_address,
            1001,
            state.pressure,
        ),
        iec104_single_asdu(
            Cot::InterrogatedByStation,
            common_address,
            1002,
            state.running,
        ),
        iec104_single_asdu(
            Cot::InterrogatedByStation,
            common_address,
            1201,
            state.breaker_closed,
        ),
        iec104_double_asdu(
            Cot::InterrogatedByStation,
            common_address,
            1202,
            state.breaker_position,
        ),
        iec104_float_asdu(
            Cot::InterrogatedByStation,
            common_address,
            1203,
            state.setpoint,
        ),
    ]
}

fn iec104_spontaneous_asdus(common_address: u16, state: Iec104State) -> Vec<Asdu> {
    vec![
        iec104_float_asdu(Cot::Spontaneous, common_address, 1001, state.pressure),
        iec104_single_asdu(Cot::Spontaneous, common_address, 1002, state.running),
    ]
}

fn iec104_float_asdu(cot: Cot, common_address: u16, ioa: u32, value: f32) -> Asdu {
    let mut data = value.to_le_bytes().to_vec();
    data.push(0);
    iec104_object_asdu(
        TypeId::MeasuredFloat,
        cot,
        common_address,
        ioa,
        Bytes::from(data),
    )
}

fn iec104_single_asdu(cot: Cot, common_address: u16, ioa: u32, value: bool) -> Asdu {
    iec104_object_asdu(
        TypeId::SinglePoint,
        cot,
        common_address,
        ioa,
        Bytes::from(vec![u8::from(value)]),
    )
}

fn iec104_double_asdu(cot: Cot, common_address: u16, ioa: u32, value: u8) -> Asdu {
    iec104_object_asdu(
        TypeId::DoublePoint,
        cot,
        common_address,
        ioa,
        Bytes::from(vec![value & 0x03]),
    )
}

fn iec104_object_asdu(
    type_id: TypeId,
    cot: Cot,
    common_address: u16,
    ioa: u32,
    data: Bytes,
) -> Asdu {
    let mut asdu = Asdu::new(AsduHeader::new(type_id, 1, cot, common_address));
    asdu.objects
        .push(InformationObject::new(Ioa::new(ioa), data));
    asdu
}

async fn send_iec104_asdu(
    framed: &mut Framed<TcpStream, Iec104Codec>,
    send_sequence: &mut u16,
    receive_sequence: u16,
    asdu: Asdu,
) -> Result<()> {
    framed
        .send(Apdu::i_frame(*send_sequence, receive_sequence, asdu))
        .await?;
    *send_sequence = next_iec104_sequence(*send_sequence);
    Ok(())
}

fn next_iec104_sequence(sequence: u16) -> u16 {
    sequence.wrapping_add(1) & 0x7fff
}

async fn check_tcp(address: SocketAddr, timeout_ms: u64) -> Result<()> {
    timeout(
        Duration::from_millis(timeout_ms),
        TcpStream::connect(address),
    )
    .await
    .context("TCP readiness check timed out")??;
    Ok(())
}

async fn serve_s7(bind: SocketAddr, update_interval_ms: u64) -> Result<()> {
    let store = DataStore::new();
    write_s7_values(&store, 2.4, true, 1_450);
    store.write_bytes(1, 10, &[1]);

    let server = S7Server::bind(ServerConfig {
        bind_addr: bind,
        max_connections: 32,
    })
    .await
    .with_context(|| format!("failed to bind S7 simulator on {bind}"))?;
    let local_addr = server.local_addr()?;
    let server_store = store.clone();
    let server_task = tokio::spawn(async move { server.serve(server_store).await });
    info!(%local_addr, "Siemens S7 simulated PLC is ready");

    let mut ticker = interval(Duration::from_millis(update_interval_ms.max(50)));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let started = Instant::now();
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let elapsed = started.elapsed().as_secs_f32();
                let command = store.read_bytes(1, 10, 1).first().copied().unwrap_or_default() & 1 != 0;
                let pressure = 2.4 + 0.18 * (elapsed * std::f32::consts::TAU / 20.0).sin();
                let speed = if command { 1_450 + (elapsed * 0.5).sin().mul_add(25.0, 0.0) as i32 } else { 0 };
                write_s7_values(&store, pressure, command, speed);
            }
            result = tokio::signal::ctrl_c() => {
                result.context("failed to wait for shutdown signal")?;
                break;
            }
        }
    }

    server_task.abort();
    let _ = server_task.await;
    Ok(())
}

fn write_s7_values(store: &DataStore, pressure: f32, running: bool, speed: i32) {
    store.write_bytes(1, 0, &pressure.to_be_bytes());
    store.write_bytes(1, 4, &[u8::from(running)]);
    store.write_bytes(1, 6, &speed.to_be_bytes());
    store.write_area(area::MARKERS, 0, 0, &[0, 0]);
}

async fn serve_fins(bind: SocketAddr, update_interval_ms: u64, server_node: u8) -> Result<()> {
    if server_node == 0 {
        bail!("FINS server node must be in the range 1..=255");
    }

    let temperature_bits = 25.5_f32.to_bits();
    let memory = Arc::new(Mutex::new(FinsMemory {
        words: HashMap::from([
            ((0x82, 100), 1),
            ((0x82, 102), temperature_bits as u16),
            ((0x82, 103), (temperature_bits >> 16) as u16),
        ]),
        bits: HashMap::from([((0x30, 0, 0), true), ((0x30, 0, 1), true)]),
    }));

    let udp = UdpSocket::bind(bind)
        .await
        .with_context(|| format!("failed to bind FINS/UDP simulator on {bind}"))?;
    let tcp = TcpListener::bind(bind)
        .await
        .with_context(|| format!("failed to bind FINS/TCP simulator on {bind}"))?;
    info!(%bind, server_node, "Omron FINS TCP/UDP simulated PLC is ready");

    let udp_memory = Arc::clone(&memory);
    let udp_task = tokio::spawn(async move {
        let mut buffer = [0_u8; 4_096];
        loop {
            let (length, peer) = udp.recv_from(&mut buffer).await?;
            match handle_fins_request(&buffer[..length], &udp_memory) {
                Ok(response) => udp.send_to(&response, peer).await.map(|_| ())?,
                Err(error) => warn!(%peer, %error, "rejected malformed FINS/UDP request"),
            }
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    });

    let tcp_memory = Arc::clone(&memory);
    let tcp_task = tokio::spawn(async move {
        loop {
            let (stream, peer) = tcp.accept().await?;
            let connection_memory = Arc::clone(&tcp_memory);
            tokio::spawn(async move {
                if let Err(error) =
                    serve_fins_tcp_connection(stream, connection_memory, server_node).await
                {
                    warn!(%peer, %error, "FINS/TCP client disconnected");
                }
            });
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    });

    let mut ticker = interval(Duration::from_millis(update_interval_ms.max(50)));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let started = Instant::now();
    loop {
        tokio::select! {
            _ = ticker.tick() => update_fins_values(&memory, started.elapsed()),
            result = tokio::signal::ctrl_c() => {
                result.context("failed to wait for shutdown signal")?;
                break;
            }
        }
    }

    udp_task.abort();
    tcp_task.abort();
    let _ = udp_task.await;
    let _ = tcp_task.await;
    Ok(())
}

fn update_fins_values(memory: &Arc<Mutex<FinsMemory>>, elapsed: Duration) {
    let elapsed_seconds = elapsed.as_secs_f32();
    let mut memory = memory.lock().expect("FINS memory mutex poisoned");
    let counter = (elapsed.as_millis() / 250).min(u16::MAX as u128) as u16;
    let temperature = 25.5 + 2.0 * (elapsed_seconds * std::f32::consts::TAU / 18.0).sin();
    let temperature_bits = temperature.to_bits();
    let start_command = memory.bits.get(&(0x30, 0, 1)).copied().unwrap_or(false);
    memory.words.insert((0x82, 100), counter);
    memory.words.insert((0x82, 102), temperature_bits as u16);
    memory
        .words
        .insert((0x82, 103), (temperature_bits >> 16) as u16);
    memory.bits.insert((0x30, 0, 0), start_command);
}

async fn serve_fins_tcp_connection(
    mut stream: TcpStream,
    memory: Arc<Mutex<FinsMemory>>,
    server_node: u8,
) -> Result<()> {
    let (command, error, requested_node) = read_fins_tcp_frame(&mut stream).await?;
    if command != 0 || error != 0 || requested_node.len() != 4 {
        bail!("invalid FINS/TCP node handshake");
    }
    let requested = u32::from_be_bytes(requested_node.as_slice().try_into()?);
    let client_node = if requested == 0 { 11 } else { requested as u8 };
    let mut assigned_nodes = Vec::with_capacity(8);
    assigned_nodes.extend_from_slice(&(client_node as u32).to_be_bytes());
    assigned_nodes.extend_from_slice(&(server_node as u32).to_be_bytes());
    write_fins_tcp_frame(&mut stream, 1, 0, &assigned_nodes).await?;

    loop {
        let (command, error, request) = read_fins_tcp_frame(&mut stream).await?;
        if command != 2 || error != 0 {
            bail!("invalid FINS/TCP command envelope");
        }
        let response = handle_fins_request(&request, &memory)?;
        write_fins_tcp_frame(&mut stream, 3, 0, &response).await?;
    }
}

fn handle_fins_request(request: &[u8], memory: &Arc<Mutex<FinsMemory>>) -> Result<Vec<u8>> {
    if request.len() < 18 {
        bail!("short FINS request: {} bytes", request.len());
    }
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
    let mut memory = memory.lock().expect("FINS memory mutex poisoned");

    match command {
        (0x01, 0x01) if matches!(area, 0x30..=0x33) => {
            for offset in 0..count {
                response.push(u8::from(
                    memory
                        .bits
                        .get(&(area, word + offset as u16, bit))
                        .copied()
                        .unwrap_or(false),
                ));
            }
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
                    for current_bit in 0..16 {
                        match memory.bits.get(&(bit_area, current_word, current_bit)) {
                            Some(true) => value |= 1_u16 << current_bit,
                            Some(false) => value &= !(1_u16 << current_bit),
                            None => {}
                        }
                    }
                }
                response.extend_from_slice(&value.to_be_bytes());
            }
        }
        (0x01, 0x02) if matches!(area, 0x30..=0x33) => {
            if request.len() < 18 + count {
                bail!("short FINS bit write payload");
            }
            for offset in 0..count {
                memory
                    .bits
                    .insert((area, word + offset as u16, bit), request[18 + offset] != 0);
            }
        }
        (0x01, 0x02) => {
            if request.len() < 18 + count * 2 {
                bail!("short FINS word write payload");
            }
            for offset in 0..count {
                let start = 18 + offset * 2;
                let value = u16::from_be_bytes([request[start], request[start + 1]]);
                memory.words.insert((area, word + offset as u16), value);
            }
        }
        _ => {
            response[12] = 0x01;
            response[13] = 0x01;
        }
    }
    Ok(response)
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

async fn read_fins_tcp_frame(stream: &mut TcpStream) -> io::Result<(u32, u32, Vec<u8>)> {
    let mut prefix = [0_u8; 8];
    stream.read_exact(&mut prefix).await?;
    if &prefix[..4] != b"FINS" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid FINS/TCP signature",
        ));
    }
    let body_length = u32::from_be_bytes(prefix[4..].try_into().expect("4-byte length")) as usize;
    if !(8..=4_096).contains(&body_length) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid FINS/TCP frame length",
        ));
    }
    let mut body = vec![0_u8; body_length];
    stream.read_exact(&mut body).await?;
    Ok((
        u32::from_be_bytes(body[..4].try_into().expect("4-byte command")),
        u32::from_be_bytes(body[4..8].try_into().expect("4-byte error")),
        body[8..].to_vec(),
    ))
}

async fn write_fins_tcp_frame(
    stream: &mut TcpStream,
    command: u32,
    error: u32,
    payload: &[u8],
) -> io::Result<()> {
    let mut frame = Vec::with_capacity(payload.len() + 16);
    frame.extend_from_slice(b"FINS");
    frame.extend_from_slice(&u32::try_from(payload.len() + 8).unwrap().to_be_bytes());
    frame.extend_from_slice(&command.to_be_bytes());
    frame.extend_from_slice(&error.to_be_bytes());
    frame.extend_from_slice(payload);
    stream.write_all(&frame).await
}
