use std::collections::{BTreeMap, BTreeSet};
use std::net::{Ipv4Addr, SocketAddrV4};

use serde::{Deserialize, Serialize};

use crate::{AlgorithmSpec, DeviceSpec, NumberRange, TelemetryType};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EdgeConfigPackage {
    pub edge_id: String,
    pub version: String,
    pub device_models: Vec<DeviceSpec>,
    pub devices: Vec<DeviceInstance>,
    pub protocol_connections: Vec<ProtocolConnection>,
    #[serde(default)]
    pub mqtt_uplinks: Vec<MqttUplinkConfig>,
    #[serde(default)]
    pub data_configs: Vec<DataConfig>,
    #[serde(default)]
    pub command_flows: Vec<CommandFlowConfig>,
    pub point_mappings: Vec<TelemetryPointMapping>,
    pub collection_tasks: Vec<CollectionTask>,
    pub algorithms: Vec<AlgorithmSpec>,
}

impl EdgeConfigPackage {
    pub fn new(edge_id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            edge_id: edge_id.into(),
            version: version.into(),
            device_models: Vec::new(),
            devices: Vec::new(),
            protocol_connections: Vec::new(),
            mqtt_uplinks: Vec::new(),
            data_configs: Vec::new(),
            command_flows: Vec::new(),
            point_mappings: Vec::new(),
            collection_tasks: Vec::new(),
            algorithms: Vec::new(),
        }
    }

    pub fn with_device(mut self, device: DeviceInstance) -> Self {
        self.devices.push(device);
        self
    }

    pub fn with_protocol_connection(mut self, connection: ProtocolConnection) -> Self {
        self.protocol_connections.push(connection);
        self
    }

    pub fn with_mqtt_uplink(mut self, uplink: MqttUplinkConfig) -> Self {
        self.mqtt_uplinks.push(uplink);
        self
    }

    pub fn with_data_config(mut self, data_config: DataConfig) -> Self {
        self.data_configs.push(data_config);
        self
    }

    pub fn with_command_flow(mut self, command_flow: CommandFlowConfig) -> Self {
        self.command_flows.push(command_flow);
        self
    }

    pub fn with_point_mapping(mut self, mapping: TelemetryPointMapping) -> Self {
        self.point_mappings.push(mapping);
        self
    }

    pub fn with_collection_task(mut self, task: CollectionTask) -> Self {
        self.collection_tasks.push(task);
        self
    }

    pub fn with_algorithm(mut self, algorithm: AlgorithmSpec) -> Self {
        self.algorithms.push(algorithm);
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceInstance {
    pub device_id: String,
    pub device_type: String,
}

impl DeviceInstance {
    pub fn new(device_id: impl Into<String>, device_type: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            device_type: device_type.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolConnection {
    pub connection_id: String,
    pub protocol: ProtocolType,
    pub endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<SerialConnectionSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iec101: Option<Iec101ConnectionSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iec104: Option<Iec104ConnectionSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opc_ua: Option<OpcUaConnectionSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bacnet_ip: Option<BacnetIpConnectionSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub siemens_s7: Option<SiemensS7ConnectionSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub omron_fins: Option<OmronFinsConnectionSettings>,
    #[serde(
        default,
        skip_serializing_if = "ProtocolCircuitBreakerConfig::is_default"
    )]
    pub circuit_breaker: ProtocolCircuitBreakerConfig,
}

impl ProtocolConnection {
    pub fn simulated(connection_id: impl Into<String>) -> Self {
        Self {
            connection_id: connection_id.into(),
            protocol: ProtocolType::Simulated,
            endpoint: None,
            serial: None,
            iec101: None,
            iec104: None,
            opc_ua: None,
            bacnet_ip: None,
            siemens_s7: None,
            omron_fins: None,
            circuit_breaker: ProtocolCircuitBreakerConfig::default(),
        }
    }

    pub fn modbus_rtu_serial(
        connection_id: impl Into<String>,
        serial: SerialConnectionSettings,
    ) -> Self {
        Self {
            connection_id: connection_id.into(),
            protocol: ProtocolType::ModbusRtu,
            endpoint: Some(serial.port.clone()),
            serial: Some(serial),
            iec101: None,
            iec104: None,
            opc_ua: None,
            bacnet_ip: None,
            siemens_s7: None,
            omron_fins: None,
            circuit_breaker: ProtocolCircuitBreakerConfig::default(),
        }
    }

    pub fn modbus_tcp(connection_id: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            connection_id: connection_id.into(),
            protocol: ProtocolType::ModbusTcp,
            endpoint: Some(endpoint.into()),
            serial: None,
            iec101: None,
            iec104: None,
            opc_ua: None,
            bacnet_ip: None,
            siemens_s7: None,
            omron_fins: None,
            circuit_breaker: ProtocolCircuitBreakerConfig::default(),
        }
    }

    pub fn dlt645_serial(
        connection_id: impl Into<String>,
        serial: SerialConnectionSettings,
    ) -> Self {
        Self {
            connection_id: connection_id.into(),
            protocol: ProtocolType::Dlt645,
            endpoint: Some(serial.port.clone()),
            serial: Some(serial),
            iec101: None,
            iec104: None,
            opc_ua: None,
            bacnet_ip: None,
            siemens_s7: None,
            omron_fins: None,
            circuit_breaker: ProtocolCircuitBreakerConfig::default(),
        }
    }

    pub fn iec101_serial(
        connection_id: impl Into<String>,
        serial: SerialConnectionSettings,
    ) -> Self {
        Self {
            connection_id: connection_id.into(),
            protocol: ProtocolType::Iec101,
            endpoint: Some(serial.port.clone()),
            serial: Some(serial),
            iec101: Some(Iec101ConnectionSettings::default()),
            iec104: None,
            opc_ua: None,
            bacnet_ip: None,
            siemens_s7: None,
            omron_fins: None,
            circuit_breaker: ProtocolCircuitBreakerConfig::default(),
        }
    }

    pub fn iec104(connection_id: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            connection_id: connection_id.into(),
            protocol: ProtocolType::Iec104,
            endpoint: Some(endpoint.into()),
            serial: None,
            iec101: None,
            iec104: Some(Iec104ConnectionSettings::default()),
            opc_ua: None,
            bacnet_ip: None,
            siemens_s7: None,
            omron_fins: None,
            circuit_breaker: ProtocolCircuitBreakerConfig::default(),
        }
    }

    pub fn with_circuit_breaker(mut self, circuit_breaker: ProtocolCircuitBreakerConfig) -> Self {
        self.circuit_breaker = circuit_breaker;
        self
    }

    pub fn with_iec104_settings(mut self, settings: Iec104ConnectionSettings) -> Self {
        self.iec104 = Some(settings);
        self
    }

    pub fn with_iec101_settings(mut self, settings: Iec101ConnectionSettings) -> Self {
        self.iec101 = Some(settings);
        self
    }

    pub fn opc_ua(
        connection_id: impl Into<String>,
        endpoint: impl Into<String>,
        settings: OpcUaConnectionSettings,
    ) -> Self {
        Self {
            connection_id: connection_id.into(),
            protocol: ProtocolType::OpcUa,
            endpoint: Some(endpoint.into()),
            serial: None,
            iec101: None,
            iec104: None,
            opc_ua: Some(settings),
            bacnet_ip: None,
            siemens_s7: None,
            omron_fins: None,
            circuit_breaker: ProtocolCircuitBreakerConfig::default(),
        }
    }

    pub fn bacnet_ip(
        connection_id: impl Into<String>,
        endpoint: Option<impl Into<String>>,
        settings: BacnetIpConnectionSettings,
    ) -> Self {
        Self {
            connection_id: connection_id.into(),
            protocol: ProtocolType::BacnetIp,
            endpoint: endpoint.map(Into::into),
            serial: None,
            iec101: None,
            iec104: None,
            opc_ua: None,
            bacnet_ip: Some(settings),
            siemens_s7: None,
            omron_fins: None,
            circuit_breaker: ProtocolCircuitBreakerConfig::default(),
        }
    }

    pub fn siemens_s7(
        connection_id: impl Into<String>,
        endpoint: impl Into<String>,
        settings: SiemensS7ConnectionSettings,
    ) -> Self {
        Self {
            connection_id: connection_id.into(),
            protocol: ProtocolType::SiemensS7,
            endpoint: Some(endpoint.into()),
            serial: None,
            iec101: None,
            iec104: None,
            opc_ua: None,
            bacnet_ip: None,
            siemens_s7: Some(settings),
            omron_fins: None,
            circuit_breaker: ProtocolCircuitBreakerConfig::default(),
        }
    }

    pub fn omron_fins(
        connection_id: impl Into<String>,
        endpoint: impl Into<String>,
        settings: OmronFinsConnectionSettings,
    ) -> Self {
        Self {
            connection_id: connection_id.into(),
            protocol: ProtocolType::OmronFins,
            endpoint: Some(endpoint.into()),
            serial: None,
            iec101: None,
            iec104: None,
            opc_ua: None,
            bacnet_ip: None,
            siemens_s7: None,
            omron_fins: Some(settings),
            circuit_breaker: ProtocolCircuitBreakerConfig::default(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        self.circuit_breaker.validate()?;
        match self.protocol {
            ProtocolType::Iec101 => self.iec101.unwrap_or_default().validate(),
            ProtocolType::Iec104 => {
                let endpoint = self
                    .endpoint
                    .as_deref()
                    .ok_or_else(|| "IEC 104 endpoint is required".to_string())?;
                validate_iec104_endpoint(endpoint)?;
                self.iec104.unwrap_or_default().validate()
            }
            ProtocolType::OpcUa => {
                let endpoint = self
                    .endpoint
                    .as_deref()
                    .ok_or_else(|| "OPC UA endpoint is required".to_string())?;
                if !endpoint.starts_with("opc.tcp://") {
                    return Err("OPC UA endpoint must use opc.tcp://".to_string());
                }
                self.opc_ua
                    .as_ref()
                    .ok_or_else(|| "OPC UA settings are required".to_string())?
                    .validate()
            }
            ProtocolType::BacnetIp => {
                if let Some(endpoint) = self.endpoint.as_deref() {
                    parse_bacnet_ip_endpoint(endpoint)?;
                }
                self.bacnet_ip
                    .as_ref()
                    .ok_or_else(|| "BACnet/IP settings are required".to_string())?
                    .validate()
            }
            ProtocolType::SiemensS7 => {
                let endpoint = self
                    .endpoint
                    .as_deref()
                    .ok_or_else(|| "Siemens S7 endpoint is required".to_string())?;
                parse_siemens_s7_endpoint(endpoint)?;
                self.siemens_s7
                    .as_ref()
                    .ok_or_else(|| "Siemens S7 settings are required".to_string())?
                    .validate()
            }
            ProtocolType::OmronFins => {
                let endpoint = self
                    .endpoint
                    .as_deref()
                    .ok_or_else(|| "Omron FINS endpoint is required".to_string())?;
                parse_omron_fins_endpoint(endpoint)?;
                self.omron_fins
                    .as_ref()
                    .ok_or_else(|| "Omron FINS settings are required".to_string())?
                    .validate()
            }
            _ if self.iec101.is_some() => {
                Err("IEC 101 settings are only valid for IEC 101 connections".to_string())
            }
            _ if self.iec104.is_some() => {
                Err("IEC 104 settings are only valid for IEC 104 connections".to_string())
            }
            _ if self.opc_ua.is_some() => {
                Err("OPC UA settings are only valid for OPC UA connections".to_string())
            }
            _ if self.bacnet_ip.is_some() => {
                Err("BACnet/IP settings are only valid for BACnet/IP connections".to_string())
            }
            _ if self.siemens_s7.is_some() => {
                Err("Siemens S7 settings are only valid for Siemens S7 connections".to_string())
            }
            _ if self.omron_fins.is_some() => {
                Err("Omron FINS settings are only valid for Omron FINS connections".to_string())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Iec101ConnectionSettings {
    #[serde(
        rename = "cp56TimeZoneOffsetMinutes",
        alias = "cp56TimezoneOffsetMinutes"
    )]
    pub cp56_timezone_offset_minutes: i16,
}

impl Iec101ConnectionSettings {
    pub fn with_cp56_timezone_offset_minutes(mut self, offset_minutes: i16) -> Self {
        self.cp56_timezone_offset_minutes = offset_minutes;
        self
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(-14 * 60..=14 * 60).contains(&self.cp56_timezone_offset_minutes) {
            return Err(
                "IEC 101 CP56Time2a timezone offset must be between -840 and 840 minutes"
                    .to_string(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Iec104ConnectionSettings {
    #[serde(
        rename = "cp56TimeZoneOffsetMinutes",
        alias = "cp56TimezoneOffsetMinutes"
    )]
    pub cp56_timezone_offset_minutes: i16,
}

impl Iec104ConnectionSettings {
    pub fn with_cp56_timezone_offset_minutes(mut self, offset_minutes: i16) -> Self {
        self.cp56_timezone_offset_minutes = offset_minutes;
        self
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(-14 * 60..=14 * 60).contains(&self.cp56_timezone_offset_minutes) {
            return Err(
                "IEC 104 CP56Time2a timezone offset must be between -840 and 840 minutes"
                    .to_string(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OmronFinsWordOrder {
    HighWordFirst,
    #[default]
    LowWordFirst,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OmronFinsTransport {
    #[default]
    Udp,
    Tcp,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct OmronFinsConnectionSettings {
    pub transport: OmronFinsTransport,
    pub source_network: u8,
    pub source_node: u8,
    pub source_unit: u8,
    pub destination_network: u8,
    pub destination_node: u8,
    pub destination_unit: u8,
    pub timeout_ms: u64,
    pub word_order: OmronFinsWordOrder,
}

impl Default for OmronFinsConnectionSettings {
    fn default() -> Self {
        Self {
            transport: OmronFinsTransport::Udp,
            source_network: 0,
            source_node: 1,
            source_unit: 0,
            destination_network: 0,
            destination_node: 0,
            destination_unit: 0,
            timeout_ms: 2_000,
            word_order: OmronFinsWordOrder::LowWordFirst,
        }
    }
}

impl OmronFinsConnectionSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.source_network > 127 || self.destination_network > 127 {
            return Err("Omron FINS network address must be between 0 and 127".to_string());
        }
        if self.source_node == u8::MAX
            || (self.transport == OmronFinsTransport::Udp && self.source_node == 0)
        {
            return Err(match self.transport {
                OmronFinsTransport::Udp => {
                    "Omron FINS/UDP source node must be between 1 and 254".to_string()
                }
                OmronFinsTransport::Tcp => {
                    "Omron FINS/TCP source node must be between 0 and 254".to_string()
                }
            });
        }
        if self.destination_node == u8::MAX {
            return Err("Omron FINS destination node must be between 0 and 254".to_string());
        }
        if !(100..=120_000).contains(&self.timeout_ms) {
            return Err("Omron FINS timeout must be between 100 and 120000 ms".to_string());
        }
        Ok(())
    }
}

pub fn parse_omron_fins_endpoint(value: &str) -> Result<(String, u16), String> {
    let value = value.trim();
    let value = value.strip_prefix("fins://").unwrap_or(value);
    let (host, port) = value
        .rsplit_once(':')
        .ok_or_else(|| "Omron FINS endpoint must be host:port or fins://host:port".to_string())?;
    let host = host.trim().trim_matches(['[', ']']);
    if host.is_empty() || host.chars().any(char::is_whitespace) {
        return Err("Omron FINS endpoint host is required".to_string());
    }
    let port = port
        .parse::<u16>()
        .map_err(|_| "Omron FINS endpoint port must be between 1 and 65535".to_string())?;
    if port == 0 {
        return Err("Omron FINS endpoint port must be between 1 and 65535".to_string());
    }
    Ok((host.to_string(), port))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct SiemensS7ConnectionSettings {
    pub rack: u8,
    pub slot: u8,
    pub pdu_size: u16,
    pub connect_timeout_ms: u64,
    pub request_timeout_ms: u64,
}

impl Default for SiemensS7ConnectionSettings {
    fn default() -> Self {
        Self {
            rack: 0,
            slot: 1,
            pdu_size: 480,
            connect_timeout_ms: 5_000,
            request_timeout_ms: 10_000,
        }
    }
}

impl SiemensS7ConnectionSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.rack > 7 {
            return Err("Siemens S7 rack must be between 0 and 7".to_string());
        }
        if self.slot > 31 {
            return Err("Siemens S7 slot must be between 0 and 31".to_string());
        }
        if ![240, 480, 960].contains(&self.pdu_size) {
            return Err("Siemens S7 PDU size must be 240, 480 or 960 bytes".to_string());
        }
        if !(100..=120_000).contains(&self.connect_timeout_ms) {
            return Err("Siemens S7 connect timeout must be between 100 and 120000 ms".to_string());
        }
        if !(100..=120_000).contains(&self.request_timeout_ms) {
            return Err("Siemens S7 request timeout must be between 100 and 120000 ms".to_string());
        }
        Ok(())
    }
}

pub fn parse_siemens_s7_endpoint(value: &str) -> Result<(String, u16), String> {
    let value = value.trim();
    let value = value.strip_prefix("s7://").unwrap_or(value);
    let (host, port) = value
        .rsplit_once(':')
        .ok_or_else(|| "Siemens S7 endpoint must be host:port or s7://host:port".to_string())?;
    let host = host.trim().trim_matches(['[', ']']);
    if host.is_empty() || host.chars().any(char::is_whitespace) {
        return Err("Siemens S7 endpoint host is required".to_string());
    }
    let port = port
        .parse::<u16>()
        .map_err(|_| "Siemens S7 endpoint port must be between 1 and 65535".to_string())?;
    if port == 0 {
        return Err("Siemens S7 endpoint port must be between 1 and 65535".to_string());
    }
    Ok((host.to_string(), port))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct BacnetIpConnectionSettings {
    pub bind_address: String,
    pub local_port: u16,
    pub broadcast_address: String,
    pub apdu_timeout_ms: u64,
    pub apdu_retries: u8,
    pub discovery_timeout_ms: u64,
    pub max_apdu_length: u16,
    pub foreign_device: Option<BacnetForeignDeviceSettings>,
    pub cov: Option<BacnetCovSettings>,
}

impl Default for BacnetIpConnectionSettings {
    fn default() -> Self {
        Self {
            bind_address: Ipv4Addr::UNSPECIFIED.to_string(),
            local_port: 0,
            broadcast_address: Ipv4Addr::BROADCAST.to_string(),
            apdu_timeout_ms: 3_000,
            apdu_retries: 3,
            discovery_timeout_ms: 1_000,
            max_apdu_length: 1_476,
            foreign_device: None,
            cov: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BacnetForeignDeviceSettings {
    pub bbmd_address: String,
    pub ttl_seconds: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BacnetCovSettings {
    pub lifetime_seconds: u32,
    pub confirmed_notifications: bool,
    pub fallback_poll_interval_ms: u64,
}

impl BacnetIpConnectionSettings {
    pub fn validate(&self) -> Result<(), String> {
        self.bind_address
            .parse::<Ipv4Addr>()
            .map_err(|_| "BACnet/IP bind address must be an IPv4 address".to_string())?;
        self.broadcast_address
            .parse::<Ipv4Addr>()
            .map_err(|_| "BACnet/IP broadcast address must be an IPv4 address".to_string())?;
        if !(100..=120_000).contains(&self.apdu_timeout_ms) {
            return Err("BACnet/IP APDU timeout must be between 100 and 120000 ms".to_string());
        }
        if self.apdu_retries > 10 {
            return Err("BACnet/IP APDU retries cannot exceed 10".to_string());
        }
        if !(100..=30_000).contains(&self.discovery_timeout_ms) {
            return Err("BACnet/IP discovery timeout must be between 100 and 30000 ms".to_string());
        }
        if ![50, 128, 206, 480, 1_024, 1_476].contains(&self.max_apdu_length) {
            return Err(
                "BACnet/IP max APDU length must be 50, 128, 206, 480, 1024 or 1476".to_string(),
            );
        }
        if let Some(foreign_device) = &self.foreign_device {
            parse_bacnet_ip_endpoint(&foreign_device.bbmd_address)
                .map_err(|_| "BACnet/IP BBMD address must be an IPv4 host:port".to_string())?;
            if !(30..=u16::MAX).contains(&foreign_device.ttl_seconds) {
                return Err(
                    "BACnet/IP foreign device TTL must be between 30 and 65535 seconds".to_string(),
                );
            }
        }
        if let Some(cov) = &self.cov {
            if !(60..=86_400).contains(&cov.lifetime_seconds) {
                return Err(
                    "BACnet/IP COV lifetime must be between 60 and 86400 seconds".to_string(),
                );
            }
            if !(1_000..=3_600_000).contains(&cov.fallback_poll_interval_ms) {
                return Err(
                    "BACnet/IP COV fallback poll interval must be between 1000 and 3600000 ms"
                        .to_string(),
                );
            }
        }
        Ok(())
    }
}

pub fn parse_bacnet_ip_endpoint(value: &str) -> Result<SocketAddrV4, String> {
    let value = value.trim();
    let value = value.strip_prefix("bacnet://").unwrap_or(value);
    value.parse::<SocketAddrV4>().map_err(|_| {
        "BACnet/IP endpoint must be an IPv4 host:port or bacnet://host:port".to_string()
    })
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpcUaSecurityPolicy {
    #[default]
    None,
    Basic256Sha256,
    Aes128Sha256RsaOaep,
    Aes256Sha256RsaPss,
}

impl OpcUaSecurityPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Basic256Sha256 => "Basic256Sha256",
            Self::Aes128Sha256RsaOaep => "Aes128_Sha256_RsaOaep",
            Self::Aes256Sha256RsaPss => "Aes256_Sha256_RsaPss",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpcUaMessageSecurityMode {
    #[default]
    None,
    Sign,
    SignAndEncrypt,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpcUaAuthMode {
    #[default]
    Anonymous,
    Username,
    X509,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct OpcUaConnectionSettings {
    pub security_policy: OpcUaSecurityPolicy,
    pub message_security_mode: OpcUaMessageSecurityMode,
    pub auth_mode: OpcUaAuthMode,
    pub username: Option<String>,
    pub password_env: Option<String>,
    pub user_certificate_path: Option<String>,
    pub user_private_key_path: Option<String>,
    pub pki_dir: String,
    pub trust_server_certs: bool,
    pub verify_server_certs: bool,
    pub connect_timeout_ms: u64,
    pub request_timeout_ms: u64,
    pub session_timeout_ms: u32,
    pub session_retry_limit: u32,
}

impl Default for OpcUaConnectionSettings {
    fn default() -> Self {
        Self {
            security_policy: OpcUaSecurityPolicy::None,
            message_security_mode: OpcUaMessageSecurityMode::None,
            auth_mode: OpcUaAuthMode::Anonymous,
            username: None,
            password_env: None,
            user_certificate_path: None,
            user_private_key_path: None,
            pki_dir: "./data/opcua-pki".to_string(),
            trust_server_certs: false,
            verify_server_certs: true,
            connect_timeout_ms: 5_000,
            request_timeout_ms: 5_000,
            session_timeout_ms: 60_000,
            session_retry_limit: 3,
        }
    }
}

impl OpcUaConnectionSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.security_policy == OpcUaSecurityPolicy::None
            && self.message_security_mode != OpcUaMessageSecurityMode::None
        {
            return Err("OPC UA None security policy requires None message mode".to_string());
        }
        if self.security_policy != OpcUaSecurityPolicy::None
            && self.message_security_mode == OpcUaMessageSecurityMode::None
        {
            return Err("secured OPC UA policy requires Sign or SignAndEncrypt mode".to_string());
        }
        match self.auth_mode {
            OpcUaAuthMode::Anonymous => {
                if self.username.is_some()
                    || self.password_env.is_some()
                    || self.user_certificate_path.is_some()
                    || self.user_private_key_path.is_some()
                {
                    return Err(
                        "anonymous OPC UA authentication cannot define credentials".to_string()
                    );
                }
            }
            OpcUaAuthMode::Username => {
                required_non_empty(&self.username, "OPC UA username")?;
                required_non_empty(&self.password_env, "OPC UA password environment variable")?;
                if self.user_certificate_path.is_some() || self.user_private_key_path.is_some() {
                    return Err(
                        "username OPC UA authentication cannot define X.509 credentials"
                            .to_string(),
                    );
                }
            }
            OpcUaAuthMode::X509 => {
                required_non_empty(&self.user_certificate_path, "OPC UA user certificate path")?;
                required_non_empty(&self.user_private_key_path, "OPC UA user private key path")?;
                if self.username.is_some() || self.password_env.is_some() {
                    return Err(
                        "X.509 OPC UA authentication cannot define username credentials"
                            .to_string(),
                    );
                }
            }
        }
        if self.pki_dir.trim().is_empty() {
            return Err("OPC UA PKI directory is required".to_string());
        }
        if !(100..=120_000).contains(&self.connect_timeout_ms) {
            return Err("OPC UA connect timeout must be between 100 and 120000 ms".to_string());
        }
        if !(100..=120_000).contains(&self.request_timeout_ms) {
            return Err("OPC UA request timeout must be between 100 and 120000 ms".to_string());
        }
        if !(1_000..=3_600_000).contains(&self.session_timeout_ms) {
            return Err("OPC UA session timeout must be between 1000 and 3600000 ms".to_string());
        }
        if self.session_retry_limit > 100 {
            return Err("OPC UA session retry limit must not exceed 100".to_string());
        }
        Ok(())
    }
}

fn required_non_empty(value: &Option<String>, label: &str) -> Result<(), String> {
    if value.as_deref().is_none_or(|value| value.trim().is_empty()) {
        return Err(format!("{label} is required"));
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ProtocolCircuitBreakerConfig {
    pub enabled: bool,
    pub failure_threshold: u32,
    pub open_duration_ms: u64,
    pub half_open_success_threshold: u32,
}

impl Default for ProtocolCircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            failure_threshold: 5,
            open_duration_ms: 30_000,
            half_open_success_threshold: 1,
        }
    }
}

impl ProtocolCircuitBreakerConfig {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(1..=MAX_PROTOCOL_CIRCUIT_FAILURE_THRESHOLD).contains(&self.failure_threshold) {
            return Err(format!(
                "circuit breaker failure threshold must be between 1 and {}",
                MAX_PROTOCOL_CIRCUIT_FAILURE_THRESHOLD
            ));
        }
        if !(MIN_PROTOCOL_CIRCUIT_OPEN_DURATION_MS..=MAX_PROTOCOL_CIRCUIT_OPEN_DURATION_MS)
            .contains(&self.open_duration_ms)
        {
            return Err(format!(
                "circuit breaker open duration must be between {} and {} ms",
                MIN_PROTOCOL_CIRCUIT_OPEN_DURATION_MS, MAX_PROTOCOL_CIRCUIT_OPEN_DURATION_MS
            ));
        }
        if !(1..=MAX_PROTOCOL_CIRCUIT_HALF_OPEN_SUCCESSES)
            .contains(&self.half_open_success_threshold)
        {
            return Err(format!(
                "circuit breaker half-open success threshold must be between 1 and {}",
                MAX_PROTOCOL_CIRCUIT_HALF_OPEN_SUCCESSES
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProtocolType {
    Simulated,
    ModbusTcp,
    ModbusRtu,
    Dlt645,
    Iec101,
    Iec104,
    CustomSerial,
    OpcUa,
    BacnetIp,
    SiemensS7,
    OmronFins,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SerialConnectionSettings {
    pub port: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub stop_bits: u8,
    pub parity: String,
}

impl SerialConnectionSettings {
    pub fn new(port: impl Into<String>, baud_rate: u32) -> Self {
        Self {
            port: port.into(),
            baud_rate,
            data_bits: 8,
            stop_bits: 1,
            parity: "none".to_string(),
        }
    }

    pub fn with_data_bits(mut self, data_bits: u8) -> Self {
        self.data_bits = data_bits;
        self
    }

    pub fn with_stop_bits(mut self, stop_bits: u8) -> Self {
        self.stop_bits = stop_bits;
        self
    }

    pub fn with_parity(mut self, parity: impl Into<String>) -> Self {
        self.parity = parity.into();
        self
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum MqttProtocolVersion {
    #[default]
    #[serde(rename = "3.1.1")]
    V3_1_1,
    #[serde(rename = "5.0")]
    V5_0,
}

fn default_mqtt_keep_alive_seconds() -> u16 {
    60
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MqttUserProperty {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MqttLastWillConfig {
    pub topic: String,
    pub payload: String,
    pub qos: u8,
    pub retain: bool,
    #[serde(default)]
    pub delay_interval_seconds: u32,
    #[serde(default)]
    pub payload_format_utf8: bool,
    #[serde(default)]
    pub message_expiry_interval_seconds: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_topic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_data: Option<String>,
    #[serde(default)]
    pub user_properties: Vec<MqttUserProperty>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MqttUplinkConfig {
    pub sink_id: String,
    pub broker: String,
    pub client_id: String,
    #[serde(default)]
    pub protocol_version: MqttProtocolVersion,
    #[serde(default = "default_mqtt_keep_alive_seconds")]
    pub keep_alive_seconds: u16,
    #[serde(default = "default_true")]
    pub clean_session: bool,
    #[serde(default = "default_true")]
    pub clean_start: bool,
    #[serde(default)]
    pub session_expiry_interval_seconds: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receive_maximum: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_packet_size_bytes: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_alias_maximum: Option<u16>,
    #[serde(default)]
    pub request_response_information: bool,
    #[serde(default = "default_true")]
    pub request_problem_information: bool,
    #[serde(default)]
    pub user_properties: Vec<MqttUserProperty>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_will: Option<MqttLastWillConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password_env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_ca_path: Option<String>,
    pub topic_template: String,
    pub qos: u8,
    pub batch_size: u32,
    pub flush_interval_ms: u64,
}

impl MqttUplinkConfig {
    pub fn velamq(
        sink_id: impl Into<String>,
        broker: impl Into<String>,
        client_id: impl Into<String>,
    ) -> Self {
        Self {
            sink_id: sink_id.into(),
            broker: broker.into(),
            client_id: client_id.into(),
            protocol_version: MqttProtocolVersion::default(),
            keep_alive_seconds: default_mqtt_keep_alive_seconds(),
            clean_session: true,
            clean_start: true,
            session_expiry_interval_seconds: 0,
            receive_maximum: None,
            maximum_packet_size_bytes: None,
            topic_alias_maximum: None,
            request_response_information: false,
            request_problem_information: true,
            user_properties: Vec::new(),
            last_will: None,
            username: None,
            password_env: None,
            tls_ca_path: None,
            topic_template: "edge/{edge_id}/device/{device_id}/telemetry".to_string(),
            qos: 1,
            batch_size: 100,
            flush_interval_ms: 1000,
        }
    }

    pub fn with_topic_template(mut self, topic_template: impl Into<String>) -> Self {
        self.topic_template = topic_template.into();
        self
    }

    pub fn with_protocol_version(mut self, protocol_version: MqttProtocolVersion) -> Self {
        self.protocol_version = protocol_version;
        self
    }

    pub fn with_qos(mut self, qos: u8) -> Self {
        self.qos = qos;
        self
    }

    pub fn with_credentials_env(
        mut self,
        username: impl Into<String>,
        password_env: impl Into<String>,
    ) -> Self {
        self.username = Some(username.into());
        self.password_env = Some(password_env.into());
        self
    }

    pub fn with_tls_ca_path(mut self, tls_ca_path: impl Into<String>) -> Self {
        self.tls_ca_path = Some(tls_ca_path.into());
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DataConfig {
    pub config_id: String,
    pub name: String,
    pub enabled: bool,
    pub device_id: String,
    pub protocol_connection_id: String,
    pub collection: DataConfigCollection,
    pub points: Vec<DataConfigPoint>,
    #[serde(default)]
    pub algorithm_ids: Vec<String>,
    #[serde(default)]
    pub visual_graph: DataConfigVisualGraph,
    pub publish: DataConfigPublish,
}

impl DataConfig {
    pub fn new(
        config_id: impl Into<String>,
        name: impl Into<String>,
        device_id: impl Into<String>,
        protocol_connection_id: impl Into<String>,
        collection: DataConfigCollection,
        publish: DataConfigPublish,
    ) -> Self {
        Self {
            config_id: config_id.into(),
            name: name.into(),
            enabled: true,
            device_id: device_id.into(),
            protocol_connection_id: protocol_connection_id.into(),
            collection,
            points: Vec::new(),
            algorithm_ids: Vec::new(),
            visual_graph: DataConfigVisualGraph::default(),
            publish,
        }
    }

    pub fn with_point(mut self, point: DataConfigPoint) -> Self {
        self.points.push(point);
        self
    }

    pub fn with_algorithm(mut self, algorithm_id: impl Into<String>) -> Self {
        self.algorithm_ids.push(algorithm_id.into());
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataConfigVisualGraph {
    #[serde(default)]
    pub nodes: Vec<DataConfigGraphNode>,
    #[serde(default)]
    pub edges: Vec<DataConfigGraphEdge>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataConfigGraphNode {
    pub node_id: String,
    pub kind: DataConfigGraphNodeKind,
    pub label: String,
    pub ref_id: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, serde_json::Value>,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DataConfigGraphNodeKind {
    Point,
    Algorithm,
    Json,
    Mqtt,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataConfigGraphEdge {
    pub edge_id: String,
    pub from: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_port: Option<String>,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_port: Option<String>,
}

pub fn validate_data_config_visual_graph(data_config: &DataConfig) -> Result<(), String> {
    macro_rules! invalid {
        ($($arg:tt)*) => {
            return Err(format!($($arg)*))
        };
    }

    let graph = &data_config.visual_graph;
    if graph.nodes.is_empty() && graph.edges.is_empty() {
        return Ok(());
    }
    if graph.nodes.is_empty() {
        invalid!(
            "data config {} graph has edges but no nodes",
            data_config.config_id
        );
    }

    let mut node_kinds = BTreeMap::new();
    for node in &graph.nodes {
        if node.node_id.trim().is_empty() {
            invalid!(
                "data config {} graph node id is required",
                data_config.config_id
            );
        }
        if node_kinds
            .insert(node.node_id.as_str(), node.kind)
            .is_some()
        {
            invalid!(
                "data config {} graph has duplicate node {}",
                data_config.config_id,
                node.node_id
            );
        }
        if node.kind == DataConfigGraphNodeKind::Point {
            let point_id = node.ref_id.as_deref().unwrap_or_default();
            if !data_config
                .points
                .iter()
                .any(|point| point.point_id == point_id)
            {
                invalid!(
                    "data config {} graph point node {} references missing point {}",
                    data_config.config_id,
                    node.node_id,
                    point_id
                );
            }
        }
    }

    let mut indegree = node_kinds
        .keys()
        .map(|node_id| (*node_id, 0usize))
        .collect::<BTreeMap<_, _>>();
    let mut outgoing = BTreeSet::new();
    let mut incoming = BTreeSet::new();
    let mut edge_ids = BTreeSet::new();
    for edge in &graph.edges {
        if !edge_ids.insert(edge.edge_id.as_str()) {
            invalid!(
                "data config {} graph has duplicate edge {}",
                data_config.config_id,
                edge.edge_id
            );
        }
        let Some(from_kind) = node_kinds.get(edge.from.as_str()) else {
            invalid!(
                "data config {} graph edge {} references missing source {}",
                data_config.config_id,
                edge.edge_id,
                edge.from
            );
        };
        let Some(to_kind) = node_kinds.get(edge.to.as_str()) else {
            invalid!(
                "data config {} graph edge {} references missing target {}",
                data_config.config_id,
                edge.edge_id,
                edge.to
            );
        };
        if *from_kind == DataConfigGraphNodeKind::Mqtt {
            invalid!(
                "data config {} graph MQTT node {} cannot have outgoing edges",
                data_config.config_id,
                edge.from
            );
        }
        if *to_kind == DataConfigGraphNodeKind::Point {
            invalid!(
                "data config {} graph point node {} cannot have incoming edges",
                data_config.config_id,
                edge.to
            );
        }
        *indegree.entry(edge.to.as_str()).or_default() += 1;
        outgoing.insert(edge.from.as_str());
        incoming.insert(edge.to.as_str());
    }

    let mqtt_nodes = graph
        .nodes
        .iter()
        .filter(|node| node.kind == DataConfigGraphNodeKind::Mqtt)
        .collect::<Vec<_>>();
    if mqtt_nodes.is_empty() {
        invalid!(
            "data config {} graph requires at least one MQTT output",
            data_config.config_id
        );
    }
    if let Some(node) = mqtt_nodes
        .iter()
        .find(|node| !incoming.contains(node.node_id.as_str()))
    {
        invalid!(
            "data config {} graph MQTT output {} is disconnected",
            data_config.config_id,
            node.node_id
        );
    }
    if let Some(node) = graph.nodes.iter().find(|node| {
        node.kind != DataConfigGraphNodeKind::Mqtt && !outgoing.contains(node.node_id.as_str())
    }) {
        invalid!(
            "data config {} graph node {} has no downstream output",
            data_config.config_id,
            node.node_id
        );
    }

    let mut queue = indegree
        .iter()
        .filter_map(|(node_id, count)| (*count == 0).then_some(*node_id))
        .collect::<Vec<_>>();
    let mut visited = 0usize;
    while let Some(node_id) = queue.pop() {
        visited += 1;
        for edge in graph.edges.iter().filter(|edge| edge.from == node_id) {
            let count = indegree
                .get_mut(edge.to.as_str())
                .expect("validated graph target exists");
            *count -= 1;
            if *count == 0 {
                queue.push(edge.to.as_str());
            }
        }
    }
    if visited != graph.nodes.len() {
        invalid!(
            "data config {} graph contains a cycle",
            data_config.config_id
        );
    }

    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandFlowConfig {
    pub flow_id: String,
    pub name: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub protocol_connection_id: String,
    pub mqtt_connection_id: String,
    pub subscribe_topic: String,
    pub qos: u8,
    pub reply_topic_template: String,
    #[serde(default)]
    pub nodes: Vec<CommandGraphNode>,
    #[serde(default)]
    pub edges: Vec<CommandGraphEdge>,
}

impl CommandFlowConfig {
    pub fn new(
        flow_id: impl Into<String>,
        name: impl Into<String>,
        mqtt_connection_id: impl Into<String>,
        subscribe_topic: impl Into<String>,
        reply_topic_template: impl Into<String>,
    ) -> Self {
        Self {
            flow_id: flow_id.into(),
            name: name.into(),
            enabled: true,
            protocol_connection_id: String::new(),
            mqtt_connection_id: mqtt_connection_id.into(),
            subscribe_topic: subscribe_topic.into(),
            qos: 1,
            reply_topic_template: reply_topic_template.into(),
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn with_node(mut self, node: CommandGraphNode) -> Self {
        self.nodes.push(node);
        self
    }

    pub fn with_protocol_connection(mut self, protocol_connection_id: impl Into<String>) -> Self {
        self.protocol_connection_id = protocol_connection_id.into();
        self
    }

    pub fn with_edge(mut self, edge: CommandGraphEdge) -> Self {
        self.edges.push(edge);
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandGraphNode {
    pub node_id: String,
    pub kind: CommandGraphNodeKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_id: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, serde_json::Value>,
    pub x: i32,
    pub y: i32,
}

impl CommandGraphNode {
    pub fn new(
        node_id: impl Into<String>,
        kind: CommandGraphNodeKind,
        label: impl Into<String>,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            kind,
            label: label.into(),
            ref_id: None,
            params: BTreeMap::new(),
            x: 0,
            y: 0,
        }
    }

    pub fn with_ref(mut self, ref_id: impl Into<String>) -> Self {
        self.ref_id = Some(ref_id.into());
        self
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandGraphNodeKind {
    MqttInput,
    JsonParse,
    Condition,
    SafetyGate,
    PointWrite,
    MqttReply,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandGraphEdge {
    pub edge_id: String,
    pub from: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_port: Option<String>,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_port: Option<String>,
}

impl CommandGraphEdge {
    pub fn new(edge_id: impl Into<String>, from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            edge_id: edge_id.into(),
            from: from.into(),
            from_port: None,
            to: to.into(),
            to_port: None,
        }
    }

    pub fn with_ports(mut self, from_port: impl Into<String>, to_port: impl Into<String>) -> Self {
        self.from_port = Some(from_port.into());
        self.to_port = Some(to_port.into());
        self
    }
}

pub fn validate_command_flow(
    flow: &CommandFlowConfig,
    point_mappings: &[TelemetryPointMapping],
) -> Result<(), String> {
    macro_rules! invalid {
        ($($arg:tt)*) => {
            return Err(format!($($arg)*))
        };
    }

    if flow.flow_id.trim().is_empty() {
        invalid!("command flow id is required");
    }
    if flow.mqtt_connection_id.trim().is_empty() {
        invalid!("command flow {} MQTT connection is required", flow.flow_id);
    }
    if flow.subscribe_topic.trim().is_empty() {
        invalid!("command flow {} subscribe topic is required", flow.flow_id);
    }
    if flow.reply_topic_template.trim().is_empty() {
        invalid!("command flow {} reply topic is required", flow.flow_id);
    }
    if flow.qos > 2 {
        invalid!("command flow {} QoS must be 0, 1, or 2", flow.flow_id);
    }
    if flow.nodes.is_empty() {
        invalid!("command flow {} graph requires nodes", flow.flow_id);
    }

    let points = point_mappings
        .iter()
        .map(|mapping| (mapping.point_id.as_str(), mapping))
        .collect::<BTreeMap<_, _>>();
    let mut node_kinds = BTreeMap::new();
    for node in &flow.nodes {
        if node.node_id.trim().is_empty() {
            invalid!("command flow {} node id is required", flow.flow_id);
        }
        if node_kinds
            .insert(node.node_id.as_str(), node.kind)
            .is_some()
        {
            invalid!(
                "command flow {} has duplicate node {}",
                flow.flow_id,
                node.node_id
            );
        }
        if node.kind == CommandGraphNodeKind::PointWrite {
            let point_id = node.ref_id.as_deref().unwrap_or_default();
            let Some(mapping) = points.get(point_id) else {
                invalid!(
                    "command flow {} write node {} references missing point {}",
                    flow.flow_id,
                    node.node_id,
                    point_id
                );
            };
            if !mapping.access.is_writable() {
                invalid!(
                    "command flow {} write node {} references read-only point {}",
                    flow.flow_id,
                    node.node_id,
                    point_id
                );
            }
            if !flow.protocol_connection_id.is_empty()
                && mapping.protocol_connection_id != flow.protocol_connection_id
            {
                invalid!(
                    "command flow {} write node {} uses protocol connection {}, expected {}",
                    flow.flow_id,
                    node.node_id,
                    mapping.protocol_connection_id,
                    flow.protocol_connection_id
                );
            }
            validate_point_access(&mapping.address, mapping.access)?;
            if let Some(value_path) = node.params.get("value_path") {
                let Some(value_path) = value_path.as_str() else {
                    invalid!(
                        "command flow {} write node {} value_path must be a string",
                        flow.flow_id,
                        node.node_id
                    );
                };
                if !valid_json_field_path(value_path) {
                    invalid!(
                        "command flow {} write node {} value_path must be a non-empty dot-separated JSON field path",
                        flow.flow_id,
                        node.node_id
                    );
                }
            }
            if let Some(verification) = node
                .params
                .get("verification")
                .or_else(|| node.params.get("verify_mode"))
            {
                let Some(verification) = verification.as_str() else {
                    invalid!(
                        "command flow {} write node {} verification must be a string",
                        flow.flow_id,
                        node.node_id
                    );
                };
                if !matches!(verification, "response" | "readback") {
                    invalid!(
                        "command flow {} write node {} verification mode is unsupported: {}",
                        flow.flow_id,
                        node.node_id,
                        verification
                    );
                }
            }
            if let Some(tolerance) = node.params.get("readback_tolerance") {
                let Some(tolerance) = tolerance.as_f64() else {
                    invalid!(
                        "command flow {} write node {} readback_tolerance must be a number",
                        flow.flow_id,
                        node.node_id
                    );
                };
                if !tolerance.is_finite() || tolerance < 0.0 {
                    invalid!(
                        "command flow {} write node {} readback_tolerance must be finite and non-negative",
                        flow.flow_id,
                        node.node_id
                    );
                }
            }
        }
        if node.kind == CommandGraphNodeKind::SafetyGate {
            if let Some(require_confirmation) = node.params.get("require_confirmation") {
                if !require_confirmation.is_boolean() {
                    invalid!(
                        "command flow {} safety node {} require_confirmation must be boolean",
                        flow.flow_id,
                        node.node_id
                    );
                }
            }
            if let Some(source_path) = node.params.get("source_path") {
                if source_path
                    .as_str()
                    .filter(|value| !value.trim().is_empty())
                    .is_none()
                {
                    invalid!(
                        "command flow {} safety node {} source_path must be a non-empty string",
                        flow.flow_id,
                        node.node_id
                    );
                }
            }
            if let Some(allowed_sources) = node.params.get("allowed_sources") {
                let Some(allowed_sources) = allowed_sources.as_array() else {
                    invalid!(
                        "command flow {} safety node {} allowed_sources must be an array",
                        flow.flow_id,
                        node.node_id
                    );
                };
                if allowed_sources.is_empty()
                    || allowed_sources.iter().any(|source| {
                        source
                            .as_str()
                            .filter(|value| !value.trim().is_empty())
                            .is_none()
                    })
                {
                    invalid!(
                        "command flow {} safety node {} allowed_sources requires non-empty strings",
                        flow.flow_id,
                        node.node_id
                    );
                }
            }
            let max_commands = node.params.get("max_commands");
            let window_ms = node.params.get("window_ms");
            if max_commands.is_some() != window_ms.is_some() {
                invalid!(
                    "command flow {} safety node {} max_commands and window_ms must be configured together",
                    flow.flow_id,
                    node.node_id
                );
            }
            if let Some(max_commands) = max_commands {
                if max_commands.as_u64().filter(|value| *value > 0).is_none() {
                    invalid!(
                        "command flow {} safety node {} max_commands must be a positive integer",
                        flow.flow_id,
                        node.node_id
                    );
                }
            }
            if let Some(window_ms) = window_ms {
                if window_ms.as_u64().filter(|value| *value > 0).is_none() {
                    invalid!(
                        "command flow {} safety node {} window_ms must be a positive integer",
                        flow.flow_id,
                        node.node_id
                    );
                }
            }
        }
    }

    for required in [
        CommandGraphNodeKind::MqttInput,
        CommandGraphNodeKind::PointWrite,
        CommandGraphNodeKind::MqttReply,
    ] {
        if !node_kinds.values().any(|kind| *kind == required) {
            invalid!(
                "command flow {} requires a {:?} node",
                flow.flow_id,
                required
            );
        }
    }

    let mut indegree = node_kinds
        .keys()
        .map(|node_id| (*node_id, 0usize))
        .collect::<BTreeMap<_, _>>();
    let mut outgoing = BTreeSet::new();
    let mut edge_ids = BTreeSet::new();
    for edge in &flow.edges {
        if !edge_ids.insert(edge.edge_id.as_str()) {
            invalid!(
                "command flow {} has duplicate edge {}",
                flow.flow_id,
                edge.edge_id
            );
        }
        let Some(from_kind) = node_kinds.get(edge.from.as_str()) else {
            invalid!(
                "command flow {} edge {} references missing source {}",
                flow.flow_id,
                edge.edge_id,
                edge.from
            );
        };
        let Some(to_kind) = node_kinds.get(edge.to.as_str()) else {
            invalid!(
                "command flow {} edge {} references missing target {}",
                flow.flow_id,
                edge.edge_id,
                edge.to
            );
        };
        if *from_kind == CommandGraphNodeKind::MqttReply {
            invalid!(
                "command flow {} reply node {} cannot have outgoing edges",
                flow.flow_id,
                edge.from
            );
        }
        if *to_kind == CommandGraphNodeKind::MqttInput {
            invalid!(
                "command flow {} input node {} cannot have incoming edges",
                flow.flow_id,
                edge.to
            );
        }
        *indegree.entry(edge.to.as_str()).or_default() += 1;
        outgoing.insert(edge.from.as_str());
    }

    if let Some(node) = flow.nodes.iter().find(|node| {
        node.kind != CommandGraphNodeKind::MqttInput
            && indegree.get(node.node_id.as_str()).copied().unwrap_or(0) == 0
    }) {
        invalid!(
            "command flow {} node {} has no upstream input",
            flow.flow_id,
            node.node_id
        );
    }
    if let Some(node) = flow.nodes.iter().find(|node| {
        node.kind != CommandGraphNodeKind::MqttReply && !outgoing.contains(node.node_id.as_str())
    }) {
        invalid!(
            "command flow {} node {} has no downstream output",
            flow.flow_id,
            node.node_id
        );
    }

    let mut queue = indegree
        .iter()
        .filter_map(|(node_id, count)| (*count == 0).then_some(*node_id))
        .collect::<Vec<_>>();
    let mut visited = 0usize;
    while let Some(node_id) = queue.pop() {
        visited += 1;
        for edge in flow.edges.iter().filter(|edge| edge.from == node_id) {
            let count = indegree
                .get_mut(edge.to.as_str())
                .expect("validated command graph target exists");
            *count -= 1;
            if *count == 0 {
                queue.push(edge.to.as_str());
            }
        }
    }
    if visited != flow.nodes.len() {
        invalid!("command flow {} graph contains a cycle", flow.flow_id);
    }

    Ok(())
}

fn valid_json_field_path(path: &str) -> bool {
    let path = path.trim();
    !path.is_empty()
        && path.len() <= 256
        && path
            .split('.')
            .all(|segment| !segment.is_empty() && !segment.chars().any(char::is_whitespace))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataConfigCollection {
    pub period_ms: u64,
    pub timeout_ms: u64,
    pub retry_count: u32,
}

pub const MAX_DATA_CONFIG_TIMEOUT_MS: u64 = 300_000;
pub const MAX_DATA_CONFIG_RETRY_COUNT: u32 = 10;
pub const MAX_PROTOCOL_CIRCUIT_FAILURE_THRESHOLD: u32 = 100;
pub const MIN_PROTOCOL_CIRCUIT_OPEN_DURATION_MS: u64 = 100;
pub const MAX_PROTOCOL_CIRCUIT_OPEN_DURATION_MS: u64 = 3_600_000;
pub const MAX_PROTOCOL_CIRCUIT_HALF_OPEN_SUCCESSES: u32 = 10;

impl DataConfigCollection {
    pub fn new(period_ms: u64) -> Self {
        Self {
            period_ms,
            timeout_ms: 800,
            retry_count: 2,
        }
    }

    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    pub fn with_retry_count(mut self, retry_count: u32) -> Self {
        self.retry_count = retry_count;
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DataConfigPoint {
    pub point_id: String,
    pub semantic_id: String,
    pub address: PointAddress,
    pub value_type: TelemetryType,
    pub unit: Option<String>,
    pub json_field: String,
}

impl DataConfigPoint {
    pub fn new(
        point_id: impl Into<String>,
        semantic_id: impl Into<String>,
        address: PointAddress,
        value_type: TelemetryType,
        json_field: impl Into<String>,
    ) -> Self {
        Self {
            point_id: point_id.into(),
            semantic_id: semantic_id.into(),
            address,
            value_type,
            unit: None,
            json_field: json_field.into(),
        }
    }

    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataConfigPublish {
    pub sink_id: String,
    pub topic_template: String,
    pub qos: u8,
    pub payload: DataConfigPayload,
}

impl DataConfigPublish {
    pub fn new(
        sink_id: impl Into<String>,
        topic_template: impl Into<String>,
        payload: DataConfigPayload,
    ) -> Self {
        Self {
            sink_id: sink_id.into(),
            topic_template: topic_template.into(),
            qos: 1,
            payload,
        }
    }

    pub fn with_qos(mut self, qos: u8) -> Self {
        self.qos = qos;
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataConfigPayload {
    pub mode: DataConfigPayloadMode,
    pub timestamp_field: String,
    pub include_quality: bool,
}

impl DataConfigPayload {
    pub fn object() -> Self {
        Self {
            mode: DataConfigPayloadMode::Object,
            timestamp_field: "ts".to_string(),
            include_quality: true,
        }
    }

    pub fn array() -> Self {
        Self {
            mode: DataConfigPayloadMode::Array,
            timestamp_field: "ts".to_string(),
            include_quality: true,
        }
    }

    pub fn without_quality(mut self) -> Self {
        self.include_quality = false;
        self
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DataConfigPayloadMode {
    Object,
    Array,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TelemetryPointMapping {
    pub point_id: String,
    pub device_id: String,
    pub semantic_id: String,
    pub protocol_connection_id: String,
    pub address: PointAddress,
    pub value_type: TelemetryType,
    #[serde(default)]
    pub access: PointAccess,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opc_ua: Option<OpcUaPointOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iec101: Option<Iec101PointOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iec104: Option<Iec104PointOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bacnet: Option<BacnetPointOptions>,
    pub unit: Option<String>,
    pub range: Option<NumberRange>,
    pub interval_ms: u64,
}

impl TelemetryPointMapping {
    pub fn new(
        point_id: impl Into<String>,
        device_id: impl Into<String>,
        semantic_id: impl Into<String>,
        protocol_connection_id: impl Into<String>,
        address: PointAddress,
        value_type: TelemetryType,
    ) -> Self {
        Self {
            point_id: point_id.into(),
            device_id: device_id.into(),
            semantic_id: semantic_id.into(),
            protocol_connection_id: protocol_connection_id.into(),
            address,
            value_type,
            access: PointAccess::ReadOnly,
            opc_ua: None,
            iec101: None,
            iec104: None,
            bacnet: None,
            unit: None,
            range: None,
            interval_ms: 1000,
        }
    }

    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    pub fn with_range(mut self, range: NumberRange) -> Self {
        self.range = Some(range);
        self
    }

    pub fn with_interval_ms(mut self, interval_ms: u64) -> Self {
        self.interval_ms = interval_ms;
        self
    }

    pub fn with_access(mut self, access: PointAccess) -> Self {
        self.access = access;
        self
    }

    pub fn with_bacnet_options(mut self, options: BacnetPointOptions) -> Self {
        self.bacnet = Some(options);
        self
    }

    pub fn with_opc_ua_options(mut self, options: OpcUaPointOptions) -> Self {
        self.opc_ua = Some(options);
        self
    }

    pub fn with_iec101_options(mut self, options: Iec101PointOptions) -> Self {
        self.iec101 = Some(options);
        self
    }

    pub fn with_iec104_options(mut self, options: Iec104PointOptions) -> Self {
        self.iec104 = Some(options);
        self
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PointAccess {
    #[default]
    ReadOnly,
    ReadWrite,
    WriteOnly,
}

impl PointAccess {
    pub fn is_readable(self) -> bool {
        matches!(self, Self::ReadOnly | Self::ReadWrite)
    }

    pub fn is_writable(self) -> bool {
        matches!(self, Self::ReadWrite | Self::WriteOnly)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum OpcUaWriteDataType {
    Boolean,
    SByte,
    Byte,
    Int16,
    UInt16,
    Int32,
    UInt32,
    Int64,
    UInt64,
    Float,
    Double,
    String,
}

impl OpcUaWriteDataType {
    pub const fn default_for(value_type: TelemetryType) -> Self {
        match value_type {
            TelemetryType::Boolean => Self::Boolean,
            TelemetryType::Integer => Self::Int32,
            TelemetryType::Float => Self::Float,
            TelemetryType::Text => Self::String,
        }
    }

    pub const fn accepts(self, value_type: TelemetryType) -> bool {
        matches!(
            (self, value_type),
            (Self::Boolean, TelemetryType::Boolean)
                | (
                    Self::SByte
                        | Self::Byte
                        | Self::Int16
                        | Self::UInt16
                        | Self::Int32
                        | Self::UInt32
                        | Self::Int64
                        | Self::UInt64,
                    TelemetryType::Integer
                )
                | (Self::Float | Self::Double, TelemetryType::Float)
                | (Self::String, TelemetryType::Text)
        )
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpcUaPointOptions {
    pub write_data_type: OpcUaWriteDataType,
}

impl OpcUaPointOptions {
    pub const fn new(write_data_type: OpcUaWriteDataType) -> Self {
        Self { write_data_type }
    }

    pub fn validate(self, value_type: TelemetryType) -> Result<(), String> {
        if !self.write_data_type.accepts(value_type) {
            return Err(format!(
                "OPC UA write data type {:?} is incompatible with {:?} telemetry",
                self.write_data_type, value_type
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Iec104ControlType {
    #[serde(rename = "C_SC_NA_1")]
    SingleCommand,
    #[serde(rename = "C_DC_NA_1")]
    DoubleCommand,
    #[serde(rename = "C_SE_NC_1")]
    SetpointFloat,
}

pub type Iec101ControlType = Iec104ControlType;

impl Iec104ControlType {
    pub const fn default_for(value_type: TelemetryType) -> Option<Self> {
        match value_type {
            TelemetryType::Boolean => Some(Self::SingleCommand),
            TelemetryType::Integer => Some(Self::DoubleCommand),
            TelemetryType::Float => Some(Self::SetpointFloat),
            TelemetryType::Text => None,
        }
    }

    pub const fn accepts(self, value_type: TelemetryType) -> bool {
        matches!(
            (self, value_type),
            (Self::SingleCommand, TelemetryType::Boolean)
                | (Self::DoubleCommand, TelemetryType::Integer)
                | (Self::SetpointFloat, TelemetryType::Float)
        )
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Iec104PointOptions {
    pub control_type: Iec104ControlType,
    #[serde(default)]
    pub select_before_operate: bool,
}

pub type Iec101PointOptions = Iec104PointOptions;

impl Iec104PointOptions {
    pub const fn new(control_type: Iec104ControlType) -> Self {
        Self {
            control_type,
            select_before_operate: false,
        }
    }

    pub const fn with_select_before_operate(mut self, enabled: bool) -> Self {
        self.select_before_operate = enabled;
        self
    }

    pub fn validate(self, value_type: TelemetryType) -> Result<(), String> {
        if !self.control_type.accepts(value_type) {
            return Err(format!(
                "IEC 104 control type {:?} is incompatible with {:?} telemetry",
                self.control_type, value_type
            ));
        }
        Ok(())
    }
}

pub fn validate_point_access(address: &PointAddress, access: PointAccess) -> Result<(), String> {
    if access.is_writable() && matches!(address.kind.as_str(), "input_register" | "discrete_input")
    {
        return Err(format!(
            "Modbus {} points are protocol-level read-only",
            address.kind
        ));
    }
    if access.is_writable()
        && address
            .modbus
            .as_ref()
            .is_some_and(|options| options.bit_index.is_some())
    {
        return Err(
            "writable Modbus register bit fields require an atomic mask-write operation"
                .to_string(),
        );
    }
    Ok(())
}

pub fn validate_modbus_point_options(
    address: &PointAddress,
    value_type: TelemetryType,
    access: PointAccess,
) -> Result<(), String> {
    validate_point_access(address, access)?;
    let Some(options) = &address.modbus else {
        return Ok(());
    };
    if !matches!(address.kind.as_str(), "holding_register" | "input_register") {
        return Err("Modbus decoding options are only valid for register points".to_string());
    }
    if !options.scale.is_finite() || options.scale == 0.0 || !options.offset.is_finite() {
        return Err(
            "Modbus scale must be finite and non-zero and offset must be finite".to_string(),
        );
    }
    if let Some(bit_index) = options.bit_index {
        if bit_index > 15 {
            return Err("Modbus register bit index must be between 0 and 15".to_string());
        }
        if value_type != TelemetryType::Boolean {
            return Err("Modbus register bit fields must use boolean telemetry type".to_string());
        }
        if options.encoding.is_some() {
            return Err("Modbus register bit fields cannot define a numeric encoding".to_string());
        }
        if options.scale != 1.0 || options.offset != 0.0 {
            return Err("Modbus register bit fields cannot define scale or offset".to_string());
        }
    }
    if value_type == TelemetryType::Boolean && options.encoding.is_some() {
        return Err("boolean Modbus register points cannot define a numeric encoding".to_string());
    }
    if value_type == TelemetryType::Text
        && (options.encoding.is_some() || options.scale != 1.0 || options.offset != 0.0)
    {
        return Err(
            "text Modbus register points cannot define numeric decoding options".to_string(),
        );
    }
    if let Some(encoding) = options.encoding {
        match value_type {
            TelemetryType::Float if !encoding.is_float() => {
                return Err("float Modbus points require f32 or f64 register encoding".to_string())
            }
            TelemetryType::Integer if !encoding.is_integer() => {
                return Err("integer Modbus points require an integer register encoding".to_string())
            }
            _ => {}
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct PointAddress {
    pub kind: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modbus: Option<ModbusPointOptions>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpcUaBrowsePathAddress {
    pub starting_node: String,
    pub elements: Vec<OpcUaBrowsePathElement>,
}

impl OpcUaBrowsePathAddress {
    pub fn new(starting_node: impl Into<String>, elements: Vec<OpcUaBrowsePathElement>) -> Self {
        Self {
            starting_node: starting_node.into(),
            elements,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpcUaBrowsePathElement {
    pub namespace_index: u16,
    pub target_name: String,
}

impl OpcUaBrowsePathElement {
    pub fn new(namespace_index: u16, target_name: impl Into<String>) -> Self {
        Self {
            namespace_index,
            target_name: target_name.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SiemensS7Area {
    DataBlock,
    Marker,
    ProcessInput,
    ProcessOutput,
}

impl SiemensS7Area {
    pub const fn is_protocol_writable(self) -> bool {
        !matches!(self, Self::ProcessInput)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SiemensS7DataType {
    Bit,
    Byte,
    Word,
    DWord,
    Int,
    DInt,
    Real,
}

impl SiemensS7DataType {
    pub const fn byte_width(self) -> u16 {
        match self {
            Self::Bit | Self::Byte => 1,
            Self::Word | Self::Int => 2,
            Self::DWord | Self::DInt | Self::Real => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SiemensS7PointAddress {
    pub area: SiemensS7Area,
    pub db_number: u16,
    pub byte_offset: u32,
    pub bit_offset: Option<u8>,
    pub data_type: SiemensS7DataType,
}

impl SiemensS7PointAddress {
    pub fn canonical(self) -> String {
        let offset = self.byte_offset;
        match (self.area, self.data_type) {
            (SiemensS7Area::DataBlock, SiemensS7DataType::Bit) => format!(
                "DB{}.DBX{}.{}",
                self.db_number,
                offset,
                self.bit_offset.unwrap_or_default()
            ),
            (SiemensS7Area::DataBlock, SiemensS7DataType::Byte) => {
                format!("DB{}.DBB{}", self.db_number, offset)
            }
            (SiemensS7Area::DataBlock, SiemensS7DataType::Word) => {
                format!("DB{}.DBW{}", self.db_number, offset)
            }
            (SiemensS7Area::DataBlock, SiemensS7DataType::DWord) => {
                format!("DB{}.DBD{}", self.db_number, offset)
            }
            (SiemensS7Area::DataBlock, SiemensS7DataType::Int) => {
                format!("DB{}.INT{}", self.db_number, offset)
            }
            (SiemensS7Area::DataBlock, SiemensS7DataType::DInt) => {
                format!("DB{}.DINT{}", self.db_number, offset)
            }
            (SiemensS7Area::DataBlock, SiemensS7DataType::Real) => {
                format!("DB{}.REAL{}", self.db_number, offset)
            }
            (area, data_type) => {
                let area = match area {
                    SiemensS7Area::Marker => "M",
                    SiemensS7Area::ProcessInput => "I",
                    SiemensS7Area::ProcessOutput => "Q",
                    SiemensS7Area::DataBlock => unreachable!(),
                };
                let suffix = match data_type {
                    SiemensS7DataType::Bit => {
                        return format!(
                            "{}{}.{}",
                            area,
                            offset,
                            self.bit_offset.unwrap_or_default()
                        )
                    }
                    SiemensS7DataType::Byte => "B",
                    SiemensS7DataType::Word | SiemensS7DataType::Int => "W",
                    SiemensS7DataType::DWord
                    | SiemensS7DataType::DInt
                    | SiemensS7DataType::Real => "D",
                };
                format!("{area}{suffix}{offset}")
            }
        }
    }
}

pub fn parse_siemens_s7_point_address(value: &str) -> Result<SiemensS7PointAddress, String> {
    let value = value.trim().to_ascii_uppercase();
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return Err("Siemens S7 point address cannot be empty or contain spaces".to_string());
    }
    if let Some(rest) = value.strip_prefix("DB") {
        let (db_number, typed_address) = rest.split_once('.').ok_or_else(|| {
            "Siemens S7 DB address must use DB<number>.<type><offset>".to_string()
        })?;
        let db_number = db_number
            .parse::<u16>()
            .map_err(|_| "Siemens S7 DB number must be between 0 and 65535".to_string())?;
        let (data_type, offset) = parse_siemens_s7_typed_offset(
            typed_address,
            &[
                ("DBX", SiemensS7DataType::Bit),
                ("DBB", SiemensS7DataType::Byte),
                ("DBW", SiemensS7DataType::Word),
                ("DBD", SiemensS7DataType::DWord),
                ("DINT", SiemensS7DataType::DInt),
                ("INT", SiemensS7DataType::Int),
                ("REAL", SiemensS7DataType::Real),
            ],
        )?;
        let (byte_offset, bit_offset) = parse_siemens_s7_offset(offset, data_type)?;
        return Ok(SiemensS7PointAddress {
            area: SiemensS7Area::DataBlock,
            db_number,
            byte_offset,
            bit_offset,
            data_type,
        });
    }

    let (prefix, rest) = value.split_at(1);
    let area = match prefix {
        "M" => SiemensS7Area::Marker,
        "I" => SiemensS7Area::ProcessInput,
        "Q" => SiemensS7Area::ProcessOutput,
        _ => {
            return Err(
                "Siemens S7 point area must be DB, M (marker), I (input) or Q (output)".to_string(),
            )
        }
    };
    let (data_type, offset) = if rest.starts_with(|ch: char| ch.is_ascii_digit()) {
        (SiemensS7DataType::Bit, rest)
    } else {
        parse_siemens_s7_typed_offset(
            rest,
            &[
                ("X", SiemensS7DataType::Bit),
                ("B", SiemensS7DataType::Byte),
                ("W", SiemensS7DataType::Word),
                ("D", SiemensS7DataType::DWord),
            ],
        )?
    };
    let (byte_offset, bit_offset) = parse_siemens_s7_offset(offset, data_type)?;
    Ok(SiemensS7PointAddress {
        area,
        db_number: 0,
        byte_offset,
        bit_offset,
        data_type,
    })
}

fn parse_siemens_s7_typed_offset<'a>(
    value: &'a str,
    prefixes: &[(&str, SiemensS7DataType)],
) -> Result<(SiemensS7DataType, &'a str), String> {
    prefixes
        .iter()
        .find_map(|(prefix, data_type)| {
            value
                .strip_prefix(prefix)
                .map(|offset| (*data_type, offset))
        })
        .ok_or_else(|| format!("unsupported Siemens S7 point type in address: {value}"))
}

fn parse_siemens_s7_offset(
    value: &str,
    data_type: SiemensS7DataType,
) -> Result<(u32, Option<u8>), String> {
    if data_type == SiemensS7DataType::Bit {
        let (byte_offset, bit_offset) = value.split_once('.').ok_or_else(|| {
            "Siemens S7 bit address must include byte and bit offsets, for example M0.0".to_string()
        })?;
        let byte_offset = byte_offset
            .parse::<u32>()
            .map_err(|_| "invalid Siemens S7 byte offset".to_string())?;
        let bit_offset = bit_offset
            .parse::<u8>()
            .map_err(|_| "invalid Siemens S7 bit offset".to_string())?;
        if bit_offset > 7 {
            return Err("Siemens S7 bit offset must be between 0 and 7".to_string());
        }
        return Ok((byte_offset, Some(bit_offset)));
    }
    let byte_offset = value
        .parse::<u32>()
        .map_err(|_| "invalid Siemens S7 byte offset".to_string())?;
    Ok((byte_offset, None))
}

pub fn validate_siemens_s7_point(
    address: &PointAddress,
    value_type: TelemetryType,
    access: PointAccess,
) -> Result<SiemensS7PointAddress, String> {
    if address.kind != "s7_address" {
        return Err("Siemens S7 address kind must be `s7_address`".to_string());
    }
    if address.modbus.is_some() {
        return Err("Siemens S7 points cannot define Modbus decoding options".to_string());
    }
    let parsed = parse_siemens_s7_point_address(&address.value)?;
    match (parsed.data_type, value_type) {
        (SiemensS7DataType::Bit, TelemetryType::Boolean)
        | (SiemensS7DataType::Real, TelemetryType::Float)
        | (SiemensS7DataType::DWord, TelemetryType::Float)
        | (
            SiemensS7DataType::Byte
            | SiemensS7DataType::Word
            | SiemensS7DataType::DWord
            | SiemensS7DataType::Int
            | SiemensS7DataType::DInt,
            TelemetryType::Integer,
        ) => {}
        _ => {
            return Err(format!(
                "Siemens S7 address {} is incompatible with telemetry type {:?}",
                parsed.canonical(),
                value_type
            ))
        }
    }
    if access.is_writable() && !parsed.area.is_protocol_writable() {
        return Err("Siemens S7 process input (I) points are protocol-level read-only".to_string());
    }
    Ok(parsed)
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OmronFinsArea {
    Cio,
    Work,
    Holding,
    DataMemory,
    Auxiliary,
}

impl OmronFinsArea {
    pub const fn canonical_prefix(self) -> &'static str {
        match self {
            Self::Cio => "CIO",
            Self::Work => "W",
            Self::Holding => "H",
            Self::DataMemory => "D",
            Self::Auxiliary => "A",
        }
    }

    pub const fn supports_bit_access(self) -> bool {
        !matches!(self, Self::DataMemory)
    }

    pub const fn word_capacity(self) -> u16 {
        match self {
            Self::Cio | Self::DataMemory => 4_096,
            Self::Work | Self::Holding => 512,
            Self::Auxiliary => 1_024,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OmronFinsPointAddress {
    pub area: OmronFinsArea,
    pub word: u16,
    pub bit: Option<u8>,
}

impl OmronFinsPointAddress {
    pub fn canonical(self) -> String {
        match self.bit {
            Some(bit) => format!("{}{}.{}", self.area.canonical_prefix(), self.word, bit),
            None => format!("{}{}", self.area.canonical_prefix(), self.word),
        }
    }
}

pub fn parse_omron_fins_point_address(value: &str) -> Result<OmronFinsPointAddress, String> {
    let value = value.trim().to_ascii_uppercase().replace(' ', "");
    let (area, rest) = [
        ("CIO", OmronFinsArea::Cio),
        ("WR", OmronFinsArea::Work),
        ("HR", OmronFinsArea::Holding),
        ("DM", OmronFinsArea::DataMemory),
        ("AR", OmronFinsArea::Auxiliary),
        ("W", OmronFinsArea::Work),
        ("H", OmronFinsArea::Holding),
        ("D", OmronFinsArea::DataMemory),
        ("A", OmronFinsArea::Auxiliary),
    ]
    .into_iter()
    .find_map(|(prefix, area)| value.strip_prefix(prefix).map(|rest| (area, rest)))
    .ok_or_else(|| "Omron FINS address must use CIO, W/WR, H/HR, D/DM or A/AR area".to_string())?;
    if rest.is_empty() {
        return Err("Omron FINS address requires a word number".to_string());
    }
    let (word, bit) = match rest.split_once('.') {
        Some((word, bit)) => {
            if bit.contains('.') {
                return Err("Omron FINS bit address contains too many separators".to_string());
            }
            let bit = bit
                .parse::<u8>()
                .map_err(|_| "Omron FINS bit must be between 0 and 15".to_string())?;
            if bit > 15 {
                return Err("Omron FINS bit must be between 0 and 15".to_string());
            }
            (word, Some(bit))
        }
        None => (rest, None),
    };
    let word = word
        .parse::<u16>()
        .map_err(|_| "Omron FINS word address must be a non-negative integer".to_string())?;
    if word >= area.word_capacity() {
        return Err(format!(
            "Omron FINS {} address must be below {}",
            area.canonical_prefix(),
            area.word_capacity()
        ));
    }
    if bit.is_some() && !area.supports_bit_access() {
        return Err("Omron FINS DM area does not support bit access".to_string());
    }
    Ok(OmronFinsPointAddress { area, word, bit })
}

pub fn validate_omron_fins_point(
    address: &PointAddress,
    value_type: TelemetryType,
    access: PointAccess,
) -> Result<OmronFinsPointAddress, String> {
    validate_point_access(address, access)?;
    if address.kind != "fins_address" {
        return Err("Omron FINS points must use fins_address kind".to_string());
    }
    if address.modbus.is_some() {
        return Err("Omron FINS points cannot define Modbus decoding options".to_string());
    }
    let parsed = parse_omron_fins_point_address(&address.value)?;
    match (parsed.bit, value_type) {
        (Some(_), TelemetryType::Boolean)
        | (None, TelemetryType::Integer)
        | (None, TelemetryType::Float) => {}
        (Some(_), _) => {
            return Err("Omron FINS bit addresses require boolean telemetry type".to_string())
        }
        (None, TelemetryType::Boolean) => {
            return Err(
                "Omron FINS boolean points require a bit address such as CIO0.05".to_string(),
            )
        }
        (None, TelemetryType::Text) => {
            return Err(
                "Omron FINS text points are not supported in this runtime version".to_string(),
            )
        }
    }
    let width = if value_type == TelemetryType::Float {
        2
    } else {
        1
    };
    if parsed.word.saturating_add(width) > parsed.area.word_capacity() {
        return Err(format!(
            "Omron FINS point {} exceeds the {} area boundary",
            parsed.canonical(),
            parsed.area.canonical_prefix()
        ));
    }
    Ok(parsed)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BacnetPointAddress {
    pub device_instance: u32,
    pub object_type: u32,
    pub object_instance: u32,
    pub property_identifier: u32,
    pub array_index: Option<u32>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BacnetPointOptions {
    #[serde(default = "default_bacnet_write_priority")]
    pub write_priority: u8,
}

impl Default for BacnetPointOptions {
    fn default() -> Self {
        Self {
            write_priority: default_bacnet_write_priority(),
        }
    }
}

const fn default_bacnet_write_priority() -> u8 {
    16
}

impl BacnetPointOptions {
    pub fn validate(self) -> Result<(), String> {
        if !(1..=16).contains(&self.write_priority) {
            return Err("BACnet write priority must be between 1 and 16".to_string());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BacnetObjectTemplate {
    pub object_type: &'static str,
    pub name: &'static str,
    pub raw_value: u32,
    pub writable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BacnetPropertyTemplate {
    pub property: &'static str,
    pub name: &'static str,
    pub raw_value: u32,
}

const BACNET_OBJECT_TEMPLATES: &[BacnetObjectTemplate] = &[
    BacnetObjectTemplate {
        object_type: "analog_input",
        name: "模拟量输入",
        raw_value: 0,
        writable: false,
    },
    BacnetObjectTemplate {
        object_type: "analog_output",
        name: "模拟量输出",
        raw_value: 1,
        writable: true,
    },
    BacnetObjectTemplate {
        object_type: "analog_value",
        name: "模拟量值",
        raw_value: 2,
        writable: true,
    },
    BacnetObjectTemplate {
        object_type: "binary_input",
        name: "开关量输入",
        raw_value: 3,
        writable: false,
    },
    BacnetObjectTemplate {
        object_type: "binary_output",
        name: "开关量输出",
        raw_value: 4,
        writable: true,
    },
    BacnetObjectTemplate {
        object_type: "binary_value",
        name: "开关量值",
        raw_value: 5,
        writable: true,
    },
    BacnetObjectTemplate {
        object_type: "device",
        name: "设备",
        raw_value: 8,
        writable: false,
    },
    BacnetObjectTemplate {
        object_type: "multi_state_input",
        name: "多状态输入",
        raw_value: 13,
        writable: false,
    },
    BacnetObjectTemplate {
        object_type: "multi_state_output",
        name: "多状态输出",
        raw_value: 14,
        writable: true,
    },
    BacnetObjectTemplate {
        object_type: "multi_state_value",
        name: "多状态值",
        raw_value: 19,
        writable: true,
    },
    BacnetObjectTemplate {
        object_type: "accumulator",
        name: "累加器",
        raw_value: 23,
        writable: false,
    },
];

const BACNET_PROPERTY_TEMPLATES: &[BacnetPropertyTemplate] = &[
    BacnetPropertyTemplate {
        property: "present_value",
        name: "当前值",
        raw_value: 85,
    },
    BacnetPropertyTemplate {
        property: "status_flags",
        name: "状态标志",
        raw_value: 111,
    },
    BacnetPropertyTemplate {
        property: "units",
        name: "工程单位",
        raw_value: 117,
    },
    BacnetPropertyTemplate {
        property: "object_name",
        name: "对象名称",
        raw_value: 77,
    },
    BacnetPropertyTemplate {
        property: "description",
        name: "描述",
        raw_value: 28,
    },
    BacnetPropertyTemplate {
        property: "out_of_service",
        name: "退出服务",
        raw_value: 81,
    },
    BacnetPropertyTemplate {
        property: "reliability",
        name: "可靠性",
        raw_value: 103,
    },
];

pub fn bacnet_object_templates() -> &'static [BacnetObjectTemplate] {
    BACNET_OBJECT_TEMPLATES
}

pub fn bacnet_property_templates() -> &'static [BacnetPropertyTemplate] {
    BACNET_PROPERTY_TEMPLATES
}

pub fn parse_bacnet_point_address(value: &str) -> Result<BacnetPointAddress, String> {
    let parts = value.split(':').map(str::trim).collect::<Vec<_>>();
    if !(4..=5).contains(&parts.len()) {
        return Err(
            "BACnet point address must be device:object_type:object_instance:property[:array_index]"
                .to_string(),
        );
    }
    let device_instance = parse_bacnet_number(parts[0], "device instance")?;
    let object_type = bacnet_object_type_raw(parts[1])?;
    let object_instance = parse_bacnet_number(parts[2], "object instance")?;
    let property_identifier = bacnet_property_raw(parts[3])?;
    let array_index = parts
        .get(4)
        .map(|value| parse_bacnet_number(value, "array index"))
        .transpose()?;
    if device_instance > 4_194_302 || object_instance > 4_194_302 {
        return Err("BACnet device and object instances must be between 0 and 4194302".to_string());
    }
    if object_type > 1_023 {
        return Err("BACnet object type must be between 0 and 1023".to_string());
    }
    if property_identifier > 4_194_303 {
        return Err("BACnet property identifier must be between 0 and 4194303".to_string());
    }
    Ok(BacnetPointAddress {
        device_instance,
        object_type,
        object_instance,
        property_identifier,
        array_index,
    })
}

pub fn validate_bacnet_point(
    address: &PointAddress,
    value_type: TelemetryType,
    access: PointAccess,
    options: Option<BacnetPointOptions>,
) -> Result<BacnetPointAddress, String> {
    if address.kind != "bacnet_object_property" {
        return Err("BACnet points require bacnet_object_property address kind".to_string());
    }
    let parsed = parse_bacnet_point_address(&address.value)?;
    if let Some(options) = options {
        options.validate()?;
    }
    if !access.is_writable() {
        return Ok(parsed);
    }
    if parsed.property_identifier != 85 {
        return Err("writable BACnet points must target present_value".to_string());
    }
    if parsed.array_index.is_some() {
        return Err("writable BACnet points cannot target an array element".to_string());
    }
    match parsed.object_type {
        1 | 2 if value_type == TelemetryType::Float => {}
        4 | 5 if value_type == TelemetryType::Boolean => {}
        14 | 19 if value_type == TelemetryType::Integer => {}
        0 | 3 | 13 | 23 => {
            return Err(
                "BACnet input and accumulator objects are protocol-level read-only".to_string(),
            )
        }
        1 | 2 => {
            return Err("BACnet analog command points must use float telemetry type".to_string())
        }
        4 | 5 => {
            return Err("BACnet binary command points must use boolean telemetry type".to_string())
        }
        14 | 19 => {
            return Err(
                "BACnet multi-state command points must use integer telemetry type".to_string(),
            )
        }
        _ => return Err("BACnet object type is not supported for command writes".to_string()),
    }
    Ok(parsed)
}

fn parse_bacnet_number(value: &str, field: &str) -> Result<u32, String> {
    value
        .parse::<u32>()
        .map_err(|_| format!("invalid BACnet {field}: {value}"))
}

fn bacnet_object_type_raw(value: &str) -> Result<u32, String> {
    bacnet_object_templates()
        .iter()
        .find(|template| template.object_type.eq_ignore_ascii_case(value))
        .map(|template| template.raw_value)
        .or_else(|| value.parse::<u32>().ok())
        .ok_or_else(|| format!("unsupported BACnet object type: {value}"))
}

fn bacnet_property_raw(value: &str) -> Result<u32, String> {
    bacnet_property_templates()
        .iter()
        .find(|template| template.property.eq_ignore_ascii_case(value))
        .map(|template| template.raw_value)
        .or_else(|| value.parse::<u32>().ok())
        .ok_or_else(|| format!("unsupported BACnet property: {value}"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dlt645PointAddress {
    pub meter_address: String,
    pub data_identifier: u32,
    pub decimal_places: u8,
    pub value_bytes: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dlt645DataIdentifierTemplate {
    pub template_id: &'static str,
    pub name: &'static str,
    pub semantic_id: &'static str,
    pub data_identifier: &'static str,
    pub value_type: TelemetryType,
    pub decimal_places: u8,
    pub value_bytes: u8,
    pub unit: Option<&'static str>,
}

const DLT645_DATA_IDENTIFIER_TEMPLATES: &[Dlt645DataIdentifierTemplate] = &[
    Dlt645DataIdentifierTemplate {
        template_id: "total_active_energy",
        name: "组合有功总电能",
        semantic_id: "electric.energy.active.total",
        data_identifier: "00000000",
        value_type: TelemetryType::Float,
        decimal_places: 2,
        value_bytes: 4,
        unit: Some("kWh"),
    },
    Dlt645DataIdentifierTemplate {
        template_id: "forward_active_energy",
        name: "正向有功总电能",
        semantic_id: "electric.energy.active.forward",
        data_identifier: "00010000",
        value_type: TelemetryType::Float,
        decimal_places: 2,
        value_bytes: 4,
        unit: Some("kWh"),
    },
    Dlt645DataIdentifierTemplate {
        template_id: "reverse_active_energy",
        name: "反向有功总电能",
        semantic_id: "electric.energy.active.reverse",
        data_identifier: "00020000",
        value_type: TelemetryType::Float,
        decimal_places: 2,
        value_bytes: 4,
        unit: Some("kWh"),
    },
    Dlt645DataIdentifierTemplate {
        template_id: "voltage_a",
        name: "A 相电压",
        semantic_id: "electric.voltage.a",
        data_identifier: "02010100",
        value_type: TelemetryType::Float,
        decimal_places: 1,
        value_bytes: 2,
        unit: Some("V"),
    },
    Dlt645DataIdentifierTemplate {
        template_id: "voltage_b",
        name: "B 相电压",
        semantic_id: "electric.voltage.b",
        data_identifier: "02010200",
        value_type: TelemetryType::Float,
        decimal_places: 1,
        value_bytes: 2,
        unit: Some("V"),
    },
    Dlt645DataIdentifierTemplate {
        template_id: "voltage_c",
        name: "C 相电压",
        semantic_id: "electric.voltage.c",
        data_identifier: "02010300",
        value_type: TelemetryType::Float,
        decimal_places: 1,
        value_bytes: 2,
        unit: Some("V"),
    },
    Dlt645DataIdentifierTemplate {
        template_id: "current_a",
        name: "A 相电流",
        semantic_id: "electric.current.a",
        data_identifier: "02020100",
        value_type: TelemetryType::Float,
        decimal_places: 3,
        value_bytes: 3,
        unit: Some("A"),
    },
    Dlt645DataIdentifierTemplate {
        template_id: "current_b",
        name: "B 相电流",
        semantic_id: "electric.current.b",
        data_identifier: "02020200",
        value_type: TelemetryType::Float,
        decimal_places: 3,
        value_bytes: 3,
        unit: Some("A"),
    },
    Dlt645DataIdentifierTemplate {
        template_id: "current_c",
        name: "C 相电流",
        semantic_id: "electric.current.c",
        data_identifier: "02020300",
        value_type: TelemetryType::Float,
        decimal_places: 3,
        value_bytes: 3,
        unit: Some("A"),
    },
    Dlt645DataIdentifierTemplate {
        template_id: "active_power_total",
        name: "总有功功率",
        semantic_id: "electric.power.active.total",
        data_identifier: "02030000",
        value_type: TelemetryType::Float,
        decimal_places: 4,
        value_bytes: 3,
        unit: Some("kW"),
    },
    Dlt645DataIdentifierTemplate {
        template_id: "active_power_a",
        name: "A 相有功功率",
        semantic_id: "electric.power.active.a",
        data_identifier: "02030100",
        value_type: TelemetryType::Float,
        decimal_places: 4,
        value_bytes: 3,
        unit: Some("kW"),
    },
    Dlt645DataIdentifierTemplate {
        template_id: "reactive_power_total",
        name: "总无功功率",
        semantic_id: "electric.power.reactive.total",
        data_identifier: "02040000",
        value_type: TelemetryType::Float,
        decimal_places: 4,
        value_bytes: 3,
        unit: Some("kvar"),
    },
    Dlt645DataIdentifierTemplate {
        template_id: "apparent_power_total",
        name: "总视在功率",
        semantic_id: "electric.power.apparent.total",
        data_identifier: "02050000",
        value_type: TelemetryType::Float,
        decimal_places: 4,
        value_bytes: 3,
        unit: Some("kVA"),
    },
    Dlt645DataIdentifierTemplate {
        template_id: "power_factor_total",
        name: "总功率因数",
        semantic_id: "electric.power_factor.total",
        data_identifier: "02060000",
        value_type: TelemetryType::Float,
        decimal_places: 3,
        value_bytes: 2,
        unit: None,
    },
    Dlt645DataIdentifierTemplate {
        template_id: "frequency",
        name: "电网频率",
        semantic_id: "electric.frequency",
        data_identifier: "02800002",
        value_type: TelemetryType::Float,
        decimal_places: 2,
        value_bytes: 2,
        unit: Some("Hz"),
    },
    Dlt645DataIdentifierTemplate {
        template_id: "meter_number",
        name: "电表编号",
        semantic_id: "meter.serial_number",
        data_identifier: "04000401",
        value_type: TelemetryType::Text,
        decimal_places: 0,
        value_bytes: 6,
        unit: None,
    },
];

pub fn dlt645_data_identifier_templates() -> &'static [Dlt645DataIdentifierTemplate] {
    DLT645_DATA_IDENTIFIER_TEMPLATES
}

pub fn dlt645_template_by_identifier(
    data_identifier: u32,
) -> Option<&'static Dlt645DataIdentifierTemplate> {
    DLT645_DATA_IDENTIFIER_TEMPLATES.iter().find(|template| {
        u32::from_str_radix(template.data_identifier, 16).ok() == Some(data_identifier)
    })
}

pub fn parse_dlt645_point_address(value: &str) -> Result<Dlt645PointAddress, String> {
    let mut parts = value.split(':');
    let meter_address = parts.next().unwrap_or_default().trim();
    if meter_address.len() != 12 || !meter_address.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("DL/T 645 meter address must contain 12 decimal digits".to_string());
    }

    let data_identifier_text = parts
        .next()
        .ok_or_else(|| "DL/T 645 data identifier is required".to_string())?;
    let data_identifier_text = data_identifier_text
        .strip_prefix("0x")
        .or_else(|| data_identifier_text.strip_prefix("0X"))
        .unwrap_or(data_identifier_text);
    if data_identifier_text.len() != 8 {
        return Err("DL/T 645 data identifier must contain 8 hexadecimal digits".to_string());
    }
    let data_identifier = u32::from_str_radix(data_identifier_text, 16)
        .map_err(|_| format!("invalid DL/T 645 data identifier: {data_identifier_text}"))?;
    let decimal_places_text = parts.next();
    let decimal_places = decimal_places_text
        .filter(|text| !text.is_empty())
        .map(|text| {
            text.parse::<u8>()
                .map_err(|_| format!("invalid DL/T 645 decimal places: {text}"))
        })
        .transpose()?
        .unwrap_or(0);
    let value_bytes = parts
        .next()
        .map(|text| {
            text.parse::<u8>()
                .map_err(|_| format!("invalid DL/T 645 response value byte length: {text}"))
        })
        .transpose()?;
    if parts.next().is_some() {
        return Err(
            "DL/T 645 address must be meter:data_identifier[:decimal_places[:value_bytes]]"
                .to_string(),
        );
    }
    if decimal_places > 18 {
        return Err("DL/T 645 decimal places cannot exceed 18".to_string());
    }
    if matches!(value_bytes, Some(0)) {
        return Err("DL/T 645 response value byte length must be at least 1".to_string());
    }
    if matches!(value_bytes, Some(252..)) {
        return Err("DL/T 645 response value byte length cannot exceed 251".to_string());
    }

    Ok(Dlt645PointAddress {
        meter_address: meter_address.to_string(),
        data_identifier,
        decimal_places,
        value_bytes,
    })
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModbusByteOrder {
    #[default]
    BigEndian,
    LittleEndian,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModbusWordOrder {
    #[default]
    HighWordFirst,
    LowWordFirst,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModbusRegisterEncoding {
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    F32,
    F64,
}

impl ModbusRegisterEncoding {
    pub fn register_count(self) -> u16 {
        match self {
            Self::U16 | Self::I16 => 1,
            Self::U32 | Self::I32 | Self::F32 => 2,
            Self::U64 | Self::I64 | Self::F64 => 4,
        }
    }

    pub fn is_float(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    pub fn is_integer(self) -> bool {
        !self.is_float()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModbusPointOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<ModbusRegisterEncoding>,
    #[serde(default)]
    pub byte_order: ModbusByteOrder,
    #[serde(default)]
    pub word_order: ModbusWordOrder,
    #[serde(default = "default_modbus_scale")]
    pub scale: f64,
    #[serde(default)]
    pub offset: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bit_index: Option<u8>,
}

impl Default for ModbusPointOptions {
    fn default() -> Self {
        Self {
            encoding: None,
            byte_order: ModbusByteOrder::BigEndian,
            word_order: ModbusWordOrder::HighWordFirst,
            scale: 1.0,
            offset: 0.0,
            bit_index: None,
        }
    }
}

fn default_modbus_scale() -> f64 {
    1.0
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CustomSerialChecksum {
    #[default]
    None,
    Sum8,
    Xor8,
    ModbusCrc16,
    Crc16CcittFalse,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CustomSerialFrameEncoding {
    #[default]
    Raw,
    Slip,
    Cobs,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CustomSerialValueEncoding {
    BoolU8,
    U8,
    I8,
    U16Be,
    U16Le,
    I16Be,
    I16Le,
    U32Be,
    U32Le,
    I32Be,
    I32Le,
    F32Be,
    F32Le,
    F64Be,
    F64Le,
    Utf8,
}

impl CustomSerialValueEncoding {
    pub fn fixed_width(self) -> Option<usize> {
        match self {
            Self::BoolU8 | Self::U8 | Self::I8 => Some(1),
            Self::U16Be | Self::U16Le | Self::I16Be | Self::I16Le => Some(2),
            Self::U32Be | Self::U32Le | Self::I32Be | Self::I32Le | Self::F32Be | Self::F32Le => {
                Some(4)
            }
            Self::F64Be | Self::F64Le => Some(8),
            Self::Utf8 => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomSerialPointSpec {
    #[serde(default = "default_custom_serial_schema_version")]
    pub schema_version: u32,
    pub request_hex: String,
    #[serde(default)]
    pub request_checksum: CustomSerialChecksum,
    #[serde(default)]
    pub response_checksum: CustomSerialChecksum,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_prefix_hex: Option<String>,
    #[serde(default)]
    pub frame_encoding: CustomSerialFrameEncoding,
    pub value_offset: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_length: Option<usize>,
    pub value_encoding: CustomSerialValueEncoding,
    #[serde(default = "default_custom_serial_scale")]
    pub scale: f64,
    #[serde(default)]
    pub offset: f64,
}

impl CustomSerialPointSpec {
    pub fn new(
        request_hex: impl Into<String>,
        value_offset: usize,
        value_encoding: CustomSerialValueEncoding,
    ) -> Self {
        Self {
            schema_version: 1,
            request_hex: request_hex.into(),
            request_checksum: CustomSerialChecksum::None,
            response_checksum: CustomSerialChecksum::None,
            response_prefix_hex: None,
            frame_encoding: CustomSerialFrameEncoding::Raw,
            value_offset,
            value_length: value_encoding.fixed_width(),
            value_encoding,
            scale: 1.0,
            offset: 0.0,
        }
    }

    pub fn value_width(&self) -> Result<usize, String> {
        match self.value_encoding.fixed_width() {
            Some(width) => {
                if let Some(configured) = self.value_length {
                    if configured != width {
                        return Err(format!(
                            "valueLength must be {width} for {:?}",
                            self.value_encoding
                        ));
                    }
                }
                Ok(width)
            }
            None => self
                .value_length
                .filter(|length| *length > 0)
                .ok_or_else(|| "valueLength is required for utf8 values".to_string()),
        }
    }
}

fn default_custom_serial_schema_version() -> u32 {
    1
}

fn default_custom_serial_scale() -> f64 {
    1.0
}

pub fn decode_custom_serial_hex(value: &str) -> Result<Vec<u8>, String> {
    let value = value.trim();
    let value = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    let compact = value
        .chars()
        .filter(|character| !character.is_ascii_whitespace() && !matches!(character, ':' | '-'))
        .collect::<String>();
    if compact.len() % 2 != 0 {
        return Err("hex value must contain complete byte pairs".to_string());
    }
    compact
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).expect("hex pairs are valid UTF-8");
            u8::from_str_radix(pair, 16).map_err(|_| format!("invalid hex byte: {pair}"))
        })
        .collect()
}

pub fn validate_custom_serial_point_spec(spec: &CustomSerialPointSpec) -> Result<(), String> {
    if !matches!(spec.schema_version, 1 | 2) {
        return Err(format!(
            "unsupported custom serial DSL schemaVersion {}; supported versions are 1 and 2",
            spec.schema_version
        ));
    }
    if spec.schema_version == 1 && spec.frame_encoding != CustomSerialFrameEncoding::Raw {
        return Err(
            "custom serial DSL schemaVersion 1 only supports raw frameEncoding".to_string(),
        );
    }
    let request = decode_custom_serial_hex(&spec.request_hex)?;
    if request.is_empty() {
        return Err("requestHex must contain at least one byte".to_string());
    }
    if request.len() > 1024 {
        return Err("requestHex exceeds the 1024-byte limit".to_string());
    }
    if let Some(prefix) = &spec.response_prefix_hex {
        let prefix = decode_custom_serial_hex(prefix)?;
        if prefix.len() > 256 {
            return Err("responsePrefixHex exceeds the 256-byte limit".to_string());
        }
    }
    if !spec.scale.is_finite() || !spec.offset.is_finite() {
        return Err("scale and offset must be finite numbers".to_string());
    }
    let width = spec.value_width()?;
    let end = spec
        .value_offset
        .checked_add(width)
        .ok_or_else(|| "value range overflows".to_string())?;
    if end > 4096 {
        return Err("value range exceeds the 4096-byte response limit".to_string());
    }
    Ok(())
}

impl PointAddress {
    pub fn simulated(value: impl Into<String>) -> Self {
        Self {
            kind: "simulated".to_string(),
            value: value.into(),
            modbus: None,
        }
    }

    pub fn modbus_holding_register(address: u32) -> Self {
        Self {
            kind: "holding_register".to_string(),
            value: address.to_string(),
            modbus: None,
        }
    }

    pub fn bacnet(
        device_instance: u32,
        object_type: impl AsRef<str>,
        object_instance: u32,
        property: impl AsRef<str>,
    ) -> Self {
        Self {
            kind: "bacnet_object_property".to_string(),
            value: format!(
                "{}:{}:{}:{}",
                device_instance,
                object_type.as_ref(),
                object_instance,
                property.as_ref()
            ),
            modbus: None,
        }
    }

    pub fn with_modbus_options(mut self, options: ModbusPointOptions) -> Self {
        self.modbus = Some(options);
        self
    }

    pub fn dlt645(meter_address: impl AsRef<str>, data_identifier: impl AsRef<str>) -> Self {
        Self {
            kind: "dlt645_address".to_string(),
            value: format!("{}:{}", meter_address.as_ref(), data_identifier.as_ref()),
            modbus: None,
        }
    }

    pub fn dlt645_scaled(
        meter_address: impl AsRef<str>,
        data_identifier: impl AsRef<str>,
        decimal_places: u8,
    ) -> Self {
        Self {
            kind: "dlt645_address".to_string(),
            value: format!(
                "{}:{}:{}",
                meter_address.as_ref(),
                data_identifier.as_ref(),
                decimal_places
            ),
            modbus: None,
        }
    }

    pub fn dlt645_vendor(
        meter_address: impl AsRef<str>,
        data_identifier: impl AsRef<str>,
        decimal_places: u8,
        value_bytes: u8,
    ) -> Self {
        Self {
            kind: "dlt645_address".to_string(),
            value: format!(
                "{}:{}:{}:{}",
                meter_address.as_ref(),
                data_identifier.as_ref(),
                decimal_places,
                value_bytes
            ),
            modbus: None,
        }
    }

    pub fn iec101(link_address: u8, common_address: u16, information_object_address: u32) -> Self {
        Self {
            kind: "iec101_ioa".to_string(),
            value: format!("{link_address}:{common_address}:{information_object_address}"),
            modbus: None,
        }
    }

    pub fn iec104(common_address: u16, information_object_address: u32) -> Self {
        Self {
            kind: "iec104_ioa".to_string(),
            value: format!("{common_address}:{information_object_address}"),
            modbus: None,
        }
    }

    pub fn custom_serial(spec: &CustomSerialPointSpec) -> Result<Self, serde_json::Error> {
        Ok(Self {
            kind: "custom_serial_frame".to_string(),
            value: serde_json::to_string(spec)?,
            modbus: None,
        })
    }

    pub fn opc_ua_node_id(node_id: impl Into<String>) -> Self {
        Self {
            kind: "node_id".to_string(),
            value: node_id.into(),
            modbus: None,
        }
    }

    pub fn opc_ua_browse_path(address: &OpcUaBrowsePathAddress) -> Result<Self, serde_json::Error> {
        Ok(Self {
            kind: "browse_path".to_string(),
            value: serde_json::to_string(address)?,
            modbus: None,
        })
    }

    pub fn siemens_s7(address: impl Into<String>) -> Self {
        Self {
            kind: "s7_address".to_string(),
            value: address.into(),
            modbus: None,
        }
    }

    pub fn omron_fins(address: impl Into<String>) -> Self {
        Self {
            kind: "fins_address".to_string(),
            value: address.into(),
            modbus: None,
        }
    }
}

pub fn validate_iec104_endpoint(endpoint: &str) -> Result<(), String> {
    let target = endpoint.strip_prefix("tcp://").unwrap_or(endpoint);
    if endpoint.contains("://") && target == endpoint {
        return Err("IEC 104 endpoint must use host:port or tcp://host:port".to_string());
    }
    let (host, port) = if let Some(bracketed) = target.strip_prefix('[') {
        let (host, port) = bracketed
            .split_once("]:")
            .ok_or_else(|| "IEC 104 IPv6 endpoint must use [address]:port".to_string())?;
        (host, port)
    } else {
        target
            .rsplit_once(':')
            .ok_or_else(|| "IEC 104 endpoint must include a TCP port".to_string())?
    };
    if host.trim().is_empty() || host.chars().any(char::is_whitespace) {
        return Err("IEC 104 endpoint host is invalid".to_string());
    }
    let port = port
        .parse::<u16>()
        .map_err(|_| "IEC 104 endpoint port is invalid".to_string())?;
    if port == 0 {
        return Err("IEC 104 endpoint port must be greater than zero".to_string());
    }
    Ok(())
}

pub fn parse_iec104_point_address(value: &str) -> Result<(u16, u32), String> {
    let (common_address, information_object_address) = value
        .split_once(':')
        .ok_or_else(|| "IEC 104 address must be common_address:ioa".to_string())?;
    let common_address = common_address
        .parse::<u16>()
        .map_err(|_| "IEC 104 common address is invalid".to_string())?;
    if common_address == 0 {
        return Err("IEC 104 common address must be greater than zero".to_string());
    }
    let information_object_address = information_object_address
        .parse::<u32>()
        .map_err(|_| "IEC 104 information object address is invalid".to_string())?;
    if information_object_address > 0x00FF_FFFF {
        return Err("IEC 104 information object address exceeds 3-byte range".to_string());
    }
    Ok((common_address, information_object_address))
}

pub fn parse_iec101_point_address(value: &str) -> Result<(u8, u16, u32), String> {
    let parts = value.split(':').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err("IEC 101 address must be link_address:common_address:ioa".to_string());
    }
    let link_address = parts[0]
        .parse::<u8>()
        .map_err(|_| "IEC 101 link address is invalid".to_string())?;
    if link_address == u8::MAX {
        return Err("IEC 101 link address 255 is reserved for broadcast".to_string());
    }
    let common_address = parts[1]
        .parse::<u16>()
        .map_err(|_| "IEC 101 common address is invalid".to_string())?;
    if common_address == 0 {
        return Err("IEC 101 common address must be greater than zero".to_string());
    }
    let information_object_address = parts[2]
        .parse::<u32>()
        .map_err(|_| "IEC 101 information object address is invalid".to_string())?;
    if information_object_address > 0x00FF_FFFF {
        return Err("IEC 101 information object address exceeds 3-byte range".to_string());
    }
    Ok((link_address, common_address, information_object_address))
}

pub fn validate_iec101_point(
    address: &PointAddress,
    value_type: TelemetryType,
    access: PointAccess,
    options: Option<Iec101PointOptions>,
) -> Result<(), String> {
    if address.kind != "iec101_ioa" {
        return Err("IEC 101 points require iec101_ioa address kind".to_string());
    }
    parse_iec101_point_address(&address.value)?;
    if access.is_writable() && options.is_none() {
        return Err("writable IEC 101 points require a controlType".to_string());
    }
    if let Some(options) = options {
        if !options.control_type.accepts(value_type) {
            return Err(format!(
                "IEC 101 control type {:?} is incompatible with {:?} telemetry",
                options.control_type, value_type
            ));
        }
    }
    Ok(())
}

pub fn validate_iec104_point(
    address: &PointAddress,
    value_type: TelemetryType,
    access: PointAccess,
    options: Option<Iec104PointOptions>,
) -> Result<(), String> {
    if address.kind != "iec104_ioa" {
        return Err("IEC 104 points require iec104_ioa address kind".to_string());
    }
    parse_iec104_point_address(&address.value)?;
    if access.is_writable() && options.is_none() {
        return Err("writable IEC 104 points require a controlType".to_string());
    }
    if let Some(options) = options {
        options.validate(value_type)?;
    }
    Ok(())
}

pub fn validate_opc_ua_node_id(value: &str) -> Result<(), String> {
    let (namespace, identifier) = value
        .split_once(';')
        .map_or((None, value), |(namespace, identifier)| {
            (Some(namespace), identifier)
        });
    if let Some(namespace) = namespace {
        let index = namespace
            .strip_prefix("ns=")
            .ok_or_else(|| "OPC UA NodeId namespace must use ns=<index>".to_string())?;
        index
            .parse::<u16>()
            .map_err(|_| "OPC UA NodeId namespace index is invalid".to_string())?;
    }
    let (kind, identifier) = identifier
        .split_once('=')
        .ok_or_else(|| "OPC UA NodeId must use i=, s=, g= or b=".to_string())?;
    if !matches!(kind, "i" | "s" | "g" | "b") || identifier.trim().is_empty() {
        return Err("OPC UA NodeId must use a non-empty i=, s=, g= or b= identifier".to_string());
    }
    if kind == "i" {
        identifier
            .parse::<u32>()
            .map_err(|_| "OPC UA numeric NodeId is invalid".to_string())?;
    }
    Ok(())
}

pub fn validate_opc_ua_point(
    address: &PointAddress,
    value_type: TelemetryType,
    access: PointAccess,
    options: Option<OpcUaPointOptions>,
) -> Result<(), String> {
    match address.kind.as_str() {
        "node_id" => validate_opc_ua_node_id(&address.value)?,
        "browse_path" => {
            parse_opc_ua_browse_path(&address.value)?;
        }
        _ => return Err("OPC UA points require node_id or browse_path address kind".to_string()),
    }

    if access.is_writable() && options.is_none() {
        return Err("writable OPC UA points require a writeDataType".to_string());
    }
    if let Some(options) = options {
        options.validate(value_type)?;
    }
    Ok(())
}

pub fn parse_opc_ua_browse_path(value: &str) -> Result<OpcUaBrowsePathAddress, String> {
    const MAX_BROWSE_PATH_ELEMENTS: usize = 32;
    const MAX_BROWSE_NAME_BYTES: usize = 256;

    let path: OpcUaBrowsePathAddress = serde_json::from_str(value)
        .map_err(|error| format!("OPC UA BrowsePath JSON is invalid: {error}"))?;
    validate_opc_ua_node_id(&path.starting_node)
        .map_err(|error| format!("OPC UA BrowsePath starting node is invalid: {error}"))?;
    if path.elements.is_empty() {
        return Err("OPC UA BrowsePath requires at least one target element".to_string());
    }
    if path.elements.len() > MAX_BROWSE_PATH_ELEMENTS {
        return Err(format!(
            "OPC UA BrowsePath cannot exceed {MAX_BROWSE_PATH_ELEMENTS} elements"
        ));
    }
    for (index, element) in path.elements.iter().enumerate() {
        let name = element.target_name.trim();
        if name.is_empty() {
            return Err(format!(
                "OPC UA BrowsePath element {} target name is required",
                index + 1
            ));
        }
        if name.len() > MAX_BROWSE_NAME_BYTES {
            return Err(format!(
                "OPC UA BrowsePath element {} target name exceeds {MAX_BROWSE_NAME_BYTES} bytes",
                index + 1
            ));
        }
    }
    Ok(path)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiscoveryReport {
    pub job_id: String,
    pub protocol_connection_id: String,
    pub discovered_points: Vec<DiscoveredPoint>,
    pub suggestions: Vec<PointMappingSuggestion>,
}

pub const MAX_DISCOVERY_POINTS: u16 = 128;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryAddressKind {
    HoldingRegister,
    OpcUaBrowse,
}

fn default_discovery_max_depth() -> u8 {
    3
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoveryRequest {
    pub job_id: String,
    pub protocol_connection_id: String,
    pub address_kind: DiscoveryAddressKind,
    #[serde(default)]
    pub start_address: u32,
    #[serde(default)]
    pub end_address: u32,
    #[serde(default = "default_modbus_discovery_slave_id")]
    pub slave_id: u8,
    #[serde(default)]
    pub root_node_id: Option<String>,
    #[serde(default = "default_discovery_max_depth")]
    pub max_depth: u8,
    #[serde(default)]
    pub include_standard_namespace: bool,
}

fn default_modbus_discovery_slave_id() -> u8 {
    1
}

impl DiscoveryRequest {
    pub fn modbus_holding_registers(
        job_id: impl Into<String>,
        protocol_connection_id: impl Into<String>,
        start_address: u32,
        end_address: u32,
    ) -> Self {
        Self {
            job_id: job_id.into(),
            protocol_connection_id: protocol_connection_id.into(),
            address_kind: DiscoveryAddressKind::HoldingRegister,
            start_address,
            end_address,
            slave_id: 1,
            root_node_id: None,
            max_depth: default_discovery_max_depth(),
            include_standard_namespace: false,
        }
    }

    pub fn opc_ua_browse(
        job_id: impl Into<String>,
        protocol_connection_id: impl Into<String>,
        root_node_id: impl Into<String>,
        max_depth: u8,
    ) -> Self {
        Self {
            job_id: job_id.into(),
            protocol_connection_id: protocol_connection_id.into(),
            address_kind: DiscoveryAddressKind::OpcUaBrowse,
            start_address: 0,
            end_address: 0,
            slave_id: 1,
            root_node_id: Some(root_node_id.into()),
            max_depth,
            include_standard_namespace: false,
        }
    }

    pub fn including_standard_namespace(mut self, include: bool) -> Self {
        self.include_standard_namespace = include;
        self
    }

    pub fn with_slave_id(mut self, slave_id: u8) -> Self {
        self.slave_id = slave_id;
        self
    }

    pub fn point_count(&self) -> Result<u16, String> {
        if self.address_kind == DiscoveryAddressKind::OpcUaBrowse {
            return Ok(0);
        }
        if self.start_address > self.end_address {
            return Err("discovery start address must not exceed end address".to_string());
        }
        let count = self
            .end_address
            .checked_sub(self.start_address)
            .and_then(|span| span.checked_add(1))
            .ok_or_else(|| "discovery address range overflows".to_string())?;
        u16::try_from(count).map_err(|_| "discovery address range is too large".to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.job_id.trim().is_empty() {
            return Err("discovery job id is required".to_string());
        }
        if self.protocol_connection_id.trim().is_empty() {
            return Err("discovery protocol connection id is required".to_string());
        }
        match self.address_kind {
            DiscoveryAddressKind::HoldingRegister => {
                if self.slave_id == 0 || self.slave_id > 247 {
                    return Err("Modbus discovery slave id must be between 1 and 247".to_string());
                }
                let point_count = self.point_count()?;
                if point_count > MAX_DISCOVERY_POINTS {
                    return Err(format!(
                        "discovery range exceeds the {MAX_DISCOVERY_POINTS}-point safety limit"
                    ));
                }
                if self.start_address < 40001 || self.end_address > 105536 {
                    return Err(
                        "holding register discovery addresses must be between 40001 and 105536"
                            .to_string(),
                    );
                }
            }
            DiscoveryAddressKind::OpcUaBrowse => {
                let root_node_id = self
                    .root_node_id
                    .as_deref()
                    .ok_or_else(|| "OPC UA discovery root NodeId is required".to_string())?;
                validate_opc_ua_node_id(root_node_id)?;
                if !(1..=8).contains(&self.max_depth) {
                    return Err("OPC UA discovery max depth must be between 1 and 8".to_string());
                }
            }
        }
        Ok(())
    }
}

impl DiscoveryReport {
    pub fn new(job_id: impl Into<String>, protocol_connection_id: impl Into<String>) -> Self {
        Self {
            job_id: job_id.into(),
            protocol_connection_id: protocol_connection_id.into(),
            discovered_points: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    pub fn with_point(mut self, point: DiscoveredPoint) -> Self {
        self.discovered_points.push(point);
        self
    }

    pub fn with_suggestion(mut self, suggestion: PointMappingSuggestion) -> Self {
        self.suggestions.push(suggestion);
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiscoveredPoint {
    pub protocol_connection_id: String,
    pub address: PointAddress,
    pub value_type: TelemetryType,
    pub sample_values: Vec<String>,
    pub confidence: f64,
}

impl DiscoveredPoint {
    pub fn new(
        protocol_connection_id: impl Into<String>,
        address: PointAddress,
        value_type: TelemetryType,
    ) -> Self {
        Self {
            protocol_connection_id: protocol_connection_id.into(),
            address,
            value_type,
            sample_values: Vec::new(),
            confidence: 0.0,
        }
    }

    pub fn with_sample_values(mut self, sample_values: Vec<String>) -> Self {
        self.sample_values = sample_values;
        self
    }

    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence;
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PointMappingSuggestion {
    pub point_id: String,
    pub device_id: String,
    pub semantic_id: String,
    pub protocol_connection_id: String,
    pub address: PointAddress,
    pub value_type: TelemetryType,
    pub unit: Option<String>,
    pub confidence: f64,
    pub evidence: String,
}

impl PointMappingSuggestion {
    pub fn new(
        point_id: impl Into<String>,
        device_id: impl Into<String>,
        semantic_id: impl Into<String>,
        protocol_connection_id: impl Into<String>,
        address: PointAddress,
        value_type: TelemetryType,
    ) -> Self {
        Self {
            point_id: point_id.into(),
            device_id: device_id.into(),
            semantic_id: semantic_id.into(),
            protocol_connection_id: protocol_connection_id.into(),
            address,
            value_type,
            unit: None,
            confidence: 0.0,
            evidence: String::new(),
        }
    }

    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence;
        self
    }

    pub fn with_evidence(mut self, evidence: impl Into<String>) -> Self {
        self.evidence = evidence.into();
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CollectionTask {
    pub task_id: String,
    pub device_id: String,
    pub point_ids: Vec<String>,
    pub interval_ms: u64,
    pub enabled: bool,
}

impl CollectionTask {
    pub fn interval(
        task_id: impl Into<String>,
        device_id: impl Into<String>,
        point_ids: Vec<String>,
        interval_ms: u64,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            device_id: device_id.into(),
            point_ids,
            interval_ms,
            enabled: true,
        }
    }
}
