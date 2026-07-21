use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use edge_core::{
    DataQuality, ProtocolConnection, ProtocolType, TelemetryPointMapping, TelemetrySample,
    TelemetryType, TelemetryValue,
};

use crate::{ProtocolAdapter, SerialBus};

const FRAME_START: u8 = 0x68;
const FRAME_END: u8 = 0x16;
const READ_DATA: u8 = 0x11;
const READ_DATA_RESPONSE: u8 = 0x91;
const DATA_OFFSET: u8 = 0x33;

pub struct Dlt645Adapter<B> {
    connection: ProtocolConnection,
    mappings: Vec<TelemetryPointMapping>,
    bus: B,
}

impl<B> Dlt645Adapter<B> {
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
impl<B> ProtocolAdapter for Dlt645Adapter<B>
where
    B: SerialBus,
{
    async fn read_telemetry(&mut self) -> Result<Vec<TelemetrySample>> {
        if self.connection.protocol != ProtocolType::Dlt645 {
            bail!("DL/T 645 adapter requires a Dlt645 protocol connection");
        }

        let mut samples = Vec::new();
        for mapping in &self.mappings {
            if mapping.protocol_connection_id != self.connection.connection_id {
                continue;
            }
            let address = parse_point_address(&mapping.address.kind, &mapping.address.value)
                .with_context(|| format!("invalid point address for {}", mapping.point_id))?;
            let request = build_read_data_request(address.meter, address.data_identifier);
            let response = self.bus.transact(&request).await?;
            let value_bytes =
                parse_read_data_response(&response, address.meter, address.data_identifier)?;
            let value = decode_bcd_value(mapping.value_type, &value_bytes, address.decimal_places)?;
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
struct Dlt645PointAddress {
    meter: [u8; 6],
    data_identifier: u32,
    decimal_places: u8,
}

fn parse_point_address(kind: &str, value: &str) -> Result<Dlt645PointAddress> {
    if kind != "dlt645_address" {
        bail!("DL/T 645 adapter supports dlt645_address addresses");
    }
    let mut parts = value.split(':');
    let meter = parse_meter_address(parts.next().unwrap_or_default())?;
    let data_identifier_text = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("DL/T 645 data identifier is required"))?;
    let decimal_places = parts
        .next()
        .map(|text| {
            text.parse::<u8>()
                .with_context(|| format!("invalid DL/T 645 decimal places: {text}"))
        })
        .transpose()?
        .unwrap_or(0);
    if parts.next().is_some() {
        bail!("DL/T 645 address must be meter:data_identifier[:decimal_places]");
    }
    if decimal_places > 18 {
        bail!("DL/T 645 decimal places cannot exceed 18");
    }

    let data_identifier_text = data_identifier_text
        .strip_prefix("0x")
        .or_else(|| data_identifier_text.strip_prefix("0X"))
        .unwrap_or(data_identifier_text);
    if data_identifier_text.len() != 8 {
        bail!("DL/T 645 data identifier must contain 8 hexadecimal digits");
    }
    let data_identifier = u32::from_str_radix(data_identifier_text, 16)
        .with_context(|| format!("invalid DL/T 645 data identifier: {data_identifier_text}"))?;

    Ok(Dlt645PointAddress {
        meter,
        data_identifier,
        decimal_places,
    })
}

fn parse_meter_address(value: &str) -> Result<[u8; 6]> {
    let digits = value.trim();
    if digits.len() != 12 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        bail!("DL/T 645 meter address must contain 12 decimal digits");
    }

    let mut address = [0_u8; 6];
    for (index, pair_start) in (0..12).step_by(2).rev().enumerate() {
        let high = digits.as_bytes()[pair_start] - b'0';
        let low = digits.as_bytes()[pair_start + 1] - b'0';
        address[index] = (high << 4) | low;
    }
    Ok(address)
}

fn build_read_data_request(meter: [u8; 6], data_identifier: u32) -> Vec<u8> {
    let mut frame = Vec::with_capacity(16);
    frame.push(FRAME_START);
    frame.extend_from_slice(&meter);
    frame.push(FRAME_START);
    frame.push(READ_DATA);
    frame.push(4);
    frame.extend(
        data_identifier
            .to_le_bytes()
            .into_iter()
            .map(|byte| byte.wrapping_add(DATA_OFFSET)),
    );
    append_dlt645_checksum(&mut frame);
    frame.push(FRAME_END);
    frame
}

pub fn append_dlt645_checksum(frame: &mut Vec<u8>) {
    frame.push(dlt645_checksum(frame));
}

fn parse_read_data_response(
    response: &[u8],
    expected_meter: [u8; 6],
    expected_data_identifier: u32,
) -> Result<Vec<u8>> {
    let start = response
        .iter()
        .position(|byte| *byte == FRAME_START)
        .ok_or_else(|| anyhow::anyhow!("DL/T 645 response has no frame start"))?;
    let frame = &response[start..];
    if frame.len() < 12 {
        bail!("DL/T 645 response is too short");
    }
    if frame[7] != FRAME_START {
        bail!("DL/T 645 response has an invalid second frame start");
    }
    let data_len = frame[9] as usize;
    let expected_len = 12 + data_len;
    if frame.len() != expected_len {
        bail!("DL/T 645 response length does not match data length");
    }
    if frame[expected_len - 1] != FRAME_END {
        bail!("DL/T 645 response has an invalid frame end");
    }
    if dlt645_checksum(&frame[..expected_len - 2]) != frame[expected_len - 2] {
        bail!("DL/T 645 checksum mismatch");
    }
    if frame[1..7] != expected_meter {
        bail!("DL/T 645 response meter address does not match request");
    }

    let control = frame[8];
    let decoded = frame[10..10 + data_len]
        .iter()
        .map(|byte| byte.wrapping_sub(DATA_OFFSET))
        .collect::<Vec<_>>();
    if control & 0x40 != 0 {
        let code = decoded.first().copied().unwrap_or_default();
        bail!("DL/T 645 exception code 0x{code:02X}");
    }
    if control != READ_DATA_RESPONSE {
        bail!("DL/T 645 response control code does not match read-data request");
    }
    if decoded.len() < 4 {
        bail!("DL/T 645 read-data response is missing its data identifier");
    }
    let actual_identifier = u32::from_le_bytes(decoded[..4].try_into().expect("checked length"));
    if actual_identifier != expected_data_identifier {
        bail!("DL/T 645 response data identifier does not match request");
    }

    Ok(decoded[4..].to_vec())
}

fn decode_bcd_value(
    value_type: TelemetryType,
    bytes: &[u8],
    decimal_places: u8,
) -> Result<TelemetryValue> {
    if bytes.is_empty() {
        bail!("DL/T 645 response contains no value data");
    }
    let mut digits = String::with_capacity(bytes.len() * 2);
    for byte in bytes.iter().rev() {
        let high = byte >> 4;
        let low = byte & 0x0F;
        if high > 9 || low > 9 {
            bail!("DL/T 645 value contains invalid BCD data");
        }
        digits.push(char::from(b'0' + high));
        digits.push(char::from(b'0' + low));
    }

    match value_type {
        TelemetryType::Text => Ok(TelemetryValue::Text(digits)),
        TelemetryType::Integer => {
            let value = digits
                .parse::<i64>()
                .context("DL/T 645 integer value exceeds i64 range")?;
            Ok(TelemetryValue::Integer(value))
        }
        TelemetryType::Float => {
            let value = digits
                .parse::<u64>()
                .context("DL/T 645 float value exceeds u64 range")?;
            let divisor = 10_u64.pow(decimal_places as u32) as f64;
            Ok(TelemetryValue::Float(value as f64 / divisor))
        }
        TelemetryType::Boolean => Ok(TelemetryValue::Boolean(
            digits.bytes().any(|byte| byte != b'0'),
        )),
    }
}

fn dlt645_checksum(bytes: &[u8]) -> u8 {
    bytes
        .iter()
        .fold(0_u8, |checksum, byte| checksum.wrapping_add(*byte))
}
