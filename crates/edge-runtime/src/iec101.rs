use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use edge_core::{
    DataQuality, ProtocolConnection, ProtocolType, TelemetryPointMapping, TelemetrySample,
    TelemetryType, TelemetryValue,
};

use crate::{ProtocolAdapter, SerialBus};

const VARIABLE_FRAME_START: u8 = 0x68;
const FIXED_FRAME_START: u8 = 0x10;
const FRAME_END: u8 = 0x16;
const SINGLE_CHAR_ACK: u8 = 0xE5;
const PRIMARY_RESET_REMOTE_LINK: u8 = 0x40;
const PRIMARY_SEND_CONFIRMED_USER_DATA: u8 = 0x53;
const PRIMARY_REQUEST_CLASS_2_DATA: u8 = 0x5B;
const C_RD_NA_1: u8 = 102;
const COT_REQUEST: u8 = 5;

pub struct Iec101Adapter<B> {
    connection: ProtocolConnection,
    mappings: Vec<TelemetryPointMapping>,
    bus: B,
    initialized_link: Option<u8>,
    frame_count_bit: bool,
}

impl<B> Iec101Adapter<B> {
    pub fn new(
        connection: ProtocolConnection,
        mappings: Vec<TelemetryPointMapping>,
        bus: B,
    ) -> Self {
        Self {
            connection,
            mappings,
            bus,
            initialized_link: None,
            frame_count_bit: false,
        }
    }
}

#[async_trait]
impl<B> ProtocolAdapter for Iec101Adapter<B>
where
    B: SerialBus,
{
    async fn read_telemetry(&mut self) -> Result<Vec<TelemetrySample>> {
        if self.connection.protocol != ProtocolType::Iec101 {
            bail!("IEC 101 adapter requires an Iec101 protocol connection");
        }

        let mappings = self
            .mappings
            .iter()
            .filter(|mapping| mapping.protocol_connection_id == self.connection.connection_id)
            .cloned()
            .collect::<Vec<_>>();
        let mut samples = Vec::with_capacity(mappings.len());

        for mapping in mappings {
            let address = parse_point_address(&mapping.address.kind, &mapping.address.value)
                .with_context(|| format!("invalid point address for {}", mapping.point_id))?;
            self.ensure_link_initialized(address.link_address).await?;

            let control = if self.frame_count_bit {
                PRIMARY_SEND_CONFIRMED_USER_DATA | 0x20
            } else {
                PRIMARY_SEND_CONFIRMED_USER_DATA
            };
            let request = build_read_command(control, address);
            let response = self.bus.transact(&request).await?;
            self.frame_count_bit = !self.frame_count_bit;

            let response = if contains_variable_frame(&response) {
                response
            } else {
                validate_link_ack(&response, address.link_address)?;
                let class_2_control = if self.frame_count_bit {
                    PRIMARY_REQUEST_CLASS_2_DATA | 0x20
                } else {
                    PRIMARY_REQUEST_CLASS_2_DATA
                };
                let class_2_request = build_fixed_frame(class_2_control, address.link_address);
                let class_2_response = self.bus.transact(&class_2_request).await?;
                if !contains_variable_frame(&class_2_response) {
                    bail!("IEC 101 class 2 response contains no ASDU data");
                }
                class_2_response
            };

            let decoded = parse_read_response(&response, address, mapping.value_type)?;
            samples.push(TelemetrySample::new(
                &mapping.device_id,
                &mapping.point_id,
                decoded.value,
                decoded.quality,
                Utc::now(),
            ));
        }

        Ok(samples)
    }
}

