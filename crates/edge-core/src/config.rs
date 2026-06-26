use serde::{Deserialize, Serialize};

use crate::{AlgorithmSpec, DeviceSpec, NumberRange, TelemetryType};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EdgeConfigPackage {
    pub edge_id: String,
    pub version: String,
    pub device_models: Vec<DeviceSpec>,
    pub devices: Vec<DeviceInstance>,
    pub protocol_connections: Vec<ProtocolConnection>,
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

    pub fn with_point_mapping(mut self, mapping: TelemetryPointMapping) -> Self {
        self.point_mappings.push(mapping);
        self
    }

    pub fn with_collection_task(mut self, task: CollectionTask) -> Self {
        self.collection_tasks.push(task);
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
}

impl ProtocolConnection {
    pub fn simulated(connection_id: impl Into<String>) -> Self {
        Self {
            connection_id: connection_id.into(),
            protocol: ProtocolType::Simulated,
            endpoint: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProtocolType {
    Simulated,
    ModbusTcp,
    OpcUa,
    Mqtt,
    SiemensS7,
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
