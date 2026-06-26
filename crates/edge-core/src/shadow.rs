use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{TelemetrySample, TelemetryValue};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DeviceShadow {
    pub edge_id: String,
    pub device_id: String,
    pub updated_at: Option<DateTime<Utc>>,
    telemetry: BTreeMap<String, TelemetrySample>,
}

impl DeviceShadow {
    pub fn new(edge_id: impl Into<String>, device_id: impl Into<String>) -> Self {
        Self {
            edge_id: edge_id.into(),
            device_id: device_id.into(),
            updated_at: None,
            telemetry: BTreeMap::new(),
        }
    }

    pub fn update(&mut self, sample: TelemetrySample) {
        self.updated_at = Some(sample.timestamp);
        self.telemetry.insert(sample.telemetry_id.clone(), sample);
    }

    pub fn latest(&self, telemetry_id: &str) -> Option<&TelemetrySample> {
        self.telemetry.get(telemetry_id)
    }

    pub fn latest_value(&self, telemetry_id: &str) -> Option<&TelemetryValue> {
        self.latest(telemetry_id).map(|sample| &sample.value)
    }

    pub fn telemetry(&self) -> &BTreeMap<String, TelemetrySample> {
        &self.telemetry
    }
}
