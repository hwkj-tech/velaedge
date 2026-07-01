use std::collections::BTreeMap;

use anyhow::{bail, Result};
use chrono::Utc;
use edge_core::{DataQuality, DeviceShadow, EdgeConfigPackage, TelemetrySample, TelemetryValue};

use crate::{
    publish_data_config_mqtt_samples, publish_mqtt_samples, CollectionReport, MqttPublisher,
};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfiguredMqttCollectionReport {
    pub collection: CollectionReport,
    pub mqtt_messages_published: usize,
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
        let samples = self.collect_samples_once().await;
        Ok(CollectionReport {
            samples_collected: samples.len(),
        })
    }

    pub async fn collect_once_and_publish_mqtt<P>(
        &mut self,
        publisher: &mut P,
    ) -> Result<ConfiguredMqttCollectionReport>
    where
        P: MqttPublisher + ?Sized,
    {
        let samples = self.collect_samples_once().await;
        let mqtt_messages_published =
            publish_mqtt_samples(self.applied.package(), &samples, publisher).await?;
        Ok(ConfiguredMqttCollectionReport {
            collection: CollectionReport {
                samples_collected: samples.len(),
            },
            mqtt_messages_published,
        })
    }

    pub async fn collect_data_configs_once_and_publish_mqtt<P>(
        &mut self,
        publisher: &mut P,
    ) -> Result<ConfiguredMqttCollectionReport>
    where
        P: MqttPublisher + ?Sized,
    {
        let samples = self.collect_data_config_samples_once().await;
        let mqtt_messages_published =
            publish_data_config_mqtt_samples(self.applied.package(), &samples, publisher).await?;
        Ok(ConfiguredMqttCollectionReport {
            collection: CollectionReport {
                samples_collected: samples.len(),
            },
            mqtt_messages_published,
        })
    }

    async fn collect_samples_once(&mut self) -> Vec<TelemetrySample> {
        let mut samples = Vec::new();
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
                shadow.update(sample.clone());
                samples_collected += 1;
                samples.push(sample);
            }
        }
        debug_assert_eq!(samples_collected, samples.len());
        samples
    }

    async fn collect_data_config_samples_once(&mut self) -> Vec<TelemetrySample> {
        let mut samples = Vec::new();
        for data_config in &self.applied.package().data_configs {
            if !data_config.enabled {
                continue;
            }

            for point in &data_config.points {
                let sample = TelemetrySample::new(
                    &data_config.device_id,
                    &point.point_id,
                    TelemetryValue::Float(1.0),
                    DataQuality::Good,
                    Utc::now(),
                );
                if let Some(shadow) = self.shadows.get_mut(&data_config.device_id) {
                    shadow.update(sample.clone());
                    samples.push(sample);
                }
            }
        }
        samples
    }

    pub fn reported_version(&self) -> &str {
        self.applied.version()
    }

    pub fn shadow(&self, device_id: &str) -> Option<&DeviceShadow> {
        self.shadows.get(device_id)
    }
}
