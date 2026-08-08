use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use edge_core::{
    validate_modbus_point_options, DataQuality, PointAddress, ProtocolConnection, ProtocolType,
    TelemetryPointMapping, TelemetrySample, TelemetryValue,
};

use crate::{
    modbus_batch::{extract_point_payload, plan_read_windows, ModbusReadPoint},
    modbus_codec::{decode_modbus_value, encode_modbus_register_values, modbus_quantity},
    ProtocolAdapter, ProtocolCommandAdapter, ProtocolPointWrite, ProtocolWriteResult, SerialBus,
};

const READ_COILS: u8 = 0x01;
const READ_DISCRETE_INPUTS: u8 = 0x02;
const READ_HOLDING_REGISTERS: u8 = 0x03;
const READ_INPUT_REGISTERS: u8 = 0x04;
const WRITE_SINGLE_COIL: u8 = 0x05;
const WRITE_SINGLE_REGISTER: u8 = 0x06;
const WRITE_MULTIPLE_COILS: u8 = 0x0F;
const WRITE_MULTIPLE_REGISTERS: u8 = 0x10;

pub struct ModbusRtuAdapter<B> {
    connection: ProtocolConnection,
    mappings: Vec<TelemetryPointMapping>,
    bus: B,
    default_slave_id: u8,
}

impl<B> ModbusRtuAdapter<B> {
    pub fn new(
        connection: ProtocolConnection,
        mappings: Vec<TelemetryPointMapping>,
        bus: B,
    ) -> Self {
        Self {
            connection,
            mappings,
            bus,
            default_slave_id: 1,
        }
    }

    pub fn with_default_slave_id(mut self, slave_id: u8) -> Self {
        self.default_slave_id = slave_id;
        self
    }