impl<B> Iec101Adapter<B>
where
    B: SerialBus,
{
    async fn ensure_link_initialized(&mut self, link_address: u8) -> Result<()> {
        if self.initialized_link == Some(link_address) {
            return Ok(());
        }

        let reset = build_fixed_frame(PRIMARY_RESET_REMOTE_LINK, link_address);
        let response = self.bus.transact(&reset).await?;
        validate_link_ack(&response, link_address)
            .context("IEC 101 remote link reset was not acknowledged")?;
        self.initialized_link = Some(link_address);
        self.frame_count_bit = false;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Iec101PointAddress {
    link_address: u8,
    common_address: u16,
    information_object_address: u32,
}

#[derive(Clone, Debug, PartialEq)]
struct DecodedIec101Value {
    value: TelemetryValue,
    quality: DataQuality,
}

fn parse_point_address(kind: &str, value: &str) -> Result<Iec101PointAddress> {
    if kind != "iec101_ioa" {
        bail!("IEC 101 adapter supports iec101_ioa addresses");
    }
    let parts = value.split(':').collect::<Vec<_>>();
    if parts.len() != 3 {
        bail!("IEC 101 address must be link_address:common_address:ioa");
    }
    let link_address = parts[0]
        .parse::<u8>()
        .with_context(|| format!("invalid IEC 101 link address: {}", parts[0]))?;
    let common_address = parts[1]
        .parse::<u16>()
        .with_context(|| format!("invalid IEC 101 common address: {}", parts[1]))?;
    let information_object_address = parts[2]
        .parse::<u32>()
        .with_context(|| format!("invalid IEC 101 information object address: {}", parts[2]))?;
    if information_object_address > 0x00FF_FFFF {
        bail!("IEC 101 information object address exceeds 3-byte range");
    }

    Ok(Iec101PointAddress {
        link_address,
        common_address,
        information_object_address,
    })
}

fn build_read_command(control: u8, address: Iec101PointAddress) -> Vec<u8> {
    let mut asdu = vec![C_RD_NA_1, 1, COT_REQUEST, 0];
    asdu.extend(address.common_address.to_le_bytes());
    asdu.extend([
        address.information_object_address as u8,
        (address.information_object_address >> 8) as u8,
        (address.information_object_address >> 16) as u8,
    ]);

    let mut body = vec![control, address.link_address];
    body.extend(asdu);
    build_variable_frame(&body)
}

fn build_variable_frame(body: &[u8]) -> Vec<u8> {
    let length = u8::try_from(body.len()).expect("IEC 101 FT1.2 body must fit one-byte length");
    let mut frame = vec![VARIABLE_FRAME_START, length, length, VARIABLE_FRAME_START];
    frame.extend_from_slice(body);
    append_iec101_checksum(&mut frame, 4);
    frame.push(FRAME_END);
    frame
}

fn build_fixed_frame(control: u8, link_address: u8) -> Vec<u8> {
    let mut frame = vec![FIXED_FRAME_START, control, link_address];
    append_iec101_checksum(&mut frame, 1);
    frame.push(FRAME_END);
    frame
}

pub fn append_iec101_checksum(frame: &mut Vec<u8>, payload_start: usize) {
    let checksum = frame[payload_start..]
        .iter()
        .fold(0_u8, |sum, byte| sum.wrapping_add(*byte));
    frame.push(checksum);
}

fn contains_variable_frame(response: &[u8]) -> bool {
    response.iter().any(|byte| *byte == VARIABLE_FRAME_START)
}

fn validate_link_ack(response: &[u8], expected_link_address: u8) -> Result<()> {
    if response == [SINGLE_CHAR_ACK] {
        return Ok(());
    }
    let start = response
        .iter()
        .position(|byte| *byte == FIXED_FRAME_START)
        .ok_or_else(|| anyhow::anyhow!("IEC 101 response is not a link acknowledgement"))?;
    let frame = response
        .get(start..start + 5)
        .ok_or_else(|| anyhow::anyhow!("IEC 101 fixed frame is truncated"))?;
    if frame[4] != FRAME_END {
        bail!("IEC 101 fixed frame has an invalid end byte");
    }
    if frame[2] != expected_link_address {
        bail!("IEC 101 fixed frame link address does not match request");
    }
    if frame[3] != frame[1].wrapping_add(frame[2]) {
        bail!("IEC 101 fixed frame checksum mismatch");
    }
    Ok(())
}

fn parse_read_response(
    response: &[u8],
    expected: Iec101PointAddress,
    expected_type: TelemetryType,
) -> Result<DecodedIec101Value> {
    let body = parse_variable_frame(response)?;
    if body.len() < 11 {
        bail!("IEC 101 response ASDU is too short");
    }
    if body[1] != expected.link_address {
        bail!("IEC 101 response link address does not match request");
    }

    let asdu = &body[2..];
    let type_id = asdu[0];
    let variable_structure_qualifier = asdu[1];
    if variable_structure_qualifier & 0x7F != 1 || variable_structure_qualifier & 0x80 != 0 {
        bail!("IEC 101 point read response must contain one explicit information object");
    }
    if asdu[2] & 0x3F != COT_REQUEST {
        bail!("IEC 101 response cause of transmission is not request");
    }
    let common_address = u16::from_le_bytes([asdu[4], asdu[5]]);
    if common_address != expected.common_address {
        bail!("IEC 101 response common address does not match request");
    }
    let information_object_address =
        asdu[6] as u32 | ((asdu[7] as u32) << 8) | ((asdu[8] as u32) << 16);
    if information_object_address != expected.information_object_address {
        bail!("IEC 101 response information object address does not match request");
    }

    decode_information_element(type_id, &asdu[9..], expected_type)
}

fn parse_variable_frame(response: &[u8]) -> Result<&[u8]> {
    let start = response
        .iter()
        .position(|byte| *byte == VARIABLE_FRAME_START)
        .ok_or_else(|| anyhow::anyhow!("IEC 101 response has no variable frame"))?;
    let header = response
        .get(start..start + 4)
        .ok_or_else(|| anyhow::anyhow!("IEC 101 variable frame header is truncated"))?;
    if header[1] != header[2] || header[3] != VARIABLE_FRAME_START {
        bail!("IEC 101 variable frame length header is invalid");
    }
    let body_len = header[1] as usize;
    let frame_len = body_len + 6;
    let frame = response
        .get(start..start + frame_len)
        .ok_or_else(|| anyhow::anyhow!("IEC 101 variable frame is truncated"))?;
    if frame[frame_len - 1] != FRAME_END {
        bail!("IEC 101 variable frame has an invalid end byte");
    }
    let body = &frame[4..4 + body_len];
    let checksum = body.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte));
    if checksum != frame[frame_len - 2] {
        bail!("IEC 101 variable frame checksum mismatch");
    }
    Ok(body)
}

