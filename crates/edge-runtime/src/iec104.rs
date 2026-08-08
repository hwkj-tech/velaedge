use std::{collections::BTreeMap, time::Duration};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, TimeZone, Timelike, Utc};
use edge_core::{
    parse_iec104_point_address, validate_iec104_point, DataQualityCode, Iec104ControlType,
    ProtocolConnection, ProtocolType, TelemetryPointMapping, TelemetrySample, TelemetryType,
    TelemetryValue,
};
use tokio::time::timeout;
use voltage_iec104::{
    ClientConfig, ConnectionState, Cp56Time2a, DataPoint, DataValue, DoublePointValue,
    Iec104Client, Iec104Event,
};

use crate::{ProtocolAdapter, ProtocolCommandAdapter, ProtocolWriteResult};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const T1_TIMEOUT: Duration = Duration::from_secs(5);
const T2_TIMEOUT: Duration = Duration::from_secs(2);
const T3_TIMEOUT: Duration = Duration::from_secs(20);

pub struct Iec104Adapter {
    connection: ProtocolConnection,
    mappings: Vec<TelemetryPointMapping>,
    client: Option<Iec104Client>,
    connection_generation: u64,
    cp56_timezone_offset_minutes: i16,
}

impl Iec104Adapter {
    pub fn new(
        connection: ProtocolConnection,
        mappings: Vec<TelemetryPointMapping>,
    ) -> Result<Self> {
        if connection.protocol != ProtocolType::Iec104 {
            bail!("IEC 104 adapter requires an IEC 104 connection");
        }
        connection.validate().map_err(anyhow::Error::msg)?;
        validate_mappings(&connection, &mappings)?;
        let cp56_timezone_offset_minutes = connection
            .iec104
            .map_or(0, |settings| settings.cp56_timezone_offset_minutes);
        Ok(Self {
            connection,
            mappings,
            client: None,
            connection_generation: 0,
            cp56_timezone_offset_minutes,
        })
    }

    pub fn set_mappings(&mut self, mappings: Vec<TelemetryPointMapping>) -> Result<()> {
        validate_mappings(&self.connection, &mappings)?;
        self.mappings = mappings;
        Ok(())
    }

    pub fn connection_generation(&self) -> u64 {
        self.connection_generation
    }

    async fn ensure_active(&mut self) -> Result<()> {
        if self
            .client
            .as_ref()
            .is_some_and(|client| client.state() == ConnectionState::Active)
        {
            return Ok(());
        }

        let endpoint = self
            .connection
            .endpoint
            .as_deref()
            .context("IEC 104 endpoint is required")?
            .strip_prefix("tcp://")
            .unwrap_or_else(|| {
                self.connection
                    .endpoint
                    .as_deref()
                    .expect("validated IEC 104 endpoint")
            });
        let config = ClientConfig::new(endpoint)
            .connect_timeout(CONNECT_TIMEOUT)
            .t1_timeout(T1_TIMEOUT)
            .t2_timeout(T2_TIMEOUT)
            .t3_timeout(T3_TIMEOUT);
        let mut client = Iec104Client::new(config);
        client
            .connect()
            .await
            .context("IEC 104 TCP connection failed")?;
        if let Err(error) = client.start_dt().await {
            let _ = client.disconnect().await;
            return Err(anyhow!(error).context("IEC 104 STARTDT handshake failed"));
        }
        self.connection_generation = self.connection_generation.saturating_add(1);
        self.client = Some(client);
        Ok(())
    }

    fn clear_client(&mut self) {
        self.client = None;
    }
}

