use std::collections::{BTreeMap, BTreeSet};

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
        validate_data_configs(&package)?;
        Ok(Self { package })
    }

    pub fn version(&self) -> &str {
        &self.package.version
    }

    pub fn package(&self) -> &EdgeConfigPackage {
        &self.package
    }
}

fn validate_data_configs(package: &EdgeConfigPackage) -> Result<()> {
    let device_ids = package
        .devices
        .iter()
        .map(|device| device.device_id.as_str())
        .collect::<BTreeSet<_>>();
    let connection_ids = package
        .protocol_connections
        .iter()
        .map(|connection| connection.connection_id.as_str())
        .collect::<BTreeSet<_>>();
    let sink_ids = package
        .mqtt_uplinks
        .iter()
        .map(|uplink| uplink.sink_id.as_str())
        .collect::<BTreeSet<_>>();
    let point_ids = package
        .point_mappings
        .iter()
        .map(|mapping| mapping.point_id.as_str())
        .collect::<BTreeSet<_>>();
    let algorithm_ids = package
        .algorithms
        .iter()
        .map(|algorithm| algorithm.id.as_str())
        .collect::<BTreeSet<_>>();

    for data_config in &package.data_configs {
        if data_config.config_id.trim().is_empty() {
            bail!("data config id is required");
        }
        if data_config.points.is_empty() {
            bail!(
                "data config {} must include at least one point",
                data_config.config_id
            );
        }
        if !device_ids.contains(data_config.device_id.as_str()) {
            bail!(
                "data config {} references missing device {}",
                data_config.config_id,
                data_config.device_id
            );
        }
        if !connection_ids.contains(data_config.protocol_connection_id.as_str()) {
            bail!(
                "data config {} references missing protocol connection {}",
                data_config.config_id,
                data_config.protocol_connection_id
            );
        }
        if !sink_ids.contains(data_config.publish.sink_id.as_str()) {
            bail!(
                "data config {} references missing mqtt sink {}",
                data_config.config_id,
                data_config.publish.sink_id
            );
        }

        let mut json_fields = BTreeSet::new();
        for point in &data_config.points {
            if !point_ids.contains(point.point_id.as_str()) {
                bail!(
                    "data config {} references missing point {}",
                    data_config.config_id,
                    point.point_id
                );
            }
            if !json_fields.insert(point.json_field.as_str()) {
                bail!(
                    "data config {} has duplicate json field {}",
                    data_config.config_id,
                    point.json_field
                );
            }
        }

        for algorithm_id in &data_config.algorithm_ids {
            if !algorithm_ids.contains(algorithm_id.as_str()) {
                bail!(
                    "data config {} references missing algorithm {}",
                    data_config.config_id,
                    algorithm_id
                );
            }
        }
    }

    Ok(())
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
