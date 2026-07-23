use std::collections::{BTreeMap, BTreeSet};

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
}

impl ProtocolConnection {
    pub fn simulated(connection_id: impl Into<String>) -> Self {
        Self {
            connection_id: connection_id.into(),
            protocol: ProtocolType::Simulated,
            endpoint: None,
            serial: None,
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
        }
    }

    pub fn modbus_tcp(connection_id: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            connection_id: connection_id.into(),
            protocol: ProtocolType::ModbusTcp,
            endpoint: Some(endpoint.into()),
            serial: None,
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
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProtocolType {
    Simulated,
    ModbusTcp,
    ModbusRtu,
    Dlt645,
    Iec101,
    CustomSerial,
    OpcUa,
    SiemensS7,
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MqttUplinkConfig {
    pub sink_id: String,
    pub broker: String,
    pub client_id: String,
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
pub struct DataConfigCollection {
    pub period_ms: u64,
    pub timeout_ms: u64,
    pub retry_count: u32,
}

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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
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
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PointAddress {
    pub kind: String,
    pub value: String,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CustomSerialChecksum {
    #[default]
    None,
    Sum8,
    Xor8,
    ModbusCrc16,
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
#[serde(rename_all = "camelCase")]
pub struct CustomSerialPointSpec {
    pub request_hex: String,
    #[serde(default)]
    pub request_checksum: CustomSerialChecksum,
    #[serde(default)]
    pub response_checksum: CustomSerialChecksum,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_prefix_hex: Option<String>,
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
            request_hex: request_hex.into(),
            request_checksum: CustomSerialChecksum::None,
            response_checksum: CustomSerialChecksum::None,
            response_prefix_hex: None,
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
        }
    }

    pub fn modbus_holding_register(address: u32) -> Self {
        Self {
            kind: "holding_register".to_string(),
            value: address.to_string(),
        }
    }

    pub fn dlt645(meter_address: impl AsRef<str>, data_identifier: impl AsRef<str>) -> Self {
        Self {
            kind: "dlt645_address".to_string(),
            value: format!("{}:{}", meter_address.as_ref(), data_identifier.as_ref()),
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
        }
    }

    pub fn iec101(link_address: u8, common_address: u16, information_object_address: u32) -> Self {
        Self {
            kind: "iec101_ioa".to_string(),
            value: format!("{link_address}:{common_address}:{information_object_address}"),
        }
    }

    pub fn custom_serial(spec: &CustomSerialPointSpec) -> Result<Self, serde_json::Error> {
        Ok(Self {
            kind: "custom_serial_frame".to_string(),
            value: serde_json::to_string(spec)?,
        })
    }
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
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoveryRequest {
    pub job_id: String,
    pub protocol_connection_id: String,
    pub address_kind: DiscoveryAddressKind,
    pub start_address: u32,
    pub end_address: u32,
    pub slave_id: u8,
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
        }
    }

    pub fn with_slave_id(mut self, slave_id: u8) -> Self {
        self.slave_id = slave_id;
        self
    }

    pub fn point_count(&self) -> Result<u16, String> {
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
                "holding register discovery addresses must be between 40001 and 105536".to_string(),
            );
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
