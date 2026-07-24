use std::{
    collections::BTreeMap,
    net::SocketAddr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use edge_core::{
    DataQuality, ProtocolConnection, ProtocolType, TelemetryPointMapping, TelemetrySample,
    TelemetryType, TelemetryValue,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};

use crate::ProtocolAdapter;

const READ_COILS: u8 = 0x01;
const READ_HOLDING_REGISTERS: u8 = 0x03;
const READ_INPUT_REGISTERS: u8 = 0x04;
const MAX_PDU_BYTES: usize = 253;

pub struct ModbusTcpAdapter {
    connection: ProtocolConnection,
    mappings: Vec<TelemetryPointMapping>,
    transaction_id: u16,
    connect_timeout: Duration,
    request_timeout: Duration,
    default_unit_id: u8,
}

impl ModbusTcpAdapter {
    pub fn new(connection: ProtocolConnection, mappings: Vec<TelemetryPointMapping>) -> Self {
        Self {
            connection,
            mappings,
            transaction_id: 0,
            connect_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(2),
            default_unit_id: 1,
        }
    }

    pub fn with_timeouts(mut self, connect_timeout: Duration, request_timeout: Duration) -> Self {
        self.connect_timeout = connect_timeout;
        self.request_timeout = request_timeout;
        self
    }

    pub fn with_default_unit_id(mut self, unit_id: u8) -> Self {
        self.default_unit_id = unit_id;
        self
    }

    async fn connect(&self) -> Result<TcpStream> {
        let target = parse_endpoint(self.connection.endpoint.as_deref())?;
        let stream = timeout(self.connect_timeout, TcpStream::connect(&target))
            .await
            .with_context(|| format!("Modbus TCP connect timeout: {target}"))?
            .with_context(|| format!("failed to connect to Modbus TCP endpoint {target}"))?;
        stream.set_nodelay(true)?;
        Ok(stream)
    }

    async fn transact(
        &mut self,
        stream: &mut TcpStream,
        address: ModbusAddress,
        quantity: u16,
    ) -> Result<Vec<u8>> {
        self.transaction_id = self.transaction_id.wrapping_add(1);
        let transaction_id = self.transaction_id;
        let request = build_request(transaction_id, address, quantity);

        timeout(self.request_timeout, stream.write_all(&request))
            .await
            .context("Modbus TCP request write timeout")??;

        let mut header = [0_u8; 7];
        timeout(self.request_timeout, stream.read_exact(&mut header))
            .await
            .context("Modbus TCP response header timeout")??;
        let response_transaction = u16::from_be_bytes([header[0], header[1]]);
        if response_transaction != transaction_id {
            bail!("Modbus TCP response transaction id does not match request");
        }
        if header[2] != 0 || header[3] != 0 {
            bail!("Modbus TCP response protocol id must be zero");
        }
        if header[6] != address.unit_id {
            bail!("Modbus TCP response unit id does not match request");
        }
        let mbap_length = u16::from_be_bytes([header[4], header[5]]) as usize;
        if !(2..=MAX_PDU_BYTES + 1).contains(&mbap_length) {
            bail!("Modbus TCP response has invalid MBAP length {mbap_length}");
        }

        let mut pdu = vec![0_u8; mbap_length - 1];
        timeout(self.request_timeout, stream.read_exact(&mut pdu))
            .await
            .context("Modbus TCP response body timeout")??;
        parse_response_pdu(&pdu, address.function, quantity)
    }
}

#[async_trait]
impl ProtocolAdapter for ModbusTcpAdapter {
    async fn read_telemetry(&mut self) -> Result<Vec<TelemetrySample>> {
        if self.connection.protocol != ProtocolType::ModbusTcp {
            bail!("Modbus TCP adapter requires a ModbusTcp protocol connection");
        }

        let mut stream = self.connect().await?;
        let mappings = self.mappings.clone();
        let mut samples = Vec::with_capacity(mappings.len());
        for mapping in mappings {
            if mapping.protocol_connection_id != self.connection.connection_id {
                continue;
            }
            let address = parse_address(
                &mapping.address.kind,
                &mapping.address.value,
                self.default_unit_id,
            )
            .with_context(|| format!("invalid point address for {}", mapping.point_id))?;
            let quantity = quantity_for(address.function, mapping.value_type)?;
            let payload = self.transact(&mut stream, address, quantity).await?;
            let value = decode_value(address.function, mapping.value_type, &payload)?;
            samples.push(TelemetrySample::new(
                mapping.device_id,
                mapping.point_id,
                value,
                DataQuality::Good,
                Utc::now(),
            ));
        }
        Ok(samples)
    }
}

#[derive(Clone, Copy, Debug)]
struct ModbusAddress {
    unit_id: u8,
    function: u8,
    offset: u16,
}

fn parse_endpoint(endpoint: Option<&str>) -> Result<String> {
    let endpoint = endpoint
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("Modbus TCP endpoint is required")?;
    let target = endpoint
        .strip_prefix("tcp://")
        .or_else(|| endpoint.strip_prefix("modbus://"))
        .unwrap_or(endpoint);
    if target.contains("://") || !target.contains(':') {
        bail!("invalid Modbus TCP endpoint: {endpoint}; expected host:port or tcp://host:port");
    }
    Ok(target.to_string())
}

fn parse_address(kind: &str, value: &str, default_unit_id: u8) -> Result<ModbusAddress> {
    let (function, reference_base) = match kind {
        "holding_register" => (READ_HOLDING_REGISTERS, 40_001_u32),
        "input_register" => (READ_INPUT_REGISTERS, 30_001_u32),
        "coil" => (READ_COILS, 1_u32),
        _ => bail!("Modbus TCP address kind is not supported: {kind}"),
    };
    let (unit_id, raw_address) = match value.split_once(':') {
        Some((unit, address)) => (
            unit.parse::<u8>()
                .with_context(|| format!("invalid Modbus unit id: {unit}"))?,
            address,
        ),
        None => (default_unit_id, value),
    };
    let raw_address = raw_address
        .parse::<u32>()
        .with_context(|| format!("invalid Modbus address: {raw_address}"))?;
    let offset = if raw_address >= reference_base {
        raw_address - reference_base
    } else if function == READ_COILS && raw_address > 0 {
        raw_address - 1
    } else {
        raw_address
    };
    Ok(ModbusAddress {
        unit_id,
        function,
        offset: u16::try_from(offset).context("Modbus address exceeds u16 range")?,
    })
}

fn quantity_for(function: u8, value_type: TelemetryType) -> Result<u16> {
    if function == READ_COILS {
        if value_type != TelemetryType::Boolean {
            bail!("Modbus coil points must use boolean telemetry type");
        }
        return Ok(1);
    }
    Ok(match value_type {
        TelemetryType::Float => 2,
        TelemetryType::Integer | TelemetryType::Boolean | TelemetryType::Text => 1,
    })
}

fn build_request(transaction_id: u16, address: ModbusAddress, quantity: u16) -> Vec<u8> {
    let mut frame = Vec::with_capacity(12);
    frame.extend(transaction_id.to_be_bytes());
    frame.extend(0_u16.to_be_bytes());
    frame.extend(6_u16.to_be_bytes());
    frame.push(address.unit_id);
    frame.push(address.function);
    frame.extend(address.offset.to_be_bytes());
    frame.extend(quantity.to_be_bytes());
    frame
}

fn parse_response_pdu(pdu: &[u8], function: u8, quantity: u16) -> Result<Vec<u8>> {
    if pdu.len() < 2 {
        bail!("Modbus TCP response PDU is too short");
    }
    if pdu[0] == function | 0x80 {
        bail!("Modbus TCP exception code {}", pdu[1]);
    }
    if pdu[0] != function {
        bail!("Modbus TCP response function does not match request");
    }
    let byte_count = pdu[1] as usize;
    let expected = if function == READ_COILS {
        quantity.div_ceil(8) as usize
    } else {
        quantity as usize * 2
    };
    if byte_count != expected || pdu.len() != byte_count + 2 {
        bail!("Modbus TCP response byte count does not match request");
    }
    Ok(pdu[2..].to_vec())
}

fn decode_value(function: u8, value_type: TelemetryType, payload: &[u8]) -> Result<TelemetryValue> {
    if function == READ_COILS {
        return Ok(TelemetryValue::Boolean(payload[0] & 1 != 0));
    }
    let registers = payload
        .chunks_exact(2)
        .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    match value_type {
        TelemetryType::Float => {
            if registers.len() < 2 {
                bail!("float value requires two Modbus registers");
            }
            let bytes = [
                (registers[0] >> 8) as u8,
                registers[0] as u8,
                (registers[1] >> 8) as u8,
                registers[1] as u8,
            ];
            Ok(TelemetryValue::Float(f32::from_be_bytes(bytes) as f64))
        }
        TelemetryType::Integer => Ok(TelemetryValue::Integer(registers[0] as i64)),
        TelemetryType::Boolean => Ok(TelemetryValue::Boolean(registers[0] != 0)),
        TelemetryType::Text => Ok(TelemetryValue::Text(registers[0].to_string())),
    }
}

#[derive(Clone, Debug)]
pub struct ModbusTcpSimulatorOptions {
    pub bind: SocketAddr,
    pub unit_id: u8,
    pub holding_registers: BTreeMap<u16, u16>,
    pub input_registers: BTreeMap<u16, u16>,
    pub coils: BTreeMap<u16, bool>,
    pub dynamic_holding_floats: BTreeMap<u16, DynamicFloatPoint>,
    pub toggling_coils: BTreeMap<u16, Duration>,
}

impl ModbusTcpSimulatorOptions {
    pub fn new(bind: SocketAddr) -> Self {
        Self {
            bind,
            unit_id: 1,
            holding_registers: BTreeMap::new(),
            input_registers: BTreeMap::new(),
            coils: BTreeMap::new(),
            dynamic_holding_floats: BTreeMap::new(),
            toggling_coils: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DynamicFloatPoint {
    pub base: f32,
    pub amplitude: f32,
    pub period: Duration,
}

impl DynamicFloatPoint {
    pub fn new(base: f32, amplitude: f32, period: Duration) -> Self {
        Self {
            base,
            amplitude,
            period,
        }
    }

    fn value(self, now: Duration) -> f32 {
        let period = self.period.as_secs_f64().max(0.001);
        let elapsed = now.as_secs_f64().rem_euclid(period);
        let phase = elapsed / period * std::f64::consts::TAU;
        self.base + self.amplitude * phase.sin() as f32
    }
}

pub struct ModbusTcpSimulator {
    listener: TcpListener,
    options: ModbusTcpSimulatorOptions,
}

impl ModbusTcpSimulator {
    pub async fn bind(options: ModbusTcpSimulatorOptions) -> Result<Self> {
        let listener = TcpListener::bind(options.bind).await?;
        Ok(Self { listener, options })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }

    pub async fn run(self) -> Result<()> {
        loop {
            let (stream, _) = self.listener.accept().await?;
            let options = self.options.clone();
            tokio::spawn(async move {
                if let Err(error) = serve_connection(stream, options).await {
                    tracing::debug!(%error, "Modbus TCP simulator connection closed");
                }
            });
        }
    }
}

async fn serve_connection(mut stream: TcpStream, options: ModbusTcpSimulatorOptions) -> Result<()> {
    loop {
        let mut request = [0_u8; 12];
        match stream.read_exact(&mut request).await {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error.into()),
        }
        let transaction = [request[0], request[1]];
        let protocol_id = u16::from_be_bytes([request[2], request[3]]);
        let length = u16::from_be_bytes([request[4], request[5]]);
        let unit = request[6];
        let function = request[7];
        let offset = u16::from_be_bytes([request[8], request[9]]);
        let quantity = u16::from_be_bytes([request[10], request[11]]);
        let pdu = if protocol_id != 0 || length != 6 || unit != options.unit_id {
            vec![function | 0x80, 0x03]
        } else {
            simulator_response_pdu(&options, function, offset, quantity)
        };
        let mut response = Vec::with_capacity(7 + pdu.len());
        response.extend(transaction);
        response.extend(0_u16.to_be_bytes());
        response.extend(((pdu.len() + 1) as u16).to_be_bytes());
        response.push(unit);
        response.extend(pdu);
        stream.write_all(&response).await?;
    }
}

fn simulator_response_pdu(
    options: &ModbusTcpSimulatorOptions,
    function: u8,
    offset: u16,
    quantity: u16,
) -> Vec<u8> {
    if quantity == 0 || quantity > 125 {
        return vec![function | 0x80, 0x03];
    }
    match function {
        READ_HOLDING_REGISTERS | READ_INPUT_REGISTERS => {
            let source = if function == READ_HOLDING_REGISTERS {
                &options.holding_registers
            } else {
                &options.input_registers
            };
            let mut pdu = vec![function, (quantity * 2) as u8];
            let now = unix_time();
            for index in 0..quantity {
                let register = offset.saturating_add(index);
                let value = if function == READ_HOLDING_REGISTERS {
                    dynamic_float_register(options, register, now)
                        .or_else(|| source.get(&register).copied())
                        .unwrap_or(0)
                } else {
                    source.get(&register).copied().unwrap_or(0)
                };
                pdu.extend(value.to_be_bytes());
            }
            pdu
        }
        READ_COILS => {
            let byte_count = quantity.div_ceil(8) as usize;
            let mut bytes = vec![0_u8; byte_count];
            let now = unix_time();
            for index in 0..quantity {
                let coil = offset.saturating_add(index);
                let value = options
                    .toggling_coils
                    .get(&coil)
                    .map(|period| {
                        let period_ms = period.as_millis().max(1);
                        (now.as_millis() / period_ms).is_multiple_of(2)
                    })
                    .unwrap_or_else(|| options.coils.get(&coil).copied().unwrap_or(false));
                if value {
                    bytes[index as usize / 8] |= 1 << (index % 8);
                }
            }
            let mut pdu = vec![function, byte_count as u8];
            pdu.extend(bytes);
            pdu
        }
        _ => vec![function | 0x80, 0x01],
    }
}

fn unix_time() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

fn dynamic_float_register(
    options: &ModbusTcpSimulatorOptions,
    register: u16,
    now: Duration,
) -> Option<u16> {
    if let Some(point) = options.dynamic_holding_floats.get(&register) {
        let bytes = point.value(now).to_be_bytes();
        return Some(u16::from_be_bytes([bytes[0], bytes[1]]));
    }
    let start = register.checked_sub(1)?;
    let point = options.dynamic_holding_floats.get(&start)?;
    let bytes = point.value(now).to_be_bytes();
    Some(u16::from_be_bytes([bytes[2], bytes[3]]))
}