fn decode_information_element(
    type_id: u8,
    bytes: &[u8],
    expected_type: TelemetryType,
) -> Result<DecodedIec101Value> {
    let (raw, quality) = match type_id {
        1 => {
            let siq = *bytes
                .first()
                .ok_or_else(|| anyhow::anyhow!("IEC 101 single-point value is missing"))?;
            (TelemetryValue::Boolean(siq & 0x01 != 0), quality(siq))
        }
        3 => {
            let diq = *bytes
                .first()
                .ok_or_else(|| anyhow::anyhow!("IEC 101 double-point value is missing"))?;
            (TelemetryValue::Integer((diq & 0x03) as i64), quality(diq))
        }
        9 | 11 => {
            if bytes.len() < 3 {
                bail!("IEC 101 measured value is truncated");
            }
            let value = i16::from_le_bytes([bytes[0], bytes[1]]);
            let value = if type_id == 9 {
                TelemetryValue::Float(value as f64 / i16::MAX as f64)
            } else {
                TelemetryValue::Integer(value as i64)
            };
            (value, quality(bytes[2]))
        }
        13 => {
            if bytes.len() < 5 {
                bail!("IEC 101 short floating-point value is truncated");
            }
            let value = f32::from_le_bytes(bytes[..4].try_into().expect("checked length"));
            (TelemetryValue::Float(value as f64), quality(bytes[4]))
        }
        _ => bail!("unsupported IEC 101 monitoring type id: {type_id}"),
    };

    Ok(DecodedIec101Value {
        value: coerce_value(raw, expected_type)?,
        quality,
    })
}

fn quality(descriptor: u8) -> DataQuality {
    if descriptor & 0x80 != 0 {
        DataQuality::Bad
    } else if descriptor & 0x70 != 0 {
        DataQuality::Uncertain
    } else {
        DataQuality::Good
    }
}

fn coerce_value(value: TelemetryValue, expected_type: TelemetryType) -> Result<TelemetryValue> {
    match (value, expected_type) {
        (value @ TelemetryValue::Float(_), TelemetryType::Float)
        | (value @ TelemetryValue::Integer(_), TelemetryType::Integer)
        | (value @ TelemetryValue::Boolean(_), TelemetryType::Boolean)
        | (value @ TelemetryValue::Text(_), TelemetryType::Text) => Ok(value),
        (TelemetryValue::Integer(value), TelemetryType::Float) => {
            Ok(TelemetryValue::Float(value as f64))
        }
        (TelemetryValue::Boolean(value), TelemetryType::Integer) => {
            Ok(TelemetryValue::Integer(i64::from(value)))
        }
        (TelemetryValue::Boolean(value), TelemetryType::Float) => {
            Ok(TelemetryValue::Float(if value { 1.0 } else { 0.0 }))
        }
        (TelemetryValue::Integer(value), TelemetryType::Boolean) => {
            Ok(TelemetryValue::Boolean(value != 0))
        }
        (TelemetryValue::Float(value), TelemetryType::Boolean) => {
            Ok(TelemetryValue::Boolean(value != 0.0))
        }
        (TelemetryValue::Float(value), TelemetryType::Integer) => {
            Ok(TelemetryValue::Integer(value as i64))
        }
        (value, TelemetryType::Text) => Ok(TelemetryValue::Text(match value {
            TelemetryValue::Float(value) => value.to_string(),
            TelemetryValue::Integer(value) => value.to_string(),
            TelemetryValue::Boolean(value) => value.to_string(),
            TelemetryValue::Text(value) => value,
        })),
        (TelemetryValue::Text(_), _) => {
            bail!("IEC 101 text value cannot be coerced to numeric type")
        }
    }
}
