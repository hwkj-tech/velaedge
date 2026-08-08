use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use edge_core::{
    validate_modbus_point_options, DataQuality, PointAddress, ProtocolConnection, ProtocolType,
    TelemetryPointMapping, TelemetrySample, TelemetryValue,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Mutex,
    time::timeout,
};

use crate::{
    modbus_batch::{extract_point_payload, plan_read_windows, ModbusReadPoint},
    modbus_codec::{decode_modbus_value, encode_modbus_register_values, modbus_quantity},
    ProtocolAdapter, ProtocolCommandAdapter, ProtocolPointWrite, ProtocolWriteResult,
};

const READ_COILS: u8 = 0x01;
const READ_DISCRETE_INPUTS: u8 = 0x02;
const READ_HOLDING_REGISTERS: u8 = 0x03;
const READ_INPUT_REGISTERS: u8 = 0x04;
const WRITE_SINGLE_COIL: u8 = 0x05;
const WRITE_SINGLE_REGISTER: u8 = 0x06;
const WRITE_MULTIPLE_COILS: u8 = 0x0F;
const WRITE_MULTIPLE_REGISTERS: u8 = 0x10;
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

    pub(crate) fn batchable_write_prefix(&self, writes: &[ProtocolPointWrite]) -> usize {
        (2..=writes.len())
            .rev()
            .find(|length| matches!(self.prepare_write_batch(&writes[..*length]), Ok(Some(_))))
            .unwrap_or(1.min(writes.len()))
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

    async fn transact_write(
        &mut self,
        stream: &mut TcpStream,
        address: ModbusAddress,
        command: &ModbusWriteCommand,
    ) -> Result<()> {
        self.transaction_id = self.transaction_id.wrapping_add(1);
        let transaction_id = self.transaction_id;
        let request = build_write_request(transaction_id, address, command);

        timeout(self.request_timeout, stream.write_all(&request))
            .await
            .context("Modbus TCP write request timeout")??;

        let mut header = [0_u8; 7];
        timeout(self.request_timeout, stream.read_exact(&mut header))
            .await
            .context("Modbus TCP write response header timeout")??;
        if u16::from_be_bytes([header[0], header[1]]) != transaction_id {
            bail!("Modbus TCP write response transaction id does not match request");
        }
        if header[2] != 0 || header[3] != 0 {
            bail!("Modbus TCP write response protocol id must be zero");
        }
        if header[6] != address.unit_id {
            bail!("Modbus TCP write response unit id does not match request");
        }
        let mbap_length = u16::from_be_bytes([header[4], header[5]]) as usize;
        if !(2..=MAX_PDU_BYTES + 1).contains(&mbap_length) {
            bail!("Modbus TCP write response has invalid MBAP length {mbap_length}");
        }
        let mut pdu = vec![0_u8; mbap_length - 1];
        timeout(self.request_timeout, stream.read_exact(&mut pdu))
            .await
            .context("Modbus TCP write response body timeout")??;
        parse_write_response_pdu(&pdu, address.offset, command)
    }

    fn prepare_point_write(
        &self,
        mapping: &TelemetryPointMapping,
        value: &TelemetryValue,
    ) -> Result<(ModbusAddress, ModbusWriteCommand)> {
        if mapping.protocol_connection_id != self.connection.connection_id {
            bail!(
                "point {} is not bound to connection {}",
                mapping.point_id,
                self.connection.connection_id
            );
        }
        if !mapping.access.is_writable() {
            bail!("point {} is not configured as writable", mapping.point_id);
        }
        let address = parse_address(
            &mapping.address.kind,
            &mapping.address.value,
            self.default_unit_id,
        )
        .with_context(|| format!("invalid point address for {}", mapping.point_id))?;
        validate_modbus_point_options(&mapping.address, mapping.value_type, mapping.access)
            .map_err(anyhow::Error::msg)?;
        let command = encode_write_command(address, &mapping.address, mapping.value_type, value)?;
        Ok((address, command))
    }

    fn prepare_write_batch(
        &self,
        writes: &[ProtocolPointWrite],
    ) -> Result<Option<(ModbusAddress, ModbusWriteCommand)>> {
        if writes.len() < 2 {
            return Ok(None);
        }
        let prepared = writes
            .iter()
            .map(|write| self.prepare_point_write(&write.mapping, &write.value))
            .collect::<Result<Vec<_>>>()?;
        let first = prepared[0].0;
        if prepared.iter().any(|(address, _)| {
            address.unit_id != first.unit_id || address.function != first.function
        }) {
            return Ok(None);
        }

        let mut expected_offset = first.offset;
        let mut values = Vec::new();
        match first.function {
            READ_COILS => {
                for (address, command) in prepared {
                    if address.offset != expected_offset || command.function != WRITE_SINGLE_COIL {
                        return Ok(None);
                    }
                    values.push(u16::from(command.values[0] != 0));
                    expected_offset = expected_offset
                        .checked_add(1)
                        .context("Modbus coil batch exceeds address range")?;
                }
                if values.len() > 1_968 {
                    return Ok(None);
                }
                Ok(Some((
                    first,
                    ModbusWriteCommand {
                        function: WRITE_MULTIPLE_COILS,
                        values,
                    },
                )))
            }
            READ_HOLDING_REGISTERS => {
                for (address, command) in prepared {
                    if address.offset != expected_offset
                        || !matches!(
                            command.function,
                            WRITE_SINGLE_REGISTER | WRITE_MULTIPLE_REGISTERS
                        )
                    {
                        return Ok(None);
                    }
                    expected_offset = expected_offset
                        .checked_add(command.values.len() as u16)
                        .context("Modbus register batch exceeds address range")?;
                    values.extend(command.values);
                }
                if values.len() > 123 {
                    return Ok(None);
                }
                Ok(Some((
                    first,
                    ModbusWriteCommand {
                        function: WRITE_MULTIPLE_REGISTERS,
                        values,
                    },
                )))
            }
            _ => Ok(None),
        }
    }
}

