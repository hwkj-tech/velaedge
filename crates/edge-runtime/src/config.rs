use std::collections::BTreeMap;

use anyhow::{bail, Result};
use chrono::Utc;
use edge_core::{
    DataQuality, DeviceShadow, EdgeConfigPackage, TelemetrySample, TelemetryValue,
};

use crate::CollectionReport;

#[derive(Clone, Debug)]
pub struct AppliedEdgeConfig {
    package: EdgeConfigPackage,
}

impl AppliedEdgeConfig {
    pub fn apply(package: EdgeConfigPackage) -> Result<Self> {
        if package.edge_id.trim().is_empty() {
            bail!("edge id is required");
        }
        if package.version.trim().is_empty() {
            bail!("config version is required");
        }
        Ok(Self { package })
    }

    pub fn version(&self) -> &str {
        &self.package.version
    }

    pub fn package(&self) -> &EdgeConfigPackage {
        &self.package
    }
}

pub struct ConfiguredSimulatedRuntime {
    applied: AppliedEdgeConfig,
    shadows: BTreeMap<String, DeviceShadow>,
}

impl ConfiguredSimulatedRuntime {
    pub fn new(applied: AppliedEdgeConfig) -> Self {
        let mut shadows = BTreeMap::new();
        for device in &applied.package().devices {
            shadows.insert(
                device.device_id.clone(),
                DeviceShadow::new(&applied.package().edge_id, &device.device_id),
            );
        }
        Self { applied, shadows }
    }

    pub async fn collect_once(&mut self) -> Result<CollectionReport> {
        let mut samples_collected = 0;
        for mapping in &self.applied.package().point_mappings {
            let sample = TelemetrySample::new(
                &mapping.device_id,
                &mapping.point_id,
                TelemetryValue::Float(1.0),
                DataQuality::Good,
                Utc::now(),
            );
            if let Some(shadow) = self.shadows.get_mut(&mapping.device_id) {
                shadow.update(sample);
                samples_collected += 1;
            }
        }
        Ok(CollectionReport { samples_collected })
    }

    pub fn reported_version(&self) -> &str {
        self.applied.version()
    }

    pub fn shadow(&self, device_id: &str) -> Option<&DeviceShadow> {
        self.shadows.get(device_id)
    }
}
