use chrono::{DateTime, Utc};
use edge_core::{AlgorithmSpec, DeviceSpec};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ConfigPackage {
    pub package_id: Uuid,
    pub edge_id: String,
    pub version: String,
    pub device_specs: Vec<DeviceSpec>,
    pub algorithms: Vec<AlgorithmSpec>,
    pub created_at: DateTime<Utc>,
}

impl ConfigPackage {
    pub fn new(edge_id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            package_id: Uuid::new_v4(),
            edge_id: edge_id.into(),
            version: version.into(),
            device_specs: Vec::new(),
            algorithms: Vec::new(),
            created_at: Utc::now(),
        }
    }

    pub fn with_device_spec(mut self, device_spec: DeviceSpec) -> Self {
        self.device_specs.push(device_spec);
        self
    }

    pub fn with_algorithm(mut self, algorithm: AlgorithmSpec) -> Self {
        self.algorithms.push(algorithm);
        self
    }
}
