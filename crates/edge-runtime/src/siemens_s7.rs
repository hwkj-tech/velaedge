use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use edge_core::{
    parse_siemens_s7_endpoint, validate_siemens_s7_point, DataQuality, ProtocolConnection,
    ProtocolType, SiemensS7Area, SiemensS7DataType, SiemensS7PointAddress, TelemetryPointMapping,
    TelemetrySample, TelemetryType, TelemetryValue,
};
use snap7_client::proto::s7::header::{Area, TransportSize};
use snap7_client::transport::TcpTransport;
use snap7_client::{ConnectParams, MultiReadItem, S7Client};
use tokio::net::lookup_host;

use crate::{ProtocolAdapter, ProtocolCommandAdapter, ProtocolWriteResult};

/// Persistent Siemens S7Comm adapter for S7-300/400/1200-compatible endpoints.
pub struct SiemensS7Adapter {
    connection: ProtocolConnection,
    mappings: Vec<TelemetryPointMapping>,
    client: Option<S7Client<TcpTransport>>,
    connection_generation: u64,
}

impl SiemensS7Adapter {
    pub fn new(
        connection: ProtocolConnection,
        mappings: Vec<TelemetryPointMapping>,
    ) -> Result<Self> {
        if connection.protocol != ProtocolType::SiemensS7 {
            bail!("Siemens S7 adapter requires a Siemens S7 connection");
        }
        connection.validate().map_err(anyhow::Error::msg)?;
        validate_mappings(&connection, &mappings)?;
        Ok(Self {
            connection,
            mappings,
            client: None,
            connection_generation: 0,
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

    async fn client(&mut self) -> Result<&S7Client<TcpTransport>> {
        let connected = match self.client.as_ref() {
            Some(client) => client.is_connected().await,
            None => false,
        };
        if connected {
            return Ok(self.client.as_ref().expect("S7 client exists"));
        }
        self.client = None;

        let endpoint = self
            .connection
            .endpoint
            .as_deref()
            .context("Siemens S7 endpoint is required")?;
        let (host, port) = parse_siemens_s7_endpoint(endpoint).map_err(anyhow::Error::msg)?;
        let settings = self
            .connection
            .siemens_s7
            .as_ref()
            .context("Siemens S7 settings are required")?;
        let params = ConnectParams {
            rack: settings.rack,
            slot: settings.slot,
            pdu_size: settings.pdu_size,
            connect_timeout: Duration::from_millis(settings.connect_timeout_ms),
            request_timeout: Duration::from_millis(settings.request_timeout_ms),
        };
        let addresses = lookup_host((host.as_str(), port))
            .await
            .with_context(|| format!("failed to resolve Siemens S7 endpoint {host}:{port}"))?
            .collect::<Vec<_>>();
        if addresses.is_empty() {
            bail!("Siemens S7 endpoint {host}:{port} resolved to no addresses");
        }

        let mut last_error = None;
        for address in addresses {
            match connect(address, params.clone()).await {
                Ok(client) => {
                    self.connection_generation = self.connection_generation.saturating_add(1);
                    self.client = Some(client);
                    return Ok(self.client.as_ref().expect("S7 client initialized"));
                }
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow!("failed to connect Siemens S7 endpoint")))
    }

    fn clear_client(&mut self) {
        self.client = None;
    }
}

async fn connect(address: SocketAddr, params: ConnectParams) -> Result<S7Client<TcpTransport>> {
    S7Client::connect(address, params)
        .await
        .with_context(|| format!("failed to connect Siemens S7 endpoint {address}"))
}

#[async_trait]
impl ProtocolAdapter for SiemensS7Adapter {
    async fn read_telemetry(&mut self) -> Result<Vec<TelemetrySample>> {
        let targets = self
            .mappings
            .iter()
            .filter(|mapping| mapping.access.is_readable())
            .map(|mapping| {
                let address =
                    validate_siemens_s7_point(&mapping.address, mapping.value_type, mapping.access)
                        .map_err(anyhow::Error::msg)
                        .with_context(|| {
                            format!("invalid Siemens S7 point {}", mapping.point_id)
                        })?;
                Ok((mapping.clone(), address))
            })
            .collect::<Result<Vec<_>>>()?;
        if targets.is_empty() {
            return Ok(Vec::new());
        }

        let requests = targets
            .iter()
            .map(|(_, address)| MultiReadItem {
                area: s7_area(address.area),
                db_number: address.db_number,
                start: address.byte_offset,
                length: address.data_type.byte_width(),
                transport: TransportSize::Byte,
            })
            .collect::<Vec<_>>();
        let result = {
            let client = self.client().await?;
            client
                .read_multi_vars(&requests)
                .await
                .context("Siemens S7 multi-variable read failed")
        };
        if result.is_err() {
            self.clear_client();
        }
        let values = result?;
        if values.len() != targets.len() {
            bail!(
                "Siemens S7 read response count mismatch: requested {}, received {}",
                targets.len(),
                values.len()
            );
        }

        targets
            .into_iter()
            .zip(values)
            .map(|((mapping, address), bytes)| {
                let value = decode_value(&mapping, address, &bytes)?;
                Ok(TelemetrySample::new(
                    mapping.device_id,
                    mapping.point_id,
                    value,
                    DataQuality::Good,
                    Utc::now(),
                ))
            })
            .collect()
    }
}

#[async_trait]
impl ProtocolCommandAdapter for SiemensS7Adapter {
    async fn write_point(
        &mut self,
        mapping: &TelemetryPointMapping,
        value: TelemetryValue,
    ) -> Result<ProtocolWriteResult> {
        if mapping.protocol_connection_id != self.connection.connection_id {
            bail!(
                "Siemens S7 point {} references connection {} instead of {}",
                mapping.point_id,
                mapping.protocol_connection_id,
                self.connection.connection_id
            );
        }
        if !mapping.access.is_writable() {
            bail!("Siemens S7 point {} is not writable", mapping.point_id);
        }
        let address =
            validate_siemens_s7_point(&mapping.address, mapping.value_type, mapping.access)
                .map_err(anyhow::Error::msg)
                .with_context(|| format!("invalid Siemens S7 point {}", mapping.point_id))?;

        let result = {
            let client = self.client().await?;
            write_value(client, address, &value)
                .await
                .with_context(|| format!("failed to write Siemens S7 point {}", mapping.point_id))
        };
        if result.is_err() {
            self.clear_client();
        }
        result?;
        Ok(ProtocolWriteResult {
            point_id: mapping.point_id.clone(),
            value,
            verified: true,
            readback_value: None,
        })
    }
}

fn validate_mappings(
    connection: &ProtocolConnection,
    mappings: &[TelemetryPointMapping],
) -> Result<()> {
    for mapping in mappings {
        if mapping.protocol_connection_id != connection.connection_id {
            bail!(
                "Siemens S7 point {} references connection {} instead of {}",
                mapping.point_id,
                mapping.protocol_connection_id,
                connection.connection_id
            );
        }
        validate_siemens_s7_point(&mapping.address, mapping.value_type, mapping.access)
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("invalid Siemens S7 point {}", mapping.point_id))?;
    }
    Ok(())
}

fn s7_area(area: SiemensS7Area) -> Area {
    match area {
        SiemensS7Area::DataBlock => Area::DataBlock,
        SiemensS7Area::Marker => Area::Marker,
        SiemensS7Area::ProcessInput => Area::ProcessInput,
        SiemensS7Area::ProcessOutput => Area::ProcessOutput,
    }
}

fn decode_value(
    mapping: &TelemetryPointMapping,
    address: SiemensS7PointAddress,
    bytes: &[u8],
) -> Result<TelemetryValue> {
    let expected = usize::from(address.data_type.byte_width());
    if bytes.len() < expected {
        bail!(
            "Siemens S7 point {} expected {expected} bytes but received {}",
            mapping.point_id,
            bytes.len()
        );
    }
    let value = match address.data_type {
        SiemensS7DataType::Bit => {
            let bit = address.bit_offset.expect("validated S7 bit offset");
            TelemetryValue::Boolean(bytes[0] & (1 << bit) != 0)
        }
        SiemensS7DataType::Byte => TelemetryValue::Integer(i64::from(bytes[0])),
        SiemensS7DataType::Word => {
            TelemetryValue::Integer(i64::from(u16::from_be_bytes([bytes[0], bytes[1]])))
        }
        SiemensS7DataType::Int => {
            TelemetryValue::Integer(i64::from(i16::from_be_bytes([bytes[0], bytes[1]])))
        }
        SiemensS7DataType::DWord if mapping.value_type == TelemetryType::Float => {
            TelemetryValue::Float(f64::from(f32::from_be_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3],
            ])))
        }
        SiemensS7DataType::DWord => TelemetryValue::Integer(i64::from(u32::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3],
        ]))),
        SiemensS7DataType::DInt => TelemetryValue::Integer(i64::from(i32::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3],
        ]))),
        SiemensS7DataType::Real => TelemetryValue::Float(f64::from(f32::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3],
        ]))),
    };
    Ok(value)
}

