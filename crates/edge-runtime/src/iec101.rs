use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Duration, FixedOffset, TimeZone, Timelike, Utc};
use edge_core::{
    parse_iec101_point_address, validate_iec101_point, DataQuality, Iec101ControlType,
    ProtocolConnection, ProtocolType, TelemetryPointMapping, TelemetrySample, TelemetryType,
    TelemetryValue,
};

use crate::{ProtocolAdapter, ProtocolCommandAdapter, ProtocolWriteResult, SerialBus};

const VARIABLE_FRAME_START: u8 = 0x68;
const FIXED_FRAME_START: u8 = 0x10;
const FRAME_END: u8 = 0x16;
const SINGLE_CHAR_ACK: u8 = 0xE5;
const PRIMARY_RESET_REMOTE_LINK: u8 = 0x40;
const PRIMARY_SEND_CONFIRMED_USER_DATA: u8 = 0x53;
const PRIMARY_REQUEST_CLASS_1_DATA: u8 = 0x5A;
const PRIMARY_REQUEST_CLASS_2_DATA: u8 = 0x5B;
const C_RD_NA_1: u8 = 102;
const C_SC_NA_1: u8 = 45;
const C_DC_NA_1: u8 = 46;
const C_SE_NC_1: u8 = 50;
const COT_REQUEST: u8 = 5;
const COT_ACTIVATION: u8 = 6;
const COT_ACTIVATION_CONFIRMATION: u8 = 7;

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

            let cp56_timezone_offset_minutes = self
                .connection
                .iec101
                .unwrap_or_default()
                .cp56_timezone_offset_minutes;
            let decoded = parse_read_response(
                &response,
                address,
                mapping.value_type,
                cp56_timezone_offset_minutes,
            )?;
            samples.push(TelemetrySample::new(
                &mapping.device_id,
                &mapping.point_id,
                decoded.value,
                decoded.quality,
                decoded.timestamp.unwrap_or_else(Utc::now),
            ));
        }

        Ok(samples)
    }
}