#[async_trait]
impl ProtocolAdapter for Iec104Adapter {
    async fn read_telemetry(&mut self) -> Result<Vec<TelemetrySample>> {
        if self.mappings.is_empty() {
            return Ok(Vec::new());
        }
        let common_address = common_address(&self.mappings)?;
        let requested = self
            .mappings
            .iter()
            .map(mapping_address)
            .collect::<Result<Vec<_>>>()?;
        self.ensure_active().await?;

        let client = self.client.as_mut().expect("IEC 104 client is active");
        if let Err(error) = client.general_interrogation(common_address).await {
            self.clear_client();
            return Err(anyhow!(error).context("IEC 104 general interrogation failed"));
        }

        let read = async {
            let mut points = BTreeMap::<u32, DataPoint>::new();
            loop {
                match client.poll().await.context("IEC 104 receive failed")? {
                    Some(Iec104Event::DataUpdate(updates)) => {
                        for point in updates {
                            if requested.iter().any(|(_, ioa)| *ioa == point.ioa) {
                                points.insert(point.ioa, point);
                            }
                        }
                        if requested.iter().all(|(_, ioa)| points.contains_key(ioa)) {
                            return Ok(points);
                        }
                    }
                    Some(Iec104Event::InterrogationComplete {
                        common_address: response_address,
                    }) if response_address == common_address => {
                        let missing = requested
                            .iter()
                            .filter_map(|(_, ioa)| (!points.contains_key(ioa)).then_some(*ioa))
                            .collect::<Vec<_>>();
                        if missing.is_empty() {
                            return Ok(points);
                        }
                        bail!("IEC 104 interrogation completed without IOAs {missing:?}");
                    }
                    Some(Iec104Event::Error(message)) => bail!("IEC 104 server error: {message}"),
                    _ => {}
                }
            }
        };

        let points = match timeout(REQUEST_TIMEOUT, read).await {
            Ok(Ok(points)) => points,
            Ok(Err(error)) => {
                self.clear_client();
                return Err(error);
            }
            Err(_) => {
                self.clear_client();
                bail!("IEC 104 general interrogation timed out");
            }
        };

        self.mappings
            .iter()
            .map(|mapping| {
                let (_, ioa) = mapping_address(mapping)?;
                let point = points
                    .get(&ioa)
                    .with_context(|| format!("IEC 104 response is missing IOA {ioa}"))?;
                Ok(point_to_sample(
                    mapping,
                    point,
                    self.cp56_timezone_offset_minutes,
                ))
            })
            .collect()
    }
}

#[async_trait]
impl ProtocolCommandAdapter for Iec104Adapter {
    async fn write_point(
        &mut self,
        mapping: &TelemetryPointMapping,
        value: TelemetryValue,
    ) -> Result<ProtocolWriteResult> {
        if mapping.protocol_connection_id != self.connection.connection_id {
            bail!(
                "IEC 104 point {} references connection {} instead of {}",
                mapping.point_id,
                mapping.protocol_connection_id,
                self.connection.connection_id
            );
        }
        if !mapping.access.is_writable() {
            bail!("IEC 104 point {} is read-only", mapping.point_id);
        }
        validate_iec104_point(
            &mapping.address,
            mapping.value_type,
            mapping.access,
            mapping.iec104,
        )
        .map_err(anyhow::Error::msg)?;
        let (common_address, ioa) = mapping_address(mapping)?;
        let options = mapping
            .iec104
            .context("writable IEC 104 point is missing control options")?;
        let command = Iec104CommandValue::try_from(options.control_type, &value)?;

        self.ensure_active().await?;
        let result = async {
            let client = self.client.as_mut().expect("IEC 104 client is active");
            if options.select_before_operate {
                send_command(
                    client,
                    options.control_type,
                    common_address,
                    ioa,
                    &command,
                    true,
                )
                .await
                .context("IEC 104 select command failed")?;
                wait_for_command_confirmation(client, ioa, "select").await?;
            }
            send_command(
                client,
                options.control_type,
                common_address,
                ioa,
                &command,
                false,
            )
            .await
            .context("IEC 104 execute command failed")?;
            wait_for_command_confirmation(client, ioa, "execute").await
        }
        .await;

        if let Err(error) = result {
            self.clear_client();
            return Err(error);
        }

        Ok(ProtocolWriteResult {
            point_id: mapping.point_id.clone(),
            value,
            verified: true,
            readback_value: None,
        })
    }
}

