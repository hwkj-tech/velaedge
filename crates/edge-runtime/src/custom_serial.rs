use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use edge_core::{
    decode_custom_serial_hex, validate_custom_serial_point_spec, CustomSerialChecksum,
    CustomSerialFrameEncoding, CustomSerialPointSpec, CustomSerialValueEncoding, DataQuality,
    ProtocolConnection, ProtocolType, TelemetryPointMapping, TelemetrySample, TelemetryType,
    TelemetryValue,
};

use crate::{ProtocolAdapter, SerialBus};

const MAX_RESPONSE_BYTES: usize = 4096;
const MAX_FRAMED_RESPONSE_BYTES: usize = 8192;

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
            let request = encode_custom_serial_frame(&request, spec.frame_encoding)?;

            let response = self.bus.transact(&request).await?;
            let response = decode_custom_serial_frame(&response, spec.frame_encoding)?;
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
        CustomSerialChecksum::Crc16CcittFalse => {
            frame.extend(crc16_ccitt_false(frame).to_be_bytes());
        }
    }
}

pub fn encode_custom_serial_frame(
    payload: &[u8],
    encoding: CustomSerialFrameEncoding,
) -> Result<Vec<u8>> {
    match encoding {
        CustomSerialFrameEncoding::Raw => Ok(payload.to_vec()),
        CustomSerialFrameEncoding::Slip => Ok(encode_slip(payload)),
        CustomSerialFrameEncoding::Cobs => Ok(encode_cobs(payload)),
    }
}

pub fn decode_custom_serial_frame(
    frame: &[u8],
    encoding: CustomSerialFrameEncoding,
) -> Result<Vec<u8>> {
    if frame.len() > MAX_FRAMED_RESPONSE_BYTES {
        bail!("custom serial framed response exceeds the 8192-byte limit");
    }
    let payload = match encoding {
        CustomSerialFrameEncoding::Raw => frame.to_vec(),
        CustomSerialFrameEncoding::Slip => decode_slip(frame)?,
        CustomSerialFrameEncoding::Cobs => decode_cobs(frame)?,
    };
    if payload.len() > MAX_RESPONSE_BYTES {
        bail!("custom serial decoded response exceeds the 4096-byte limit");
    }
    Ok(payload)
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
        CustomSerialChecksum::Crc16CcittFalse => {
            if response.len() < 3 {
                bail!("custom serial response is too short for CRC-16/CCITT-FALSE");
            }
            let payload = &response[..response.len() - 2];
            let expected =
                u16::from_be_bytes([response[response.len() - 2], response[response.len() - 1]]);
            if crc16_ccitt_false(payload) != expected {
                bail!("custom serial CRC-16/CCITT-FALSE mismatch");
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

fn crc16_ccitt_false(bytes: &[u8]) -> u16 {
    let mut crc = 0xFFFF_u16;
    for byte in bytes {
        crc ^= (*byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn encode_slip(payload: &[u8]) -> Vec<u8> {
    const END: u8 = 0xC0;
    const ESC: u8 = 0xDB;
    const ESC_END: u8 = 0xDC;
    const ESC_ESC: u8 = 0xDD;

    let mut encoded = Vec::with_capacity(payload.len() + 2);
    encoded.push(END);
    for byte in payload {
        match *byte {
            END => encoded.extend([ESC, ESC_END]),
            ESC => encoded.extend([ESC, ESC_ESC]),
            byte => encoded.push(byte),
        }
    }
    encoded.push(END);
    encoded
}

fn decode_slip(frame: &[u8]) -> Result<Vec<u8>> {
    const END: u8 = 0xC0;
    const ESC: u8 = 0xDB;
    const ESC_END: u8 = 0xDC;
    const ESC_ESC: u8 = 0xDD;

    if frame.len() < 2 || frame.first() != Some(&END) || frame.last() != Some(&END) {
        bail!("custom serial SLIP frame must start and end with 0xC0");
    }
    let mut decoded = Vec::with_capacity(frame.len() - 2);
    let mut bytes = frame[1..frame.len() - 1].iter();
    while let Some(byte) = bytes.next() {
        match *byte {
            END => bail!("custom serial SLIP frame contains an unescaped 0xC0 byte"),
            ESC => match bytes.next().copied() {
                Some(ESC_END) => decoded.push(END),
                Some(ESC_ESC) => decoded.push(ESC),
                Some(value) => {
                    bail!("custom serial SLIP frame has invalid escape byte 0x{value:02X}")
                }
                None => bail!("custom serial SLIP frame ends with an incomplete escape"),
            },
            byte => decoded.push(byte),
        }
    }
    Ok(decoded)
}

fn encode_cobs(payload: &[u8]) -> Vec<u8> {
    let mut encoded = vec![0_u8];
    let mut code_index = 0;
    let mut code = 1_u8;
    for byte in payload {
        if *byte == 0 {
            encoded[code_index] = code;
            code_index = encoded.len();
            encoded.push(0);
            code = 1;
        } else {
            encoded.push(*byte);
            code = code.wrapping_add(1);
            if code == 0xFF {
                encoded[code_index] = code;
                code_index = encoded.len();
                encoded.push(0);
                code = 1;
            }
        }
    }
    encoded[code_index] = code;
    encoded.push(0);
    encoded
}

fn decode_cobs(frame: &[u8]) -> Result<Vec<u8>> {
    if frame.len() < 2 || frame.last() != Some(&0) {
        bail!("custom serial COBS frame must end with a 0x00 delimiter");
    }
    let encoded = &frame[..frame.len() - 1];
    if encoded.is_empty() {
        bail!("custom serial COBS frame is empty");
    }
    let mut decoded = Vec::with_capacity(encoded.len());
    let mut index = 0;
    while index < encoded.len() {
        let code = encoded[index];
        if code == 0 {
            bail!("custom serial COBS frame contains an unexpected 0x00 byte");
        }
        index += 1;
        let block_length = usize::from(code - 1);
        let block_end = index
            .checked_add(block_length)
            .context("custom serial COBS block length overflows")?;
        if block_end > encoded.len() {
            bail!("custom serial COBS block exceeds the frame length");
        }
        if encoded[index..block_end].contains(&0) {
            bail!("custom serial COBS block contains an unexpected 0x00 byte");
        }
        decoded.extend_from_slice(&encoded[index..block_end]);
        index = block_end;
        if code != 0xFF && index < encoded.len() {
            decoded.push(0);
        }
    }
    Ok(decoded)
}