#[async_trait]
impl<B> ProtocolCommandAdapter for Iec101Adapter<B>
where
    B: SerialBus,
{
    async fn write_point(
        &mut self,
        mapping: &TelemetryPointMapping,
        value: TelemetryValue,
    ) -> Result<ProtocolWriteResult> {
        if self.connection.protocol != ProtocolType::Iec101 {
            bail!("IEC 101 adapter requires an Iec101 protocol connection");
        }
        if mapping.protocol_connection_id != self.connection.connection_id {
            bail!(
                "IEC 101 point {} references connection {} instead of {}",
                mapping.point_id,
                mapping.protocol_connection_id,
                self.connection.connection_id
            );
        }
        if !mapping.access.is_writable() {
            bail!("IEC 101 point {} is read-only", mapping.point_id);
        }
        validate_iec101_point(
            &mapping.address,
            mapping.value_type,
            mapping.access,
            mapping.iec101,
        )
        .map_err(anyhow::Error::msg)?;
        let address = parse_point_address(&mapping.address.kind, &mapping.address.value)
            .with_context(|| format!("invalid IEC 101 address for point {}", mapping.point_id))?;
        let options = mapping
            .iec101
            .context("writable IEC 101 point is missing control options")?;
        let command = Iec101CommandValue::try_from(options.control_type, &value)?;

        self.ensure_link_initialized(address.link_address).await?;
        if options.select_before_operate {
            self.send_command(address, options.control_type, &command, true, "select")
                .await?;
        }
        self.send_command(address, options.control_type, &command, false, "execute")
            .await?;

        Ok(ProtocolWriteResult {
            point_id: mapping.point_id.clone(),
            value,
            verified: true,
            readback_value: None,
        })
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

    async fn send_command(
        &mut self,
        address: Iec101PointAddress,
        control_type: Iec101ControlType,
        command: &Iec101CommandValue,
        select: bool,
        phase: &str,
    ) -> Result<()> {
        let control = self.primary_control(PRIMARY_SEND_CONFIRMED_USER_DATA);
        let request = build_control_command(control, address, control_type, command, select)?;
        let response = self
            .bus
            .transact(&request)
            .await
            .with_context(|| format!("IEC 101 {phase} command transport failed"))?;
        self.frame_count_bit = !self.frame_count_bit;

        let confirmation = if contains_variable_frame(&response) {
            response
        } else {
            validate_link_ack(&response, address.link_address)
                .with_context(|| format!("IEC 101 {phase} command was not acknowledged"))?;
            let poll = build_fixed_frame(
                self.primary_control(PRIMARY_REQUEST_CLASS_1_DATA),
                address.link_address,
            );
            let response = self
                .bus
                .transact(&poll)
                .await
                .with_context(|| format!("IEC 101 {phase} confirmation poll failed"))?;
            if !contains_variable_frame(&response) {
                bail!("IEC 101 {phase} confirmation contains no ASDU data");
            }
            response
        };

        parse_command_confirmation(&confirmation, address, control_type, phase)
    }

    fn primary_control(&self, function: u8) -> u8 {
        if self.frame_count_bit {
            function | 0x20
        } else {
            function
        }
    }
}

enum Iec101CommandValue {
    Single(bool),
    Double(u8),
    SetpointFloat(f32),
}

impl Iec101CommandValue {
    fn try_from(control_type: Iec101ControlType, value: &TelemetryValue) -> Result<Self> {
        match (control_type, value) {
            (Iec101ControlType::SingleCommand, TelemetryValue::Boolean(value)) => {
                Ok(Self::Single(*value))
            }
            (Iec101ControlType::DoubleCommand, TelemetryValue::Integer(value))
                if matches!(*value, 1 | 2) =>
            {
                Ok(Self::Double(*value as u8))
            }
            (Iec101ControlType::DoubleCommand, TelemetryValue::Integer(value)) => {
                bail!("IEC 101 double command value must be 1 (OFF) or 2 (ON), got {value}")
            }
            (Iec101ControlType::SetpointFloat, TelemetryValue::Float(value))
                if value.is_finite() && *value >= f32::MIN as f64 && *value <= f32::MAX as f64 =>
            {
                Ok(Self::SetpointFloat(*value as f32))
            }
            (Iec101ControlType::SetpointFloat, TelemetryValue::Float(value)) => {
                bail!("IEC 101 float setpoint must be a finite f32 value, got {value}")
            }
            (control_type, value) => bail!(
                "IEC 101 control type {control_type:?} cannot encode telemetry value {value:?}"
            ),
        }
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
    timestamp: Option<DateTime<Utc>>,
}

fn parse_point_address(kind: &str, value: &str) -> Result<Iec101PointAddress> {
    if kind != "iec101_ioa" {
        bail!("IEC 101 adapter supports iec101_ioa addresses");
    }
    let (link_address, common_address, information_object_address) =
        parse_iec101_point_address(value).map_err(anyhow::Error::msg)?;

    Ok(Iec101PointAddress {
        link_address,
        common_address,
        information_object_address,
    })
}

fn build_control_command(
    control: u8,
    address: Iec101PointAddress,
    control_type: Iec101ControlType,
    command: &Iec101CommandValue,
    select: bool,
) -> Result<Vec<u8>> {
    let type_id = match control_type {
        Iec101ControlType::SingleCommand => C_SC_NA_1,
        Iec101ControlType::DoubleCommand => C_DC_NA_1,
        Iec101ControlType::SetpointFloat => C_SE_NC_1,
    };
    let select_bit = if select { 0x80 } else { 0 };
    let mut information = match (control_type, command) {
        (Iec101ControlType::SingleCommand, Iec101CommandValue::Single(value)) => {
            vec![u8::from(*value) | select_bit]
        }
        (Iec101ControlType::DoubleCommand, Iec101CommandValue::Double(value)) => {
            vec![*value | select_bit]
        }
        (Iec101ControlType::SetpointFloat, Iec101CommandValue::SetpointFloat(value)) => {
            let mut bytes = value.to_le_bytes().to_vec();
            bytes.push(select_bit);
            bytes
        }
        _ => bail!("IEC 101 command value does not match its configured control type"),
    };
    let mut asdu = vec![type_id, 1, COT_ACTIVATION, 0];
    asdu.extend(address.common_address.to_le_bytes());
    asdu.extend([
        address.information_object_address as u8,
        (address.information_object_address >> 8) as u8,
        (address.information_object_address >> 16) as u8,
    ]);
    asdu.append(&mut information);

    let mut body = vec![control, address.link_address];
    body.extend(asdu);
    Ok(build_variable_frame(&body))
}

fn parse_command_confirmation(
    response: &[u8],
    expected: Iec101PointAddress,
    control_type: Iec101ControlType,
    phase: &str,
) -> Result<()> {
    let body = parse_variable_frame(response)?;
    if body.len() < 11 {
        bail!("IEC 101 {phase} confirmation ASDU is too short");
    }
    if body[1] != expected.link_address {
        bail!("IEC 101 {phase} confirmation link address does not match request");
    }
    let asdu = &body[2..];
    let expected_type = match control_type {
        Iec101ControlType::SingleCommand => C_SC_NA_1,
        Iec101ControlType::DoubleCommand => C_DC_NA_1,
        Iec101ControlType::SetpointFloat => C_SE_NC_1,
    };
    if asdu[0] != expected_type {
        bail!("IEC 101 {phase} confirmation type does not match request");
    }
    if asdu[1] & 0x7F != 1 || asdu[1] & 0x80 != 0 {
        bail!("IEC 101 {phase} confirmation must contain one explicit information object");
    }
    if asdu[2] & 0x3F != COT_ACTIVATION_CONFIRMATION {
        bail!("IEC 101 {phase} confirmation cause is not activation confirmation");
    }
    if asdu[2] & 0x40 != 0 {
        bail!("IEC 101 {phase} command was rejected by the station");
    }
    if u16::from_le_bytes([asdu[4], asdu[5]]) != expected.common_address {
        bail!("IEC 101 {phase} confirmation common address does not match request");
    }
    let ioa = asdu[6] as u32 | ((asdu[7] as u32) << 8) | ((asdu[8] as u32) << 16);
    if ioa != expected.information_object_address {
        bail!("IEC 101 {phase} confirmation information object address does not match request");
    }
    Ok(())
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
    response.contains(&VARIABLE_FRAME_START)
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
    cp56_timezone_offset_minutes: i16,
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

    decode_information_element(
        type_id,
        &asdu[9..],
        expected_type,
        cp56_timezone_offset_minutes,
    )
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
    cp56_timezone_offset_minutes: i16,
) -> Result<DecodedIec101Value> {
    let (base_type_id, time_size) = match type_id {
        1 | 3 | 9 | 11 | 13 => (type_id, 0),
        2 => (1, 3),
        4 => (3, 3),
        10 => (9, 3),
        12 => (11, 3),
        14 => (13, 3),
        30 => (1, 7),
        31 => (3, 7),
        34 => (9, 7),
        35 => (11, 7),
        36 => (13, 7),
        _ => bail!("unsupported IEC 101 monitoring type id: {type_id}"),
    };

    let (raw, mut quality, value_size) = match base_type_id {
        1 => {
            let siq = *bytes
                .first()
                .ok_or_else(|| anyhow::anyhow!("IEC 101 single-point value is missing"))?;
            (TelemetryValue::Boolean(siq & 0x01 != 0), quality(siq), 1)
        }
        3 => {
            let diq = *bytes
                .first()
                .ok_or_else(|| anyhow::anyhow!("IEC 101 double-point value is missing"))?;
            (
                TelemetryValue::Integer((diq & 0x03) as i64),
                quality(diq),
                1,
            )
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
            (value, quality(bytes[2]), 3)
        }
        13 => {
            if bytes.len() < 5 {
                bail!("IEC 101 short floating-point value is truncated");
            }
            let value = f32::from_le_bytes(bytes[..4].try_into().expect("checked length"));
            (TelemetryValue::Float(value as f64), quality(bytes[4]), 5)
        }
        _ => unreachable!("time-tagged types map to a supported base type"),
    };

    if bytes.len() < value_size + time_size {
        bail!("IEC 101 time-tagged information element is truncated");
    }
    let timestamp = match time_size {
        0 => None,
        3 => parse_cp24_time2a(&bytes[value_size..value_size + time_size])?,
        7 => parse_cp56_time2a(
            &bytes[value_size..value_size + time_size],
            cp56_timezone_offset_minutes,
        )?,
        _ => unreachable!("IEC 101 time tag size is known"),
    };
    if time_size > 0 && timestamp.is_none() && quality == DataQuality::Good {
        quality = DataQuality::Uncertain;
    }

    Ok(DecodedIec101Value {
        value: coerce_value(raw, expected_type)?,
        quality,
        timestamp,
    })
}

fn parse_cp24_time2a(bytes: &[u8]) -> Result<Option<DateTime<Utc>>> {
    let bytes: [u8; 3] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("IEC 101 CP24Time2a must contain 3 bytes"))?;
    if bytes[2] & 0x80 != 0 {
        return Ok(None);
    }

    let milliseconds = u16::from_le_bytes([bytes[0], bytes[1]]) as u32;
    let minute = (bytes[2] & 0x3F) as u32;
    if milliseconds >= 60_000 || minute >= 60 {
        bail!("IEC 101 CP24Time2a contains an invalid clock value");
    }

    let observed = Utc::now();
    let second = milliseconds / 1_000;
    let nanosecond = (milliseconds % 1_000) * 1_000_000;
    let candidate = observed
        .with_minute(minute)
        .and_then(|value| value.with_second(second))
        .and_then(|value| value.with_nanosecond(nanosecond))
        .ok_or_else(|| anyhow::anyhow!("IEC 101 CP24Time2a cannot be represented"))?;
    let candidates = [
        candidate - Duration::hours(1),
        candidate,
        candidate + Duration::hours(1),
    ];
    Ok(candidates
        .into_iter()
        .min_by_key(|value| (*value - observed).num_milliseconds().abs()))
}

fn parse_cp56_time2a(
    bytes: &[u8],
    cp56_timezone_offset_minutes: i16,
) -> Result<Option<DateTime<Utc>>> {
    let bytes: [u8; 7] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("IEC 101 CP56Time2a must contain 7 bytes"))?;
    if bytes[2] & 0x80 != 0 {
        return Ok(None);
    }

    let milliseconds = u16::from_le_bytes([bytes[0], bytes[1]]) as u32;
    let minute = (bytes[2] & 0x3F) as u32;
    let hour = (bytes[3] & 0x1F) as u32;
    let day = (bytes[4] & 0x1F) as u32;
    let month = (bytes[5] & 0x0F) as u32;
    let year = 2000 + (bytes[6] & 0x7F) as i32;
    if milliseconds >= 60_000 || minute >= 60 || hour >= 24 {
        bail!("IEC 101 CP56Time2a contains an invalid clock value");
    }

    let second = milliseconds / 1_000;
    let station_offset = FixedOffset::east_opt(i32::from(cp56_timezone_offset_minutes) * 60)
        .ok_or_else(|| anyhow::anyhow!("IEC 101 CP56Time2a timezone offset is invalid"))?;
    let timestamp = station_offset
        .with_ymd_and_hms(year, month, day, hour, minute, second)
        .single()
        .ok_or_else(|| anyhow::anyhow!("IEC 101 CP56Time2a contains an invalid date"))?
        + Duration::milliseconds((milliseconds % 1_000) as i64);
    Ok(Some(timestamp.with_timezone(&Utc)))
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