async fn write_value(
    client: &S7Client<TcpTransport>,
    address: SiemensS7PointAddress,
    value: &TelemetryValue,
) -> Result<()> {
    let area = s7_area(address.area);
    if address.data_type == SiemensS7DataType::Bit {
        let TelemetryValue::Boolean(value) = value else {
            bail!("Siemens S7 bit write requires a boolean value");
        };
        let current = client
            .read_area(
                area,
                address.db_number,
                address.byte_offset,
                1,
                TransportSize::Byte,
            )
            .await
            .context("Siemens S7 bit read-before-write failed")?;
        let mut byte = *current
            .first()
            .context("Siemens S7 bit read-before-write returned no data")?;
        let mask = 1_u8 << address.bit_offset.expect("validated S7 bit offset");
        if *value {
            byte |= mask;
        } else {
            byte &= !mask;
        }
        return client
            .write_area(
                area,
                address.db_number,
                address.byte_offset,
                TransportSize::Byte,
                &[byte],
            )
            .await
            .context("Siemens S7 bit write failed");
    }

    let bytes = encode_value(address.data_type, value)?;
    client
        .write_area(
            area,
            address.db_number,
            address.byte_offset,
            TransportSize::Byte,
            &bytes,
        )
        .await
        .context("Siemens S7 value write failed")
}

