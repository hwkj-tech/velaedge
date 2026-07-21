use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use edge_core::{
    decode_custom_serial_hex, validate_custom_serial_point_spec, CustomSerialChecksum,
    CustomSerialPointSpec, CustomSerialValueEncoding, DataQuality, ProtocolConnection,
    ProtocolType, TelemetryPointMapping, TelemetrySample, TelemetryType, TelemetryValue,
};

use crate::{ProtocolAdapter, SerialBus};

const MAX_RESPONSE_BYTES: usize = 4096;

pub struct CustomSerialAdapter<B> {
    connection: ProtocolConnection,
    mappings: Vec<TelemetryPointMapping>,
    bus: B,
}

impl<B> CustomSerialAdapter<B> {
    pub fn new(
        connection: ProtocolConnection,
        mappings: Vec<TelemetryPointMapping>,
        bus: B,
    ) -> Self {
        Self {
            connection,
            mappings,
            bus,
        }
    }
}

#[async_trait]
impl<B> ProtocolAdapter for CustomSerialAdapter<B>
where
    B: SerialBus,
{
    async fn read_telemetry(&mut self) -> Result<Vec<TelemetrySample>> {
        if self.connection.protocol != ProtocolType::CustomSerial {
            bail!("custom serial adapter requires a CustomSerial protocol connection");
        }

        let mut samples = Vec::new();
        for mapping in &self.mappings {
            if mapping.protocol_connection_id != self.connection.connection_id {
                continue;
            }
            let spec = parse_point_spec(mapping)
                .with_context(|| format!("invalid point address for {}", mapping.point_id))?;
            let mut request = decode_custom_serial_hex(&spec.request_hex)
                .map_err(anyhow::Error::msg)
                .context("invalid request frame")?;
            append_custom_serial_checksum(&mut request, spec.request_checksum);

            let response = self.bus.transact(&request).await?;
            let payload = verified_response_payload(&response, &spec)?;
            let value = decode_value(payload, &spec, mapping.value_type)
                .with_context(|| format!("failed to decode point {}", mapping.point_id))?;
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

pub fn append_custom_serial_checksum(frame: &mut Vec<u8>, checksum: CustomSerialChecksum) {
    match checksum {
        CustomSerialChecksum::None => {}
        CustomSerialChecksum::Sum8 => {
            frame.push(frame.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte)));
        }
        CustomSerialChecksum::Xor8 => {
            frame.push(frame.iter().fold(0_u8, |value, byte| value ^ byte));
        }
        CustomSerialChecksum::ModbusCrc16 => {
            frame.extend(modbus_crc16(frame).to_le_bytes());
        }
    }
}

fn parse_point_spec(mapping: &TelemetryPointMapping) -> Result<CustomSerialPointSpec> {
    if mapping.address.kind != "custom_serial_frame" {
        bail!("custom serial adapter supports custom_serial_frame addresses");
    }
    let spec = serde_json::from_str::<CustomSerialPointSpec>(&mapping.address.value)
        .context("address value must be a custom serial frame JSON object")?;
    validate_custom_serial_point_spec(&spec).map_err(anyhow::Error::msg)?;
    Ok(spec)
}

fn verified_response_payload<'a>(
    response: &'a [u8],
    spec: &CustomSerialPointSpec,
) -> Result<&'a [u8]> {
    if response.is_empty() {
        bail!("custom serial response is empty");
    }
    if response.len() > MAX_RESPONSE_BYTES {
        bail!("custom serial response exceeds the 4096-byte limit");
    }

    let payload = match spec.response_checksum {
        CustomSerialChecksum::None => response,
        CustomSerialChecksum::Sum8 => {
            if response.len() < 2 {
                bail!("custom serial response is too short for sum8");
            }
            let payload = &response[..response.len() - 1];
            let actual = payload
                .iter()
                .fold(0_u8, |sum, byte| sum.wrapping_add(*byte));
            if actual != response[response.len() - 1] {
                bail!("custom serial sum8 mismatch");
            }
            payload
        }
        CustomSerialChecksum::Xor8 => {
            if response.len() < 2 {
                bail!("custom serial response is too short for xor8");
            }
            let payload = &response[..response.len() - 1];
            let actual = payload.iter().fold(0_u8, |value, byte| value ^ byte);
            if actual != response[response.len() - 1] {
                bail!("custom serial xor8 mismatch");
            }
            payload
        }
        CustomSerialChecksum::ModbusCrc16 => {
            if response.len() < 3 {
                bail!("custom serial response is too short for Modbus CRC16");
            }
            let payload = &response[..response.len() - 2];
            let expected =
                u16::from_le_bytes([response[response.len() - 2], response[response.len() - 1]]);
            if modbus_crc16(payload) != expected {
                bail!("custom serial Modbus CRC16 mismatch");
            }
            payload
        }
    };

    if let Some(prefix_hex) = &spec.response_prefix_hex {
        let prefix = decode_custom_serial_hex(prefix_hex).map_err(anyhow::Error::msg)?;
        if !payload.starts_with(&prefix) {
            bail!("custom serial response prefix does not match");
        }
    }
    Ok(payload)
}

