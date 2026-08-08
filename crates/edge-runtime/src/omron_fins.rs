use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use edge_core::{
    parse_omron_fins_endpoint, validate_omron_fins_point, DataQuality, OmronFinsArea,
    OmronFinsPointAddress, OmronFinsTransport, OmronFinsWordOrder, ProtocolConnection,
    ProtocolType, TelemetryPointMapping, TelemetrySample, TelemetryType, TelemetryValue,
};
use omron_fins::{
    Client, ClientConfig, FinsResponse, MemoryArea, NodeAddress, ReadWordCommand, WriteBitCommand,
    WriteWordCommand, MAX_WORDS_PER_COMMAND,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{lookup_host, TcpStream};
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::timeout;

use crate::{ProtocolAdapter, ProtocolCommandAdapter, ProtocolWriteResult};

/// Persistent Omron FINS adapter for CIO, WR, HR, DM and AR memory areas.
pub struct OmronFinsAdapter {
    connection: ProtocolConnection,
    mappings: Vec<TelemetryPointMapping>,
    client: Option<FinsSession>,
    connection_generation: u64,
}

#[derive(Clone)]
enum FinsSession {
    Udp(Arc<Client>),
    Tcp(Arc<AsyncMutex<FinsTcpClient>>),
}

struct FinsTcpClient {
    stream: TcpStream,
    source: NodeAddress,
    destination: NodeAddress,
    next_sid: u8,
    request_timeout: Duration,
}

const FINS_TCP_MAGIC: [u8; 4] = *b"FINS";
const FINS_TCP_NODE_ADDRESS_REQUEST: u32 = 0;
const FINS_TCP_NODE_ADDRESS_RESPONSE: u32 = 1;
const FINS_TCP_FRAME_SEND: u32 = 2;
const FINS_TCP_FRAME_RESPONSE: u32 = 3;
const MAX_FINS_TCP_BODY_SIZE: usize = 1_048_576;

struct FinsReadTarget {
    mapping: TelemetryPointMapping,
    address: OmronFinsPointAddress,
    output_index: usize,
}

struct FinsReadWindow {
    area: OmronFinsArea,
    start_word: u16,
    end_word_exclusive: u32,
    targets: Vec<FinsReadTarget>,
}

impl OmronFinsAdapter {
    pub fn new(
        connection: ProtocolConnection,
        mappings: Vec<TelemetryPointMapping>,
    ) -> Result<Self> {
        if connection.protocol != ProtocolType::OmronFins {
            bail!("Omron FINS adapter requires an Omron FINS connection");
        }
        connection.validate().map_err(anyhow::Error::msg)?;
        validate_mappings(&connection, &mappings)?;
        Ok(Self {
            connection,
            mappings,
            client: None,
            connection_generation: 0,
        })
    }

    pub fn set_mappings(&mut self, mappings: Vec<TelemetryPointMapping>) -> Result<()> {
        validate_mappings(&self.connection, &mappings)?;
        self.mappings = mappings;
        Ok(())
    }

    pub fn connection_generation(&self) -> u64 {
        self.connection_generation
    }

    async fn client(&mut self) -> Result<FinsSession> {
        if let Some(client) = &self.client {
            return Ok(client.clone());
        }
        let endpoint = self
            .connection
            .endpoint
            .as_deref()
            .context("Omron FINS endpoint is required")?;
        let (host, port) = parse_omron_fins_endpoint(endpoint).map_err(anyhow::Error::msg)?;
        let settings = self
            .connection
            .omron_fins
            .as_ref()
            .context("Omron FINS settings are required")?
            .clone();
        let client = match settings.transport {
            OmronFinsTransport::Udp => {
                let ip = lookup_host((host.as_str(), port))
                    .await
                    .with_context(|| {
                        format!("failed to resolve Omron FINS endpoint {host}:{port}")
                    })?
                    .find_map(|address| match address.ip() {
                        std::net::IpAddr::V4(ip) => Some(ip),
                        std::net::IpAddr::V6(_) => None,
                    })
                    .ok_or_else(|| {
                        anyhow!("Omron FINS endpoint {host}:{port} resolved to no IPv4 address")
                    })?;
                let config = client_config(ip, port, &settings);
                FinsSession::Udp(Arc::new(
                    Client::new(config).context("failed to create Omron FINS UDP client")?,
                ))
            }
            OmronFinsTransport::Tcp => FinsSession::Tcp(Arc::new(AsyncMutex::new(
                FinsTcpClient::connect(&host, port, &settings).await?,
            ))),
        };
        self.connection_generation = self.connection_generation.saturating_add(1);
        self.client = Some(client.clone());
        Ok(client)
    }

    fn clear_client(&mut self) {
        self.client = None;
    }
}

impl FinsTcpClient {
    async fn connect(
        host: &str,
        port: u16,
        settings: &edge_core::OmronFinsConnectionSettings,
    ) -> Result<Self> {
        let request_timeout = Duration::from_millis(settings.timeout_ms);
        let stream = timeout(request_timeout, TcpStream::connect((host, port)))
            .await
            .with_context(|| format!("Omron FINS/TCP connect to {host}:{port} timed out"))?
            .with_context(|| {
                format!("failed to connect to Omron FINS/TCP endpoint {host}:{port}")
            })?;
        let mut client = Self {
            stream,
            source: NodeAddress::local(),
            destination: NodeAddress::local(),
            next_sid: 1,
            request_timeout,
        };
        let requested_node = u32::from(settings.source_node);
        client
            .write_frame(
                FINS_TCP_NODE_ADDRESS_REQUEST,
                0,
                &requested_node.to_be_bytes(),
                "node-address request",
            )
            .await?;
        let (command, error, payload) = client.read_frame("node-address response").await?;
        if command != FINS_TCP_NODE_ADDRESS_RESPONSE {
            bail!(
                "Omron FINS/TCP node-address response used command {command}, expected {}",
                FINS_TCP_NODE_ADDRESS_RESPONSE
            );
        }
        if error != 0 {
            bail!("Omron FINS/TCP node-address handshake failed with error 0x{error:08x}");
        }
        if payload.len() != 8 {
            bail!(
                "Omron FINS/TCP node-address response must contain 8 bytes, got {}",
                payload.len()
            );
        }
        let assigned_source = tcp_node_from_payload(&payload[..4], "client")?;
        let assigned_destination = tcp_node_from_payload(&payload[4..], "server")?;
        client.source = NodeAddress::new(
            settings.source_network,
            if settings.source_node == 0 {
                assigned_source
            } else {
                settings.source_node
            },
            settings.source_unit,
        );
        client.destination = NodeAddress::new(
            settings.destination_network,
            if settings.destination_node == 0 {
                assigned_destination
            } else {
                settings.destination_node
            },
            settings.destination_unit,
        );
        Ok(client)
    }

    async fn read_words(
        &mut self,
        area: OmronFinsArea,
        start_word: u16,
        count: u16,
    ) -> Result<Vec<u16>> {
        let sid = self.take_sid();
        let command = ReadWordCommand::new(
            self.destination,
            self.source,
            sid,
            memory_area(area),
            start_word,
            count,
        )
        .map_err(anyhow::Error::new)?;
        self.execute(command.to_bytes(), sid)
            .await?
            .to_words()
            .map_err(anyhow::Error::new)
    }

    async fn write_words(
        &mut self,
        area: OmronFinsArea,
        start_word: u16,
        words: &[u16],
    ) -> Result<()> {
        let sid = self.take_sid();
        let command = WriteWordCommand::new(
            self.destination,
            self.source,
            sid,
            memory_area(area),
            start_word,
            words,
        )
        .map_err(anyhow::Error::new)?;
        self.execute(command.to_bytes(), sid).await?;
        Ok(())
    }

    async fn write_bit(
        &mut self,
        area: OmronFinsArea,
        word: u16,
        bit: u8,
        value: bool,
    ) -> Result<()> {
        let sid = self.take_sid();
        let command = WriteBitCommand::new(
            self.destination,
            self.source,
            sid,
            memory_area(area),
            word,
            bit,
            value,
        )
        .map_err(anyhow::Error::new)?;
        self.execute(command.to_bytes().map_err(anyhow::Error::new)?, sid)
            .await?;
        Ok(())
    }

    async fn execute(&mut self, fins_frame: Vec<u8>, sid: u8) -> Result<FinsResponse> {
        let expected_command = fins_frame
            .get(10..12)
            .context("Omron FINS command frame is too short")?;
        self.write_frame(FINS_TCP_FRAME_SEND, 0, &fins_frame, "command request")
            .await?;
        let (command, error, payload) = self.read_frame("command response").await?;
        if command != FINS_TCP_FRAME_RESPONSE {
            bail!(
                "Omron FINS/TCP response used command {command}, expected {}",
                FINS_TCP_FRAME_RESPONSE
            );
        }
        if error != 0 {
            bail!("Omron FINS/TCP command failed with error 0x{error:08x}");
        }
        let response = FinsResponse::from_bytes(&payload).map_err(anyhow::Error::new)?;
        response.check_sid(sid).map_err(anyhow::Error::new)?;
        response.check_error().map_err(anyhow::Error::new)?;
        if [response.mrc, response.src] != expected_command {
            bail!(
                "Omron FINS/TCP response command {:02x}{:02x} does not match request {:02x}{:02x}",
                response.mrc,
                response.src,
                expected_command[0],
                expected_command[1]
            );
        }
        Ok(response)
    }

    fn take_sid(&mut self) -> u8 {
        let sid = self.next_sid;
        self.next_sid = self.next_sid.wrapping_add(1);
        sid
    }

    async fn write_frame(
        &mut self,
        command: u32,
        error: u32,
        payload: &[u8],
        operation: &str,
    ) -> Result<()> {
        let body_length = payload
            .len()
            .checked_add(8)
            .and_then(|length| u32::try_from(length).ok())
            .context("Omron FINS/TCP frame is too large")?;
        let mut frame = Vec::with_capacity(payload.len() + 16);
        frame.extend_from_slice(&FINS_TCP_MAGIC);
        frame.extend_from_slice(&body_length.to_be_bytes());
        frame.extend_from_slice(&command.to_be_bytes());
        frame.extend_from_slice(&error.to_be_bytes());
        frame.extend_from_slice(payload);
        timeout(self.request_timeout, self.stream.write_all(&frame))
            .await
            .with_context(|| format!("Omron FINS/TCP {operation} timed out"))?
            .with_context(|| format!("failed to write Omron FINS/TCP {operation}"))?;
        Ok(())
    }

    async fn read_frame(&mut self, operation: &str) -> Result<(u32, u32, Vec<u8>)> {
        let mut prefix = [0_u8; 8];
        timeout(self.request_timeout, self.stream.read_exact(&mut prefix))
            .await
            .with_context(|| format!("Omron FINS/TCP {operation} timed out"))?
            .with_context(|| format!("failed to read Omron FINS/TCP {operation} header"))?;
        if prefix[..4] != FINS_TCP_MAGIC {
            bail!("Omron FINS/TCP {operation} has invalid magic bytes");
        }
        let body_length = u32::from_be_bytes(prefix[4..8].try_into().expect("four-byte length"));
        let body_length = usize::try_from(body_length).context("invalid FINS/TCP frame length")?;
        if !(8..=MAX_FINS_TCP_BODY_SIZE).contains(&body_length) {
            bail!(
                "Omron FINS/TCP {operation} body length {body_length} is outside 8..={MAX_FINS_TCP_BODY_SIZE}"
            );
        }
        let mut body = vec![0_u8; body_length];
        timeout(self.request_timeout, self.stream.read_exact(&mut body))
            .await
            .with_context(|| format!("Omron FINS/TCP {operation} timed out"))?
            .with_context(|| format!("failed to read Omron FINS/TCP {operation} body"))?;
        let command = u32::from_be_bytes(body[..4].try_into().expect("four-byte command"));
        let error = u32::from_be_bytes(body[4..8].try_into().expect("four-byte error"));
        Ok((command, error, body[8..].to_vec()))
    }
}

fn tcp_node_from_payload(payload: &[u8], role: &str) -> Result<u8> {
    let raw = u32::from_be_bytes(
        payload
            .try_into()
            .map_err(|_| anyhow!("Omron FINS/TCP {role} node address must contain 4 bytes"))?,
    );
    let node = raw as u8;
    if node == 0 || node == u8::MAX {
        bail!("Omron FINS/TCP assigned invalid {role} node {node}");
    }
    Ok(node)
}

fn client_config(
    ip: Ipv4Addr,
    port: u16,
    settings: &edge_core::OmronFinsConnectionSettings,
) -> ClientConfig {
    ClientConfig::new(ip, settings.source_node, settings.destination_node)
        .with_port(port)
        .with_timeout(Duration::from_millis(settings.timeout_ms))
        .with_source_network(settings.source_network)
        .with_source_unit(settings.source_unit)
        .with_dest_network(settings.destination_network)
        .with_dest_unit(settings.destination_unit)
}

#[async_trait]
impl ProtocolAdapter for OmronFinsAdapter {
    async fn read_telemetry(&mut self) -> Result<Vec<TelemetrySample>> {
        let mut targets = Vec::new();
        for mapping in self
            .mappings
            .iter()
            .filter(|mapping| mapping.access.is_readable())
        {
            let address =
                validate_omron_fins_point(&mapping.address, mapping.value_type, mapping.access)
                    .map_err(anyhow::Error::msg)
                    .with_context(|| format!("invalid Omron FINS point {}", mapping.point_id))?;
            targets.push(FinsReadTarget {
                mapping: mapping.clone(),
                address,
                output_index: targets.len(),
            });
        }
        if targets.is_empty() {
            return Ok(Vec::new());
        }
        let word_order = self
            .connection
            .omron_fins
            .as_ref()
            .context("Omron FINS settings are required")?
            .word_order;
        let client = self.client().await?;
        let target_count = targets.len();
        let windows = plan_read_windows(targets);
        let mut samples = std::iter::repeat_with(|| None)
            .take(target_count)
            .collect::<Vec<Option<TelemetrySample>>>();
        for window in windows {
            let count = u16::try_from(window.end_word_exclusive - u32::from(window.start_word))
                .context("Omron FINS read window exceeds the protocol limit")?;
            let operation = read_words(client.clone(), window.area, window.start_word, count)
                .await
                .with_context(|| {
                    format!(
                        "failed to read Omron FINS {}{}..{}",
                        window.area.canonical_prefix(),
                        window.start_word,
                        window.end_word_exclusive - 1
                    )
                });
            let words = match operation {
                Ok(words) => words,
                Err(error) => {
                    self.clear_client();
                    return Err(error);
                }
            };
            if words.len() != usize::from(count) {
                self.clear_client();
                bail!(
                    "Omron FINS read window returned {} words, expected {}",
                    words.len(),
                    count
                );
            }
            for target in window.targets {
                let value = decode_window_value(
                    &words,
                    window.start_word,
                    target.address,
                    target.mapping.value_type,
                    word_order,
                )
                .with_context(|| {
                    format!(
                        "failed to decode Omron FINS point {}",
                        target.mapping.point_id
                    )
                })?;
                samples[target.output_index] = Some(TelemetrySample::new(
                    target.mapping.device_id,
                    target.mapping.point_id,
                    value,
                    DataQuality::Good,
                    Utc::now(),
                ));
            }
        }
        samples
            .into_iter()
            .enumerate()
            .map(|(index, sample)| {
                sample.ok_or_else(|| anyhow!("Omron FINS sample {index} was not populated"))
            })
            .collect()
    }
}

#[async_trait]
impl ProtocolCommandAdapter for OmronFinsAdapter {
    async fn write_point(
        &mut self,
        mapping: &TelemetryPointMapping,
        value: TelemetryValue,
    ) -> Result<ProtocolWriteResult> {
        if mapping.protocol_connection_id != self.connection.connection_id {
            bail!(
                "Omron FINS point {} references connection {} instead of {}",
                mapping.point_id,
                mapping.protocol_connection_id,
                self.connection.connection_id
            );
        }
        if !mapping.access.is_writable() {
            bail!("Omron FINS point {} is not writable", mapping.point_id);
        }
        let address =
            validate_omron_fins_point(&mapping.address, mapping.value_type, mapping.access)
                .map_err(anyhow::Error::msg)
                .with_context(|| format!("invalid Omron FINS point {}", mapping.point_id))?;
        let word_order = self
            .connection
            .omron_fins
            .as_ref()
            .context("Omron FINS settings are required")?
            .word_order;
        let client = self.client().await?;
        let operation = write_value(client, address, value.clone(), word_order)
            .await
            .with_context(|| format!("failed to write Omron FINS point {}", mapping.point_id));
        if let Err(error) = operation {
            self.clear_client();
            return Err(error);
        }
        Ok(ProtocolWriteResult {
            point_id: mapping.point_id.clone(),
            value,
            verified: true,
            readback_value: None,
        })
    }
}

fn plan_read_windows(mut targets: Vec<FinsReadTarget>) -> Vec<FinsReadWindow> {
    targets.sort_by_key(|target| {
        (
            area_rank(target.address.area),
            target.address.word,
            target.output_index,
        )
    });
    let mut windows: Vec<FinsReadWindow> = Vec::new();
    for target in targets {
        let target_start = u32::from(target.address.word);
        let target_end = target_start + u32::from(target_word_width(&target));
        let can_merge = windows.last().is_some_and(|window| {
            window.area == target.address.area
                && target_start <= window.end_word_exclusive
                && target_end - u32::from(window.start_word) <= u32::from(MAX_WORDS_PER_COMMAND)
        });
        if can_merge {
            let window = windows.last_mut().expect("read window must exist");
            window.end_word_exclusive = window.end_word_exclusive.max(target_end);
            window.targets.push(target);
        } else {
            windows.push(FinsReadWindow {
                area: target.address.area,
                start_word: target.address.word,
                end_word_exclusive: target_end,
                targets: vec![target],
            });
        }
    }
    windows
}

fn target_word_width(target: &FinsReadTarget) -> u16 {
    if target.mapping.value_type == TelemetryType::Float {
        2
    } else {
        1
    }
}

const fn area_rank(area: OmronFinsArea) -> u8 {
    match area {
        OmronFinsArea::Cio => 0,
        OmronFinsArea::Work => 1,
        OmronFinsArea::Holding => 2,
        OmronFinsArea::DataMemory => 3,
        OmronFinsArea::Auxiliary => 4,
    }
}

async fn read_words(
    client: FinsSession,
    area: OmronFinsArea,
    start_word: u16,
    count: u16,
) -> Result<Vec<u16>> {
    match client {
        FinsSession::Udp(client) => tokio::task::spawn_blocking(move || {
            client
                .read(memory_area(area), start_word, count)
                .map_err(anyhow::Error::new)
        })
        .await
        .context("Omron FINS/UDP read task failed")?,
        FinsSession::Tcp(client) => {
            client
                .lock()
                .await
                .read_words(area, start_word, count)
                .await
        }
    }
}

fn decode_window_value(
    words: &[u16],
    start_word: u16,
    address: OmronFinsPointAddress,
    value_type: TelemetryType,
    word_order: OmronFinsWordOrder,
) -> Result<TelemetryValue> {
    let offset = usize::from(
        address
            .word
            .checked_sub(start_word)
            .context("Omron FINS point precedes its read window")?,
    );
    match address.bit {
        Some(bit) => words
            .get(offset)
            .copied()
            .map(|word| TelemetryValue::Boolean(word & (1_u16 << bit) != 0))
            .context("Omron FINS response did not contain the requested bit word"),
        None if value_type == TelemetryType::Integer => words
            .get(offset)
            .copied()
            .map(|value| TelemetryValue::Integer(i64::from(value)))
            .context("Omron FINS response did not contain the requested word"),
        None if value_type == TelemetryType::Float => {
            let end = offset.saturating_add(2);
            decode_f32(
                words
                    .get(offset..end)
                    .context("Omron FINS response did not contain the requested float words")?,
                word_order,
            )
        }
        _ => bail!("unsupported Omron FINS telemetry type {value_type:?}"),
    }
}

async fn write_value(
    client: FinsSession,
    address: OmronFinsPointAddress,
    value: TelemetryValue,
    word_order: OmronFinsWordOrder,
) -> Result<()> {
    match client {
        FinsSession::Udp(client) => {
            tokio::task::spawn_blocking(move || match (address.bit, value) {
                (Some(bit), TelemetryValue::Boolean(value)) => client
                    .write_bit(memory_area(address.area), address.word, bit, value)
                    .map_err(anyhow::Error::new),
                (None, TelemetryValue::Integer(value)) => {
                    let value = u16::try_from(value)
                        .context("Omron FINS integer writes must be between 0 and 65535")?;
                    client
                        .write(memory_area(address.area), address.word, &[value])
                        .map_err(anyhow::Error::new)
                }
                (None, TelemetryValue::Float(value)) => {
                    let value = value as f32;
                    if !value.is_finite() {
                        bail!("Omron FINS float writes must be finite");
                    }
                    let words = encode_f32(value, word_order);
                    client
                        .write(memory_area(address.area), address.word, &words)
                        .map_err(anyhow::Error::new)
                }
                _ => bail!("Omron FINS value type does not match the point address"),
            })
            .await
            .context("Omron FINS/UDP write task failed")?
        }
        FinsSession::Tcp(client) => {
            let mut client = client.lock().await;
            match (address.bit, value) {
                (Some(bit), TelemetryValue::Boolean(value)) => {
                    client
                        .write_bit(address.area, address.word, bit, value)
                        .await
                }
                (None, TelemetryValue::Integer(value)) => {
                    let value = u16::try_from(value)
                        .context("Omron FINS integer writes must be between 0 and 65535")?;
                    client
                        .write_words(address.area, address.word, &[value])
                        .await
                }
                (None, TelemetryValue::Float(value)) => {
                    let value = value as f32;
                    if !value.is_finite() {
                        bail!("Omron FINS float writes must be finite");
                    }
                    client
                        .write_words(address.area, address.word, &encode_f32(value, word_order))
                        .await
                }
                _ => bail!("Omron FINS value type does not match the point address"),
            }
        }
    }
}

fn decode_f32(words: &[u16], word_order: OmronFinsWordOrder) -> Result<TelemetryValue> {
    if words.len() != 2 {
        bail!("Omron FINS float response must contain exactly two words");
    }
    let (high, low) = match word_order {
        OmronFinsWordOrder::HighWordFirst => (words[0], words[1]),
        OmronFinsWordOrder::LowWordFirst => (words[1], words[0]),
    };
    Ok(TelemetryValue::Float(f64::from(f32::from_bits(
        (u32::from(high) << 16) | u32::from(low),
    ))))
}

fn encode_f32(value: f32, word_order: OmronFinsWordOrder) -> [u16; 2] {
    let bits = value.to_bits();
    let high = (bits >> 16) as u16;
    let low = bits as u16;
    match word_order {
        OmronFinsWordOrder::HighWordFirst => [high, low],
        OmronFinsWordOrder::LowWordFirst => [low, high],
    }
}

fn memory_area(area: OmronFinsArea) -> MemoryArea {
    match area {
        OmronFinsArea::Cio => MemoryArea::CIO,
        OmronFinsArea::Work => MemoryArea::WR,
        OmronFinsArea::Holding => MemoryArea::HR,
        OmronFinsArea::DataMemory => MemoryArea::DM,
        OmronFinsArea::Auxiliary => MemoryArea::AR,
    }
}

fn validate_mappings(
    connection: &ProtocolConnection,
    mappings: &[TelemetryPointMapping],
) -> Result<()> {
    for mapping in mappings {
        if mapping.protocol_connection_id != connection.connection_id {
            bail!(
                "Omron FINS point {} references connection {} instead of {}",
                mapping.point_id,
                mapping.protocol_connection_id,
                connection.connection_id
            );
        }
        validate_omron_fins_point(&mapping.address, mapping.value_type, mapping.access)
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("invalid Omron FINS point {}", mapping.point_id))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_word_order_round_trips() {
        for order in [
            OmronFinsWordOrder::HighWordFirst,
            OmronFinsWordOrder::LowWordFirst,
        ] {
            let words = encode_f32(25.5, order);
            assert_eq!(
                decode_f32(&words, order).unwrap(),
                TelemetryValue::Float(25.5)
            );
        }
    }

    #[test]
    fn adjacent_windows_respect_the_fins_command_limit() {
        let targets = (0..=MAX_WORDS_PER_COMMAND + 1)
            .enumerate()
            .map(|(output_index, word)| FinsReadTarget {
                mapping: TelemetryPointMapping::new(
                    format!("word-{word}"),
                    "plc-1",
                    format!("word-{word}"),
                    "fins-main",
                    edge_core::PointAddress::omron_fins(format!("D{word}")),
                    TelemetryType::Integer,
                ),
                address: OmronFinsPointAddress {
                    area: OmronFinsArea::DataMemory,
                    word,
                    bit: None,
                },
                output_index,
            })
            .collect();

        let windows = plan_read_windows(targets);

        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].start_word, 0);
        assert_eq!(windows[0].end_word_exclusive, 700);
        assert_eq!(windows[0].targets.len(), 700);
        assert_eq!(windows[1].start_word, 700);
        assert_eq!(windows[1].end_word_exclusive, 702);
        assert_eq!(windows[1].targets.len(), 2);
    }
}