enum Iec104CommandValue {
    Single(bool),
    Double(u8),
    SetpointFloat(f32),
}

impl Iec104CommandValue {
    fn try_from(control_type: Iec104ControlType, value: &TelemetryValue) -> Result<Self> {
        match (control_type, value) {
            (Iec104ControlType::SingleCommand, TelemetryValue::Boolean(value)) => {
                Ok(Self::Single(*value))
            }
            (Iec104ControlType::DoubleCommand, TelemetryValue::Integer(value))
                if matches!(*value, 1 | 2) =>
            {
                Ok(Self::Double(*value as u8))
            }
            (Iec104ControlType::DoubleCommand, TelemetryValue::Integer(value)) => {
                bail!("IEC 104 double command value must be 1 (OFF) or 2 (ON), got {value}")
            }
            (Iec104ControlType::SetpointFloat, TelemetryValue::Float(value))
                if value.is_finite() && *value >= f32::MIN as f64 && *value <= f32::MAX as f64 =>
            {
                Ok(Self::SetpointFloat(*value as f32))
            }
            (Iec104ControlType::SetpointFloat, TelemetryValue::Float(value)) => {
                bail!("IEC 104 float setpoint must be a finite f32 value, got {value}")
            }
            (control_type, value) => bail!(
                "IEC 104 control type {control_type:?} cannot encode telemetry value {value:?}"
            ),
        }
    }
}

async fn send_command(
    client: &mut Iec104Client,
    control_type: Iec104ControlType,
    common_address: u16,
    ioa: u32,
    value: &Iec104CommandValue,
    select: bool,
) -> Result<()> {
    match (control_type, value) {
        (Iec104ControlType::SingleCommand, Iec104CommandValue::Single(value)) => {
            client
                .single_command(common_address, ioa, *value, select)
                .await?
        }
        (Iec104ControlType::DoubleCommand, Iec104CommandValue::Double(value)) => {
            client
                .double_command(common_address, ioa, *value, select)
                .await?
        }
        (Iec104ControlType::SetpointFloat, Iec104CommandValue::SetpointFloat(value)) => {
            client
                .setpoint_float(common_address, ioa, *value, select)
                .await?
        }
        _ => bail!("IEC 104 command value does not match its configured control type"),
    }
    Ok(())
}

async fn wait_for_command_confirmation(
    client: &mut Iec104Client,
    expected_ioa: u32,
    phase: &str,
) -> Result<()> {
    let confirmation = async {
        loop {
            match client.poll().await.context("IEC 104 receive failed")? {
                Some(Iec104Event::CommandConfirm { ioa, success }) if ioa == expected_ioa => {
                    if !success {
                        bail!("IEC 104 {phase} command was rejected for IOA {expected_ioa}");
                    }
                    return Ok(());
                }
                Some(Iec104Event::Error(message)) => {
                    bail!("IEC 104 server error during {phase}: {message}")
                }
                Some(Iec104Event::Disconnected) => {
                    bail!("IEC 104 connection closed during {phase}")
                }
                _ => {}
            }
        }
    };

    match timeout(REQUEST_TIMEOUT, confirmation).await {
        Ok(result) => result,
        Err(_) => bail!("IEC 104 {phase} confirmation timed out for IOA {expected_ioa}"),
    }
}

fn validate_mappings(
    connection: &ProtocolConnection,
    mappings: &[TelemetryPointMapping],
) -> Result<()> {
    for mapping in mappings {
        if mapping.protocol_connection_id != connection.connection_id {
            bail!(
                "IEC 104 point {} references connection {} instead of {}",
                mapping.point_id,
                mapping.protocol_connection_id,
                connection.connection_id
            );
        }
        validate_iec104_point(
            &mapping.address,
            mapping.value_type,
            mapping.access,
            mapping.iec104,
        )
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("invalid IEC 104 point {}", mapping.point_id))?;
        mapping_address(mapping)?;
    }
    common_address(mappings)?;
    Ok(())
}