#[async_trait]
impl ProtocolAdapter for ModbusTcpAdapter {
    async fn read_telemetry(&mut self) -> Result<Vec<TelemetrySample>> {
        if self.connection.protocol != ProtocolType::ModbusTcp {
            bail!("Modbus TCP adapter requires a ModbusTcp protocol connection");
        }

        let mappings = self.mappings.clone();
        let mut planned_mappings = Vec::new();
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
            validate_modbus_point_options(&mapping.address, mapping.value_type, mapping.access)
                .map_err(anyhow::Error::msg)?;
            let quantity = modbus_quantity(
                is_bit_function(address.function),
                mapping.value_type,
                &mapping.address,
            )?;
            planned_mappings.push((mapping, address, quantity));
        }

        if planned_mappings.is_empty() {
            return Ok(Vec::new());
        }

        let windows = plan_read_windows(
            planned_mappings
                .iter()
                .enumerate()
                .map(|(mapping_index, (_, address, quantity))| ModbusReadPoint {
                    mapping_index,
                    station_id: address.unit_id,
                    function: address.function,
                    offset: address.offset,
                    quantity: *quantity,
                    is_bit: is_bit_function(address.function),
                })
                .collect(),
        )?;
        let mut stream = self.connect().await?;
        let mut samples = vec![None; planned_mappings.len()];
        for window in windows {
            let address = ModbusAddress {
                unit_id: window.station_id,
                function: window.function,
                offset: window.offset,
            };
            let payload = self.transact(&mut stream, address, window.quantity).await?;
            for point in &window.points {
                let (mapping, _, _) = &planned_mappings[point.mapping_index];
                let point_payload = extract_point_payload(&window, *point, &payload)?;
                let value = decode_modbus_value(
                    is_bit_function(window.function),
                    mapping.value_type,
                    &mapping.address,
                    &point_payload,
                )?;
                samples[point.mapping_index] = Some(TelemetrySample::new(
                    &mapping.device_id,
                    &mapping.point_id,
                    value,
                    DataQuality::Good,
                    Utc::now(),
                ));
            }
        }
        Ok(samples.into_iter().flatten().collect())
    }
}

#[async_trait]
impl ProtocolCommandAdapter for ModbusTcpAdapter {
    async fn write_point(
        &mut self,
        mapping: &TelemetryPointMapping,
        value: TelemetryValue,
    ) -> Result<ProtocolWriteResult> {
        if self.connection.protocol != ProtocolType::ModbusTcp {
            bail!("Modbus TCP adapter requires a ModbusTcp protocol connection");
        }
        let (address, command) = self.prepare_point_write(mapping, &value)?;
        let mut stream = self.connect().await?;
        self.transact_write(&mut stream, address, &command).await?;
        Ok(ProtocolWriteResult {
            point_id: mapping.point_id.clone(),
            value,
            verified: true,
            readback_value: None,
        })
    }

