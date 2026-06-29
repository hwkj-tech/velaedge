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
            publish,
        }
    }

    pub fn with_point(mut self, point: DataConfigPoint) -> Self {
        self.points.push(point);
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
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
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiscoveryReport {
    pub job_id: String,
    pub protocol_connection_id: String,
    pub discovered_points: Vec<DiscoveredPoint>,
    pub suggestions: Vec<PointMappingSuggestion>,
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
