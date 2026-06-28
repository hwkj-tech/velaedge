use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use edge_core::{
    DataQuality, ProtocolConnection, ProtocolType, TelemetryPointMapping, TelemetrySample,
    TelemetryType, TelemetryValue,
};

use crate::{ProtocolAdapter, SerialBus};

const READ_HOLDING_REGISTERS: u8 = 0x03;

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

        let mut samples = Vec::new();
        for mapping in &self.mappings {
            if mapping.protocol_connection_id != self.connection.connection_id {
                continue;
            }
            let address = parse_holding_register_address(
                mapping.address.kind.as_str(),
                mapping.address.value.as_str(),
                self.default_slave_id,
            )
            .with_context(|| format!("invalid point address for {}", mapping.point_id))?;
            let register_count = register_count_for(mapping.value_type);
            let request = build_read_holding_registers_request(
                address.slave_id,
                address.register,
                register_count,
            );
            let response = self.bus.transact(&request).await?;
            let registers =
                parse_read_holding_registers_response(&response, address.slave_id, register_count)?;
            let value = decode_register_value(mapping.value_type, &registers)?;
            samples.push(TelemetrySample::new(
                &mapping.device_id,
                &mapping.point_id,
                value,
                DataQuality::Good,
                Utc::now(),
            ));
        }

        Ok(samples)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ModbusRegisterAddress {
    slave_id: u8,
    register: u16,
}

pub fn append_modbus_rtu_crc(frame: &mut Vec<u8>) {
    let crc = modbus_rtu_crc(frame);
    frame.extend(crc.to_le_bytes());
}

fn build_read_holding_registers_request(slave_id: u8, register: u16, count: u16) -> Vec<u8> {
    let mut frame = vec![slave_id, READ_HOLDING_REGISTERS];
    frame.extend(register.to_be_bytes());
    frame.extend(count.to_be_bytes());
    append_modbus_rtu_crc(&mut frame);
    frame
}

fn parse_read_holding_registers_response(
    response: &[u8],
    expected_slave_id: u8,
    expected_registers: u16,
) -> Result<Vec<u16>> {
    if response.len() < 5 {
        bail!("Modbus RTU response is too short");
    }
    verify_modbus_rtu_crc(response)?;
    if response[0] != expected_slave_id {
        bail!("Modbus RTU response slave id does not match request");
    }
    if response[1] == (READ_HOLDING_REGISTERS | 0x80) {
        bail!("Modbus RTU exception code {}", response[2]);
    }
    if response[1] != READ_HOLDING_REGISTERS {
        bail!("Modbus RTU response function does not match request");
    }
    let byte_count = response[2] as usize;
    if byte_count != expected_registers as usize * 2 {
        bail!("Modbus RTU response byte count does not match request");
    }
    if response.len() != byte_count + 5 {
        bail!("Modbus RTU response length does not match byte count");
    }

    let mut registers = Vec::with_capacity(expected_registers as usize);
    for chunk in response[3..3 + byte_count].chunks_exact(2) {
        registers.push(u16::from_be_bytes([chunk[0], chunk[1]]));
    }
    Ok(registers)
}

fn parse_holding_register_address(
    kind: &str,
    value: &str,
    default_slave_id: u8,
) -> Result<ModbusRegisterAddress> {
    if kind != "holding_register" {
        bail!("Modbus RTU adapter supports holding_register addresses");
    }
    let (slave_id, register_text) = match value.split_once(':') {
        Some((slave, register)) => {
            let slave_id = slave
                .parse::<u8>()
                .with_context(|| format!("invalid Modbus slave id: {slave}"))?;
            (slave_id, register)
        }
        None => (default_slave_id, value),
    };
    if slave_id == 0 {
        bail!("Modbus slave id must be greater than zero");
    }

    let raw_register = register_text
        .parse::<u32>()
        .with_context(|| format!("invalid Modbus register: {register_text}"))?;
    let register = if raw_register >= 40001 {
        raw_register - 40001
    } else {
        raw_register
    };
    let register = u16::try_from(register).context("Modbus register exceeds u16 range")?;

    Ok(ModbusRegisterAddress { slave_id, register })
}

fn register_count_for(value_type: TelemetryType) -> u16 {
    match value_type {
        TelemetryType::Float => 2,
        TelemetryType::Integer | TelemetryType::Boolean | TelemetryType::Text => 1,
    }
}

fn decode_register_value(value_type: TelemetryType, registers: &[u16]) -> Result<TelemetryValue> {
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