    pub(crate) fn batchable_write_prefix(&self, writes: &[ProtocolPointWrite]) -> usize {
        (2..=writes.len())
            .rev()
            .find(|length| matches!(self.prepare_write_batch(&writes[..*length]), Ok(Some(_))))
            .unwrap_or(1.min(writes.len()))
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
            mapping.address.kind.as_str(),
            mapping.address.value.as_str(),
            self.default_slave_id,
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
            address.slave_id != first.slave_id || address.function != first.function
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
impl<B> ProtocolAdapter for ModbusRtuAdapter<B>
where
    B: SerialBus,
{
    async fn read_telemetry(&mut self) -> Result<Vec<TelemetrySample>> {
        if self.connection.protocol != ProtocolType::ModbusRtu {
            bail!("Modbus RTU adapter requires a ModbusRtu protocol connection");
        }

        let mut planned_mappings = Vec::new();
        for mapping in &self.mappings {
            if mapping.protocol_connection_id != self.connection.connection_id {
                continue;
            }
            let address = parse_address(
                mapping.address.kind.as_str(),
                mapping.address.value.as_str(),
                self.default_slave_id,
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

        let windows = plan_read_windows(
            planned_mappings
                .iter()
                .enumerate()
                .map(|(mapping_index, (_, address, quantity))| ModbusReadPoint {
                    mapping_index,
                    station_id: address.slave_id,
                    function: address.function,
                    offset: address.offset,
                    quantity: *quantity,
                    is_bit: is_bit_function(address.function),
                })
                .collect(),
        )?;
        let mut samples = vec![None; planned_mappings.len()];
        for window in windows {
            let request = build_read_request(
                window.station_id,
                window.function,
                window.offset,
                window.quantity,
            );
            let response = self.bus.transact(&request).await?;
            let payload = parse_read_response(
                &response,
                window.station_id,
                window.function,
                window.quantity,
            )?;
            for point in &window.points {
                let (mapping, _, _) = planned_mappings[point.mapping_index];
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
impl<B> ProtocolCommandAdapter for ModbusRtuAdapter<B>
where
    B: SerialBus,
{
    async fn write_point(
        &mut self,
        mapping: &TelemetryPointMapping,
        value: TelemetryValue,
    ) -> Result<ProtocolWriteResult> {
        if self.connection.protocol != ProtocolType::ModbusRtu {
            bail!("Modbus RTU adapter requires a ModbusRtu protocol connection");
        }
        let (address, command) = self.prepare_point_write(mapping, &value)?;
        let request = build_write_request(address.slave_id, address.offset, &command);
        let response = self.bus.transact(&request).await?;
        parse_write_response(&response, address.slave_id, address.offset, &command)?;
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
        if self.connection.protocol != ProtocolType::ModbusRtu {
            bail!("Modbus RTU adapter requires a ModbusRtu protocol connection");
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
        let request = build_write_request(address.slave_id, address.offset, &command);
        let response = self.bus.transact(&request).await?;
        parse_write_response(&response, address.slave_id, address.offset, &command)?;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ModbusAddress {
    slave_id: u8,
    function: u8,
    offset: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ModbusWriteCommand {
    function: u8,
    values: Vec<u16>,
}

pub fn append_modbus_rtu_crc(frame: &mut Vec<u8>) {
    let crc = modbus_rtu_crc(frame);
    frame.extend(crc.to_le_bytes());
}

pub(crate) fn build_read_holding_registers_request(
    slave_id: u8,
    register: u16,
    count: u16,
) -> Vec<u8> {
    build_read_request(slave_id, READ_HOLDING_REGISTERS, register, count)
}

pub(crate) fn parse_read_holding_registers_response(
    response: &[u8],
    expected_slave_id: u8,
    expected_registers: u16,
) -> Result<Vec<u16>> {
    let payload = parse_read_response(
        response,
        expected_slave_id,
        READ_HOLDING_REGISTERS,
        expected_registers,
    )?;
    Ok(payload
        .chunks_exact(2)
        .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
        .collect())
}

fn build_read_request(slave_id: u8, function: u8, offset: u16, quantity: u16) -> Vec<u8> {
    let mut frame = vec![slave_id, function];
    frame.extend(offset.to_be_bytes());
    frame.extend(quantity.to_be_bytes());
    append_modbus_rtu_crc(&mut frame);
    frame
}

fn build_write_request(slave_id: u8, offset: u16, command: &ModbusWriteCommand) -> Vec<u8> {
    let mut frame = vec![slave_id, command.function];
    frame.extend(offset.to_be_bytes());
    match command.function {
        WRITE_SINGLE_COIL | WRITE_SINGLE_REGISTER => {
            frame.extend(command.values[0].to_be_bytes());
        }
        WRITE_MULTIPLE_COILS => {
            let quantity = u16::try_from(command.values.len()).unwrap_or(u16::MAX);
            let mut packed = vec![0_u8; quantity.div_ceil(8) as usize];
            for (index, value) in command.values.iter().enumerate() {
                if *value != 0 {
                    packed[index / 8] |= 1 << (index % 8);
                }
            }
            frame.extend(quantity.to_be_bytes());
            frame.push(packed.len() as u8);
            frame.extend(packed);
        }
        WRITE_MULTIPLE_REGISTERS => {
            let quantity = u16::try_from(command.values.len()).unwrap_or(u16::MAX);
            frame.extend(quantity.to_be_bytes());
            frame.push((command.values.len() * 2) as u8);
            for value in &command.values {
                frame.extend(value.to_be_bytes());
            }
        }
        _ => unreachable!("write command function is validated when encoded"),
    }
    append_modbus_rtu_crc(&mut frame);
    frame
}

fn parse_write_response(
    response: &[u8],
    expected_slave_id: u8,
    expected_offset: u16,
    command: &ModbusWriteCommand,
) -> Result<()> {
    if response.len() != 8 {
        bail!("Modbus RTU write response must contain 8 bytes");
    }
    verify_modbus_rtu_crc(response)?;
    if response[0] != expected_slave_id {
        bail!("Modbus RTU write response slave id does not match request");
    }
    if response[1] == command.function | 0x80 {
        bail!("Modbus RTU exception code {}", response[2]);
    }
    if response[1] != command.function {
        bail!("Modbus RTU write response function does not match request");
    }
    if u16::from_be_bytes([response[2], response[3]]) != expected_offset {
        bail!("Modbus RTU write response address does not match request");
    }
    let echoed = u16::from_be_bytes([response[4], response[5]]);
    let expected = match command.function {
        WRITE_SINGLE_COIL | WRITE_SINGLE_REGISTER => command.values[0],
        WRITE_MULTIPLE_COILS | WRITE_MULTIPLE_REGISTERS => command.values.len() as u16,
        _ => unreachable!("write command function is validated when encoded"),
    };
    if echoed != expected {
        bail!("Modbus RTU write response value or quantity does not match request");
    }
    Ok(())
}

fn parse_read_response(
    response: &[u8],
    expected_slave_id: u8,
    expected_function: u8,
    expected_quantity: u16,
) -> Result<Vec<u8>> {
    if response.len() < 5 {
        bail!("Modbus RTU response is too short");
    }
    verify_modbus_rtu_crc(response)?;
    if response[0] != expected_slave_id {
        bail!("Modbus RTU response slave id does not match request");
    }
    if response[1] == (expected_function | 0x80) {
        bail!("Modbus RTU exception code {}", response[2]);
    }
    if response[1] != expected_function {
        bail!("Modbus RTU response function does not match request");
    }
    let byte_count = response[2] as usize;
    let expected_byte_count = if is_bit_function(expected_function) {
        expected_quantity.div_ceil(8) as usize
    } else {
        expected_quantity as usize * 2
    };
    if byte_count != expected_byte_count {
        bail!("Modbus RTU response byte count does not match request");
    }
    if response.len() != byte_count + 5 {
        bail!("Modbus RTU response length does not match byte count");
    }

    Ok(response[3..3 + byte_count].to_vec())
}

fn parse_address(kind: &str, value: &str, default_slave_id: u8) -> Result<ModbusAddress> {
    let (function, reference_base) = match kind {
        "holding_register" => (READ_HOLDING_REGISTERS, 40_001_u32),
        "input_register" => (READ_INPUT_REGISTERS, 30_001_u32),
        "coil" => (READ_COILS, 1_u32),
        "discrete_input" => (READ_DISCRETE_INPUTS, 10_001_u32),
        _ => bail!("Modbus RTU address kind is not supported: {kind}"),
    };
    let (slave_id, address_text) = match value.split_once(':') {
        Some((slave, address)) => {
            let slave_id = slave
                .parse::<u8>()
                .with_context(|| format!("invalid Modbus slave id: {slave}"))?;
            (slave_id, address)
        }
        None => (default_slave_id, value),
    };
    if !(1..=247).contains(&slave_id) {
        bail!("Modbus slave id must be between 1 and 247");
    }

    let raw_address = address_text
        .parse::<u32>()
        .with_context(|| format!("invalid Modbus address: {address_text}"))?;
    let offset = if raw_address >= reference_base {
        raw_address - reference_base
    } else if is_bit_function(function) && raw_address > 0 {
        raw_address - 1
    } else {
        raw_address
    };
    let offset = u16::try_from(offset).context("Modbus address exceeds u16 range")?;

    Ok(ModbusAddress {
        slave_id,
        function,
        offset,
    })
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

fn is_bit_function(function: u8) -> bool {
    matches!(function, READ_COILS | READ_DISCRETE_INPUTS)
}

fn verify_modbus_rtu_crc(frame: &[u8]) -> Result<()> {
    if frame.len() < 3 {
        bail!("Modbus RTU frame is too short for CRC");
    }
    let payload_len = frame.len() - 2;
    let expected = u16::from_le_bytes([frame[payload_len], frame[payload_len + 1]]);
    let actual = modbus_rtu_crc(&frame[..payload_len]);
    if expected != actual {
        bail!("Modbus RTU CRC mismatch");
    }
    Ok(())
}

fn modbus_rtu_crc(bytes: &[u8]) -> u16 {
    let mut crc = 0xFFFF_u16;
    for byte in bytes {
        crc ^= *byte as u16;
        for _ in 0..8 {
            if crc & 0x0001 != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}