fn common_address(mappings: &[TelemetryPointMapping]) -> Result<u16> {
    let mut addresses = mappings
        .iter()
        .map(mapping_address)
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .map(|(common_address, _)| common_address);
    let Some(first) = addresses.next() else {
        return Ok(1);
    };
    if addresses.any(|address| address != first) {
        bail!(
            "one IEC 104 connection can target only one common address; use separate connections for multiple stations"
        );
    }
    Ok(first)
}

fn mapping_address(mapping: &TelemetryPointMapping) -> Result<(u16, u32)> {
    if mapping.address.kind != "iec104_ioa" {
        bail!(
            "IEC 104 point {} requires iec104_ioa address kind",
            mapping.point_id
        );
    }
    parse_iec104_point_address(&mapping.address.value)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("invalid IEC 104 address for point {}", mapping.point_id))
}

fn point_to_sample(
    mapping: &TelemetryPointMapping,
    point: &DataPoint,
    timezone_offset_minutes: i16,
) -> TelemetrySample {
    let decoded_timestamp = point
        .timestamp
        .and_then(|time| cp56_timestamp(time, timezone_offset_minutes));
    let timestamp_invalid = point.timestamp.is_some() && decoded_timestamp.is_none();
    let timestamp = decoded_timestamp.unwrap_or_else(Utc::now);
    let (value, decode_failed) = match telemetry_value(&point.value, mapping.value_type) {
        Ok(value) => (value, false),
        Err(_) => (default_value(mapping.value_type), true),
    };
    let quality_code = if decode_failed {
        DataQualityCode::BadDecode
    } else if point.quality.invalid {
        DataQualityCode::BadProtocol
    } else if point.quality.blocked {
        DataQualityCode::BadOutOfService
    } else if point.quality.substituted {
        DataQualityCode::UncertainSubstituted
    } else if point.quality.not_topical {
        DataQualityCode::UncertainLastKnown
    } else if point.quality.overflow {
        DataQualityCode::UncertainOverflow
    } else if point.quality.elapsed_time_invalid || timestamp_invalid {
        DataQualityCode::UncertainProtocol
    } else {
        DataQualityCode::Good
    };
    TelemetrySample::new(
        &mapping.device_id,
        &mapping.point_id,
        value,
        quality_code.quality(),
        timestamp,
    )
    .with_quality_code(quality_code)
}

fn telemetry_value(value: &DataValue, expected_type: TelemetryType) -> Result<TelemetryValue> {
    match expected_type {
        TelemetryType::Boolean => value
            .as_bool()
            .map(TelemetryValue::Boolean)
            .context("IEC 104 value is not boolean"),
        TelemetryType::Integer => {
            if let DataValue::Double(value) = value {
                return match value {
                    DoublePointValue::Off => Ok(TelemetryValue::Integer(1)),
                    DoublePointValue::On => Ok(TelemetryValue::Integer(2)),
                    DoublePointValue::Indeterminate => {
                        bail!("IEC 104 double-point value is indeterminate")
                    }
                    DoublePointValue::IndeterminateOrFaulty => {
                        bail!("IEC 104 double-point value is indeterminate or faulty")
                    }
                };
            }
            let value = value.as_f64().context("IEC 104 value is not numeric")?;
            if !value.is_finite() || value < i64::MIN as f64 || value > i64::MAX as f64 {
                bail!("IEC 104 numeric value is outside the integer range");
            }
            Ok(TelemetryValue::Integer(value as i64))
        }
        TelemetryType::Float => {
            let value = value.as_f64().context("IEC 104 value is not numeric")?;
            if !value.is_finite() {
                bail!("IEC 104 floating-point value is not finite");
            }
            Ok(TelemetryValue::Float(value))
        }
        TelemetryType::Text => bail!("IEC 104 monitoring values cannot be decoded as text"),
    }
}