    async fn write_points(
        &mut self,
        writes: &[ProtocolPointWrite],
    ) -> Result<Vec<ProtocolWriteResult>> {
        if self.connection.protocol != ProtocolType::ModbusTcp {
            bail!("Modbus TCP adapter requires a ModbusTcp protocol connection");
        }
        let Some((address, command)) = self.prepare_write_batch(writes)? else {
            let mut results = Vec::with_capacity(writes.len());
            for write in writes {
                results.push(
                    self.write_point(&write.mapping, write.value.clone())
                        .await?,
                );
            }
            return Ok(results);
        };
        let mut stream = self.connect().await?;
        self.transact_write(&mut stream, address, &command).await?;
        Ok(writes
            .iter()
            .map(|write| ProtocolWriteResult {
                point_id: write.mapping.point_id.clone(),
                value: write.value.clone(),
                verified: true,
                readback_value: None,
            })
            .collect())
    }
}

#[derive(Clone, Copy, Debug)]
struct ModbusAddress {
    unit_id: u8,
    function: u8,
    offset: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ModbusWriteCommand {
    function: u8,
    values: Vec<u16>,
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
        "discrete_input" => (READ_DISCRETE_INPUTS, 10_001_u32),
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
    } else if is_bit_function(function) && raw_address > 0 {
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

fn build_write_request(
    transaction_id: u16,
    address: ModbusAddress,
    command: &ModbusWriteCommand,
) -> Vec<u8> {
    let mut pdu = vec![command.function];
    pdu.extend(address.offset.to_be_bytes());
    match command.function {
        WRITE_SINGLE_COIL | WRITE_SINGLE_REGISTER => {
            pdu.extend(command.values[0].to_be_bytes());
        }
        WRITE_MULTIPLE_COILS => {
            let quantity = u16::try_from(command.values.len()).unwrap_or(u16::MAX);
            let mut packed = vec![0_u8; quantity.div_ceil(8) as usize];
            for (index, value) in command.values.iter().enumerate() {
                if *value != 0 {
                    packed[index / 8] |= 1 << (index % 8);
                }
            }
            pdu.extend(quantity.to_be_bytes());
            pdu.push(packed.len() as u8);
            pdu.extend(packed);
        }
        WRITE_MULTIPLE_REGISTERS => {
            let quantity = u16::try_from(command.values.len()).unwrap_or(u16::MAX);
            pdu.extend(quantity.to_be_bytes());
            pdu.push((command.values.len() * 2) as u8);
            for value in &command.values {
                pdu.extend(value.to_be_bytes());
            }
        }
        _ => unreachable!("write command function is validated when encoded"),
    }

    let mut frame = Vec::with_capacity(7 + pdu.len());
    frame.extend(transaction_id.to_be_bytes());
    frame.extend(0_u16.to_be_bytes());
    frame.extend(((pdu.len() + 1) as u16).to_be_bytes());
    frame.push(address.unit_id);
    frame.extend(pdu);
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
    let expected = if is_bit_function(function) {
        quantity.div_ceil(8) as usize
    } else {
        quantity as usize * 2
    };
    if byte_count != expected || pdu.len() != byte_count + 2 {
        bail!("Modbus TCP response byte count does not match request");
    }
    Ok(pdu[2..].to_vec())
}

fn parse_write_response_pdu(
    pdu: &[u8],
    expected_offset: u16,
    command: &ModbusWriteCommand,
) -> Result<()> {
    if pdu.len() < 2 {
        bail!("Modbus TCP write response PDU is too short");
    }
    if pdu[0] == command.function | 0x80 {
        bail!("Modbus TCP exception code {}", pdu[1]);
    }
    if pdu.len() != 5 || pdu[0] != command.function {
        bail!("Modbus TCP write response does not match request");
    }
    if u16::from_be_bytes([pdu[1], pdu[2]]) != expected_offset {
        bail!("Modbus TCP write response address does not match request");
    }
    let echoed = u16::from_be_bytes([pdu[3], pdu[4]]);
    let expected = match command.function {
        WRITE_SINGLE_COIL | WRITE_SINGLE_REGISTER => command.values[0],
        WRITE_MULTIPLE_COILS | WRITE_MULTIPLE_REGISTERS => command.values.len() as u16,
        _ => unreachable!("write command function is validated when encoded"),
    };
    if echoed != expected {
        bail!("Modbus TCP write response value or quantity does not match request");
    }
    Ok(())
}

fn encode_write_command(
    address: ModbusAddress,
    point_address: &PointAddress,
    value_type: edge_core::TelemetryType,
    value: &TelemetryValue,
) -> Result<ModbusWriteCommand> {
    match address.function {
        READ_COILS => match value {
            TelemetryValue::Boolean(value) => Ok(ModbusWriteCommand {
                function: WRITE_SINGLE_COIL,
                values: vec![if *value { 0xFF00 } else { 0x0000 }],
            }),
            _ => bail!("Modbus coil writes require a boolean value"),
        },
        READ_HOLDING_REGISTERS => {
            let values = encode_modbus_register_values(value_type, point_address, value)?;
            Ok(ModbusWriteCommand {
                function: if values.len() == 1 {
                    WRITE_SINGLE_REGISTER
                } else {
                    WRITE_MULTIPLE_REGISTERS
                },
                values,
            })
        }
        READ_DISCRETE_INPUTS | READ_INPUT_REGISTERS => {
            bail!("Modbus input and discrete input areas are read-only")
        }
        _ => bail!("unsupported Modbus write target"),
    }
}

#[derive(Clone, Debug)]
pub struct ModbusTcpSimulatorOptions {
    pub bind: SocketAddr,
    pub unit_id: u8,
    pub holding_registers: BTreeMap<u16, u16>,
    pub input_registers: BTreeMap<u16, u16>,
    pub coils: BTreeMap<u16, bool>,
    pub discrete_inputs: BTreeMap<u16, bool>,
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
            discrete_inputs: BTreeMap::new(),
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
    options: Arc<Mutex<ModbusTcpSimulatorOptions>>,
    metrics: ModbusTcpSimulatorMetrics,
}

#[derive(Clone, Debug, Default)]
pub struct ModbusTcpSimulatorMetrics {
    requests_total: Arc<AtomicU64>,
}

impl ModbusTcpSimulatorMetrics {
    pub fn requests_total(&self) -> u64 {
        self.requests_total.load(Ordering::Relaxed)
    }
}

impl ModbusTcpSimulator {
    pub async fn bind(options: ModbusTcpSimulatorOptions) -> Result<Self> {
        let listener = TcpListener::bind(options.bind).await?;
        Ok(Self {
            listener,
            options: Arc::new(Mutex::new(options)),
            metrics: ModbusTcpSimulatorMetrics::default(),
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }

    pub fn metrics(&self) -> ModbusTcpSimulatorMetrics {
        self.metrics.clone()
    }

    pub async fn run(self) -> Result<()> {
        loop {
            let (stream, _) = self.listener.accept().await?;
            let options = self.options.clone();
            let metrics = self.metrics.clone();
            tokio::spawn(async move {
                if let Err(error) = serve_connection(stream, options, metrics).await {
                    tracing::debug!(%error, "Modbus TCP simulator connection closed");
                }
            });
        }
    }
}

async fn serve_connection(
    mut stream: TcpStream,
    options: Arc<Mutex<ModbusTcpSimulatorOptions>>,
    metrics: ModbusTcpSimulatorMetrics,
) -> Result<()> {
    loop {
        let mut header = [0_u8; 7];
        match stream.read_exact(&mut header).await {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error.into()),
        }
        let transaction = [header[0], header[1]];
        let protocol_id = u16::from_be_bytes([header[2], header[3]]);
        let length = u16::from_be_bytes([header[4], header[5]]) as usize;
        let unit = header[6];
        if !(2..=MAX_PDU_BYTES + 1).contains(&length) {
            bail!("invalid Modbus TCP request length {length}");
        }
        let mut request_pdu = vec![0_u8; length - 1];
        stream.read_exact(&mut request_pdu).await?;
        metrics.requests_total.fetch_add(1, Ordering::Relaxed);
        let function = request_pdu[0];
        let mut options = options.lock().await;
        let pdu = if protocol_id != 0 || unit != options.unit_id {
            vec![function | 0x80, 0x03]
        } else {
            simulator_response_pdu(&mut options, &request_pdu)
        };
        drop(options);
        let mut response = Vec::with_capacity(7 + pdu.len());
        response.extend(transaction);
        response.extend(0_u16.to_be_bytes());
        response.extend(((pdu.len() + 1) as u16).to_be_bytes());
        response.push(unit);
        response.extend(pdu);
        stream.write_all(&response).await?;
    }
}

fn simulator_response_pdu(options: &mut ModbusTcpSimulatorOptions, request: &[u8]) -> Vec<u8> {
    let function = request[0];
    if request.len() < 5 {
        return simulator_exception(function, 0x03);
    }
    let offset = u16::from_be_bytes([request[1], request[2]]);
    let quantity = u16::from_be_bytes([request[3], request[4]]);
    match function {
        READ_HOLDING_REGISTERS | READ_INPUT_REGISTERS => {
            if request.len() != 5 || quantity == 0 || quantity > 125 {
                return simulator_exception(function, 0x03);
            }
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
        READ_COILS | READ_DISCRETE_INPUTS => {
            if request.len() != 5 || quantity == 0 || quantity > 2_000 {
                return simulator_exception(function, 0x03);
            }
            let byte_count = quantity.div_ceil(8) as usize;
            let mut bytes = vec![0_u8; byte_count];
            let now = unix_time();
            for index in 0..quantity {
                let coil = offset.saturating_add(index);
                let value = if function == READ_COILS {
                    options
                        .toggling_coils
                        .get(&coil)
                        .map(|period| {
                            let period_ms = period.as_millis().max(1);
                            (now.as_millis() / period_ms).is_multiple_of(2)
                        })
                        .unwrap_or_else(|| options.coils.get(&coil).copied().unwrap_or(false))
                } else {
                    options.discrete_inputs.get(&coil).copied().unwrap_or(false)
                };
                if value {
                    bytes[index as usize / 8] |= 1 << (index % 8);
                }
            }
            let mut pdu = vec![function, byte_count as u8];
            pdu.extend(bytes);
            pdu
        }
        WRITE_SINGLE_COIL => {
            if request.len() != 5 || !matches!(quantity, 0x0000 | 0xFF00) {
                return simulator_exception(function, 0x03);
            }
            options.coils.insert(offset, quantity == 0xFF00);
            request.to_vec()
        }
        WRITE_SINGLE_REGISTER => {
            if request.len() != 5 {
                return simulator_exception(function, 0x03);
            }
            options.holding_registers.insert(offset, quantity);
            request.to_vec()
        }
        WRITE_MULTIPLE_COILS => {
            if quantity == 0 || quantity > 1_968 || request.len() < 6 {
                return simulator_exception(function, 0x03);
            }
            let byte_count = request[5] as usize;
            let expected = quantity.div_ceil(8) as usize;
            if byte_count != expected || request.len() != 6 + byte_count {
                return simulator_exception(function, 0x03);
            }
            for index in 0..quantity {
                let Some(coil) = offset.checked_add(index) else {
                    return simulator_exception(function, 0x02);
                };
                let value = request[6 + index as usize / 8] & (1 << (index % 8)) != 0;
                options.coils.insert(coil, value);
            }
            vec![function, request[1], request[2], request[3], request[4]]
        }
        WRITE_MULTIPLE_REGISTERS => {
            if quantity == 0 || quantity > 123 || request.len() < 6 {
                return simulator_exception(function, 0x03);
            }
            let byte_count = request[5] as usize;
            if byte_count != quantity as usize * 2 || request.len() != 6 + byte_count {
                return simulator_exception(function, 0x03);
            }
            for index in 0..quantity {
                let Some(register) = offset.checked_add(index) else {
                    return simulator_exception(function, 0x02);
                };
                let cursor = 6 + index as usize * 2;
                let value = u16::from_be_bytes([request[cursor], request[cursor + 1]]);
                options.holding_registers.insert(register, value);
            }
            vec![function, request[1], request[2], request[3], request[4]]
        }
        _ => simulator_exception(function, 0x01),
    }
}

fn simulator_exception(function: u8, code: u8) -> Vec<u8> {
    vec![function | 0x80, code]
}

fn is_bit_function(function: u8) -> bool {
    matches!(function, READ_COILS | READ_DISCRETE_INPUTS)
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
