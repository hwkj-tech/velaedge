use chrono::{DateTime, Utc};
use edge_core::{
    AlgorithmSpec, BacnetPointOptions, CollectionTask, CommandFlowConfig, DataConfig,
    DeviceInstance, DeviceSpec, Iec101PointOptions, Iec104PointOptions, MqttUplinkConfig,
    OpcUaPointOptions, PointAccess, PointAddress, ProtocolConnection, ProtocolType, TelemetryType,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub project_id: String,
    pub name: String,
    pub environment: String,
    pub owner: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Project {
    pub fn new(project_id: impl Into<String>, name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            project_id: project_id.into(),
            name: name.into(),
            environment: "staging".to_string(),
            owner: "platform-team".to_string(),
            description: String::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PointSetPoint {
    pub point_id: String,
    pub semantic_id: String,
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
    pub interval_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PointSet {
    pub point_set_id: String,
    pub project_id: String,
    pub name: String,
    pub description: String,
    pub protocol: ProtocolType,
    pub points: Vec<PointSetPoint>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PointSet {
    pub fn new(
        point_set_id: impl Into<String>,
        project_id: impl Into<String>,
        name: impl Into<String>,
        protocol: ProtocolType,
    ) -> Self {
        let now = Utc::now();
        Self {
            point_set_id: point_set_id.into(),
            project_id: project_id.into(),
            name: name.into(),
            description: String::new(),
            protocol,
            points: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub product_id: String,
    pub project_id: String,
    pub name: String,
    pub product_type: String,
    pub description: String,
    pub latest_version: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Product {
    pub fn new(
        product_id: impl Into<String>,
        project_id: impl Into<String>,
        name: impl Into<String>,
        product_type: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            product_id: product_id.into(),
            project_id: project_id.into(),
            name: name.into(),
            product_type: product_type.into(),
            description: String::new(),
            latest_version: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProductVersionStatus {
    Draft,
    Published,
    Retired,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProductVersion {
    pub product_id: String,
    pub version: String,
    pub status: ProductVersionStatus,
    pub point_set_ids: Vec<String>,
    pub device_models: Vec<DeviceSpec>,
    pub devices: Vec<DeviceInstance>,
    pub protocol_connections: Vec<ProtocolConnection>,
    pub collection_tasks: Vec<CollectionTask>,
    pub algorithms: Vec<AlgorithmSpec>,
    pub data_configs: Vec<DataConfig>,
    #[serde(default)]
    pub command_flows: Vec<CommandFlowConfig>,
    pub mqtt_uplinks: Vec<MqttUplinkConfig>,
    pub created_at: DateTime<Utc>,
}

impl ProductVersion {
    pub fn draft(product_id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            product_id: product_id.into(),
            version: version.into(),
            status: ProductVersionStatus::Draft,
            point_set_ids: Vec::new(),
            device_models: Vec::new(),
            devices: Vec::new(),
            protocol_connections: Vec::new(),
            collection_tasks: Vec::new(),
            algorithms: Vec::new(),
            data_configs: Vec::new(),
            command_flows: Vec::new(),
            mqtt_uplinks: Vec::new(),
            created_at: Utc::now(),
        }
    }
}