fn default_value(value_type: TelemetryType) -> TelemetryValue {
    match value_type {
        TelemetryType::Boolean => TelemetryValue::Boolean(false),
        TelemetryType::Integer => TelemetryValue::Integer(0),
        TelemetryType::Float => TelemetryValue::Float(0.0),
        TelemetryType::Text => TelemetryValue::Text(String::new()),
    }
}

fn cp56_timestamp(time: Cp56Time2a, timezone_offset_minutes: i16) -> Option<DateTime<Utc>> {
    if time.invalid || time.milliseconds >= 60_000 || time.minutes >= 60 || time.hours >= 24 {
        return None;
    }
    let timezone = FixedOffset::east_opt(i32::from(timezone_offset_minutes) * 60)?;
    Some(
        timezone
            .with_ymd_and_hms(
                2000 + i32::from(time.year),
                u32::from(time.month),
                u32::from(time.day),
                u32::from(time.hours),
                u32::from(time.minutes),
                u32::from(time.milliseconds / 1000),
            )
            .single()?
            .with_nanosecond(u32::from(time.milliseconds % 1000) * 1_000_000)?
            .with_timezone(&Utc),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_mapping_preserves_protocol_detail() {
        let mapping = TelemetryPointMapping::new(
            "pressure",
            "pump-1",
            "pressure",
            "iec104-main",
            edge_core::PointAddress::iec104(1, 1001),
            TelemetryType::Float,
        );
        let point = DataPoint {
            ioa: 1001,
            value: DataValue::Float(12.5),
            quality: voltage_iec104::Quality {
                substituted: true,
                ..voltage_iec104::Quality::Good
            },
            timestamp: None,
        };
        let sample = point_to_sample(&mapping, &point, 0);
        assert_eq!(sample.value, TelemetryValue::Float(12.5));
        assert_eq!(
            sample.quality_code,
            Some(DataQualityCode::UncertainSubstituted)
        );
    }

    #[test]
    fn invalid_cp56_timestamp_downgrades_quality() {
        let mapping = TelemetryPointMapping::new(
            "running",
            "pump-1",
            "running",
            "iec104-main",
            edge_core::PointAddress::iec104(1, 1002),
            TelemetryType::Boolean,
        );
        let point = DataPoint {
            ioa: 1002,
            value: DataValue::Single(true),
            quality: voltage_iec104::Quality::Good,
            timestamp: Some(Cp56Time2a {
                milliseconds: 0,
                minutes: 0,
                hours: 0,
                day: 1,
                day_of_week: 1,
                month: 1,
                year: 26,
                invalid: true,
                summer_time: false,
            }),
        };
        let sample = point_to_sample(&mapping, &point, 0);
        assert_eq!(
            sample.quality_code,
            Some(DataQualityCode::UncertainProtocol)
        );
    }

    #[test]
    fn cp56_timestamp_uses_the_station_fixed_timezone_offset() {
        let timestamp = cp56_timestamp(
            Cp56Time2a {
                milliseconds: 10_250,
                minutes: 9,
                hours: 8,
                day: 16,
                day_of_week: 4,
                month: 7,
                year: 26,
                invalid: false,
                summer_time: false,
            },
            8 * 60,
        )
        .expect("valid CP56Time2a timestamp");

        assert_eq!(timestamp.to_rfc3339(), "2026-07-16T00:09:10.250+00:00");
    }

    #[test]
    fn integer_double_point_preserves_iec104_command_state_codes() {
        assert_eq!(
            telemetry_value(
                &DataValue::Double(DoublePointValue::Off),
                TelemetryType::Integer,
            )
            .unwrap(),
            TelemetryValue::Integer(1)
        );
        assert_eq!(
            telemetry_value(
                &DataValue::Double(DoublePointValue::On),
                TelemetryType::Integer,
            )
            .unwrap(),
            TelemetryValue::Integer(2)
        );
        assert!(telemetry_value(
            &DataValue::Double(DoublePointValue::IndeterminateOrFaulty),
            TelemetryType::Integer,
        )
        .is_err());
    }
}