fn decode_value(
    payload: &[u8],
    spec: &CustomSerialPointSpec,
    value_type: TelemetryType,
) -> Result<TelemetryValue> {
    let width = spec.value_width().map_err(anyhow::Error::msg)?;
    let end = spec
        .value_offset
        .checked_add(width)
        .context("custom serial value range overflows")?;
    let bytes = payload
        .get(spec.value_offset..end)
        .context("custom serial response does not contain the configured value range")?;

    match spec.value_encoding {
        CustomSerialValueEncoding::BoolU8 => {
            if value_type != TelemetryType::Boolean {
                bail!("bool_u8 encoding requires a Boolean telemetry point");
            }
            Ok(TelemetryValue::Boolean(bytes[0] != 0))
        }
        CustomSerialValueEncoding::Utf8 => {
            if value_type != TelemetryType::Text {
                bail!("utf8 encoding requires a Text telemetry point");
            }
            let text = std::str::from_utf8(bytes)
                .context("custom serial text value is not valid UTF-8")?
                .trim_end_matches('\0')
                .to_string();
            Ok(TelemetryValue::Text(text))
        }
        encoding => {
            let raw = decode_number(bytes, encoding)?;
            let value = raw * spec.scale + spec.offset;
            if !value.is_finite() {
                bail!("custom serial numeric result is not finite");
            }
            match value_type {
                TelemetryType::Float => Ok(TelemetryValue::Float(value)),
                TelemetryType::Integer => {
                    if value.fract() != 0.0 || value < i64::MIN as f64 || value > i64::MAX as f64 {
                        bail!("custom serial value cannot be represented as an integer");
                    }
                    Ok(TelemetryValue::Integer(value as i64))
                }
                TelemetryType::Boolean | TelemetryType::Text => {
                    bail!("numeric encoding requires a Float or Integer telemetry point")
                }
            }
        }
    }
}

fn decode_number(bytes: &[u8], encoding: CustomSerialValueEncoding) -> Result<f64> {
    let number = match encoding {
        CustomSerialValueEncoding::U8 => bytes[0] as f64,
        CustomSerialValueEncoding::I8 => (bytes[0] as i8) as f64,
        CustomSerialValueEncoding::U16Be => u16::from_be_bytes([bytes[0], bytes[1]]) as f64,
        CustomSerialValueEncoding::U16Le => u16::from_le_bytes([bytes[0], bytes[1]]) as f64,
        CustomSerialValueEncoding::I16Be => i16::from_be_bytes([bytes[0], bytes[1]]) as f64,
        CustomSerialValueEncoding::I16Le => i16::from_le_bytes([bytes[0], bytes[1]]) as f64,
        CustomSerialValueEncoding::U32Be => {
            u32::from_be_bytes(bytes.try_into().expect("validated u32 width")) as f64
        }
        CustomSerialValueEncoding::U32Le => {
            u32::from_le_bytes(bytes.try_into().expect("validated u32 width")) as f64
        }
        CustomSerialValueEncoding::I32Be => {
            i32::from_be_bytes(bytes.try_into().expect("validated i32 width")) as f64
        }
        CustomSerialValueEncoding::I32Le => {
            i32::from_le_bytes(bytes.try_into().expect("validated i32 width")) as f64
        }
        CustomSerialValueEncoding::F32Be => {
            f32::from_be_bytes(bytes.try_into().expect("validated f32 width")) as f64
        }
        CustomSerialValueEncoding::F32Le => {
            f32::from_le_bytes(bytes.try_into().expect("validated f32 width")) as f64
        }
        CustomSerialValueEncoding::F64Be => {
            f64::from_be_bytes(bytes.try_into().expect("validated f64 width"))
        }
        CustomSerialValueEncoding::F64Le => {
            f64::from_le_bytes(bytes.try_into().expect("validated f64 width"))
        }
        CustomSerialValueEncoding::BoolU8 | CustomSerialValueEncoding::Utf8 => {
            bail!("encoding is not numeric")
        }
    };
    Ok(number)
}

fn modbus_crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0xFFFF_u16;
    for byte in bytes {
        crc ^= *byte as u16;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}
