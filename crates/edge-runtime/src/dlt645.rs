use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use edge_core::{
    dlt645_template_by_identifier, parse_dlt645_point_address, DataQuality, DataQualityCode,
    ProtocolConnection, ProtocolType, TelemetryPointMapping, TelemetrySample, TelemetryType,
    TelemetryValue,
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
    read_failures: Vec<Dlt645ReadFailure>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dlt645ReadFailure {
    pub meter_address: String,
    pub data_identifier: u32,
    pub point_count: usize,
    pub quality_code: DataQualityCode,
    pub reason: String,
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
            read_failures: Vec::new(),
        }
    }

    pub fn read_failures(&self) -> &[Dlt645ReadFailure] {
        &self.read_failures
    }
}

#[async_trait]
impl<B> ProtocolAdapter for Dlt645Adapter<B>
where
    B: SerialBus,
{
    async fn read_telemetry(&mut self) -> Result<Vec<TelemetrySample>> {
        self.read_failures.clear();
        if self.connection.protocol != ProtocolType::Dlt645 {
            bail!("DL/T 645 adapter requires a Dlt645 protocol connection");
        }

        let groups = build_read_groups(&self.connection.connection_id, &self.mappings)?;
        let mut samples = Vec::with_capacity(groups.iter().map(|group| group.targets.len()).sum());
        let mut successful_groups = 0_usize;
        let mut failed_groups = 0_usize;
        let mut first_group_error = None;
        for group in groups {
            let value_bytes = match read_group(&mut self.bus, &group).await {
                Ok(value_bytes) => {
                    successful_groups = successful_groups.saturating_add(1);
                    value_bytes
                }
                Err(error) => {
                    failed_groups = failed_groups.saturating_add(1);
                    self.read_failures.push(Dlt645ReadFailure {
                        meter_address: group.meter_address.clone(),
                        data_identifier: group.data_identifier,
                        point_count: group.targets.len(),
                        quality_code: classify_dlt645_error(&error),
                        reason: error.to_string(),
                    });
                    if first_group_error.is_none() {
                        first_group_error = Some(error);
                    }
                    continue;
                }
            };
            for target in group.targets {
                let value = match decode_bcd_value(
                    target.mapping.value_type,
                    &value_bytes,
                    target.decimal_places,
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        self.read_failures.push(Dlt645ReadFailure {
                            meter_address: group.meter_address.clone(),
                            data_identifier: group.data_identifier,
                            point_count: 1,
                            quality_code: classify_dlt645_error(&error),
                            reason: error.to_string(),
                        });
                        continue;
                    }
                };
                samples.push(TelemetrySample::new(
                    &target.mapping.device_id,
                    &target.mapping.point_id,
                    value,
                    DataQuality::Good,
                    Utc::now(),
                ));
            }
        }

        if successful_groups == 0 && failed_groups > 0 {
            let error = first_group_error.expect("a failed group must retain its first error");
            bail!("all {failed_groups} DL/T 645 read groups failed: {error}");
        }

        Ok(samples)
    }
}

#[derive(Clone, Debug)]
struct Dlt645ReadTarget {
    mapping: TelemetryPointMapping,
    decimal_places: u8,
}

#[derive(Clone, Debug)]
struct Dlt645ReadGroup {
    meter_address: String,
    data_identifier: u32,
    expected_value_bytes: Option<u8>,
    targets: Vec<Dlt645ReadTarget>,
}

async fn read_group<B>(bus: &mut B, group: &Dlt645ReadGroup) -> Result<Vec<u8>>
where
    B: SerialBus,
{
    let meter = encode_meter_address(&group.meter_address)?;
    let request = build_read_data_request(meter, group.data_identifier);
    let response = bus.transact(&request).await?;
    let value_bytes = parse_read_data_response(&response, meter, group.data_identifier)?;
    validate_value_length(
        group.data_identifier,
        group.expected_value_bytes,
        value_bytes.len(),
    )?;
    Ok(value_bytes)
}

fn classify_dlt645_error(error: &anyhow::Error) -> DataQualityCode {
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("timeout") || message.contains("timed out") {
        DataQualityCode::BadTimeout
    } else if message.contains("meter address") || message.contains("configuration") {
        DataQualityCode::BadConfiguration
    } else if message.contains("bcd")
        || message.contains("value bytes")
        || message.contains("value data")
        || message.contains("exceeds")
    {
        DataQualityCode::BadDecode
    } else if message.contains("checksum")
        || message.contains("exception")
        || message.contains("response")
        || message.contains("frame")
        || message.contains("control code")
        || message.contains("data identifier")
    {
        DataQualityCode::BadProtocol
    } else {
        DataQualityCode::BadCommunication
    }
}

fn build_read_groups(
    connection_id: &str,
    mappings: &[TelemetryPointMapping],
) -> Result<Vec<Dlt645ReadGroup>> {
    let mut groups: Vec<Dlt645ReadGroup> = Vec::new();
    for mapping in mappings
        .iter()
        .filter(|mapping| mapping.protocol_connection_id == connection_id)
    {
        if mapping.address.kind != "dlt645_address" {
            bail!("DL/T 645 adapter supports dlt645_address addresses");
        }
        let address = parse_dlt645_point_address(&mapping.address.value)
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("invalid point address for {}", mapping.point_id))?;
        let expected_value_bytes = expected_value_bytes(
            address.data_identifier,
            address.value_bytes,
            &mapping.point_id,
        )?;
        let target = Dlt645ReadTarget {
            mapping: mapping.clone(),
            decimal_places: address.decimal_places,
        };
        if let Some(group) = groups.iter_mut().find(|group| {
            group.meter_address == address.meter_address
                && group.data_identifier == address.data_identifier
        }) {
            match (group.expected_value_bytes, expected_value_bytes) {
                (Some(current), Some(candidate)) if current != candidate => bail!(
                    "DL/T 645 points sharing meter {} data identifier {:08X} configure conflicting response value byte lengths: {current} and {candidate}",
                    address.meter_address,
                    address.data_identifier
                ),
                (None, Some(candidate)) => group.expected_value_bytes = Some(candidate),
                _ => {}
            }
            group.targets.push(target);
        } else {
            groups.push(Dlt645ReadGroup {
                meter_address: address.meter_address,
                data_identifier: address.data_identifier,
                expected_value_bytes,
                targets: vec![target],
            });
        }
    }
    Ok(groups)
}

fn encode_meter_address(value: &str) -> Result<[u8; 6]> {
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

fn expected_value_bytes(
    data_identifier: u32,
    configured: Option<u8>,
    point_id: &str,
) -> Result<Option<u8>> {
    let Some(template) = dlt645_template_by_identifier(data_identifier) else {
        return Ok(configured);
    };
    if let Some(configured) = configured {
        if configured != template.value_bytes {
            bail!(
                "DL/T 645 point {point_id} data identifier {} uses the standard response length {} but configures {configured}",
                template.data_identifier,
                template.value_bytes
            );
        }
    }
    Ok(Some(template.value_bytes))
}

fn validate_value_length(
    data_identifier: u32,
    expected_bytes: Option<u8>,
    actual_bytes: usize,
) -> Result<()> {
    let Some(expected_bytes) = expected_bytes else {
        return Ok(());
    };
    if actual_bytes != usize::from(expected_bytes) {
        bail!(
            "DL/T 645 data identifier {data_identifier:08X} expects {expected_bytes} value bytes, received {actual_bytes}"
        );
    }
    Ok(())
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