fn encode_value(data_type: SiemensS7DataType, value: &TelemetryValue) -> Result<Vec<u8>> {
    match (data_type, value) {
        (SiemensS7DataType::Byte, TelemetryValue::Integer(value)) => Ok(vec![
            u8::try_from(*value).context("Siemens S7 BYTE value is outside 0..255")?
        ]),
        (SiemensS7DataType::Word, TelemetryValue::Integer(value)) => Ok(u16::try_from(*value)
            .context("Siemens S7 WORD value is outside 0..65535")?
            .to_be_bytes()
            .to_vec()),
        (SiemensS7DataType::Int, TelemetryValue::Integer(value)) => Ok(i16::try_from(*value)
            .context("Siemens S7 INT value is outside -32768..32767")?
            .to_be_bytes()
            .to_vec()),
        (SiemensS7DataType::DWord, TelemetryValue::Integer(value)) => Ok(u32::try_from(*value)
            .context("Siemens S7 DWORD value is outside 0..4294967295")?
            .to_be_bytes()
            .to_vec()),
        (SiemensS7DataType::DInt, TelemetryValue::Integer(value)) => Ok(i32::try_from(*value)
            .context("Siemens S7 DINT value is outside the signed 32-bit range")?
            .to_be_bytes()
            .to_vec()),
        (SiemensS7DataType::DWord | SiemensS7DataType::Real, TelemetryValue::Float(value)) => {
            if !value.is_finite() || *value > f32::MAX as f64 || *value < f32::MIN as f64 {
                bail!("Siemens S7 REAL value must be finite and fit in 32 bits");
            }
            Ok((*value as f32).to_be_bytes().to_vec())
        }
        (SiemensS7DataType::Bit, _) => {
            unreachable!("Siemens S7 bit values are handled before encoding")
        }
        (_, _) => bail!("telemetry value is incompatible with Siemens S7 data type"),
    }
}

#[cfg(test)]
mod tests {
    use edge_core::{PointAccess, PointAddress};

    use super::*;

    fn mapping(address: &str, value_type: TelemetryType) -> TelemetryPointMapping {
        TelemetryPointMapping::new(
            "point-1",
            "device-1",
            "value",
            "s7-main",
            PointAddress::siemens_s7(address),
            value_type,
        )
        .with_access(PointAccess::ReadWrite)
    }

    #[test]
    fn value_codec_is_big_endian_and_range_checked() {
        let mapping = mapping("DB1.DINT4", TelemetryType::Integer);
        let address =
            validate_siemens_s7_point(&mapping.address, mapping.value_type, mapping.access)
                .unwrap();
        assert_eq!(
            decode_value(&mapping, address, &(-42_i32).to_be_bytes()).unwrap(),
            TelemetryValue::Integer(-42)
        );
        assert_eq!(
            encode_value(SiemensS7DataType::Word, &TelemetryValue::Integer(65_535)).unwrap(),
            [0xff, 0xff]
        );
        assert!(encode_value(SiemensS7DataType::Word, &TelemetryValue::Integer(65_536)).is_err());
    }
}
