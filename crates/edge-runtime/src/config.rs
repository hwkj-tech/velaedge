use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Result};
use chrono::Utc;
use edge_core::{
    validate_command_flow, validate_data_config_visual_graph, validate_iec101_point,
    validate_iec104_point, validate_omron_fins_point, validate_point_access,
    validate_siemens_s7_point, DataQuality, DeviceShadow, EdgeConfigPackage, ProtocolType,
    TelemetrySample, TelemetryValue, MAX_DATA_CONFIG_RETRY_COUNT, MAX_DATA_CONFIG_TIMEOUT_MS,
};

use crate::{
    publish_data_config_mqtt_samples, publish_data_config_mqtt_samples_with_outbox,
    publish_mqtt_samples, publish_mqtt_samples_with_outbox, CollectionReport, MqttPublisher,
    RocksEdgeRuntimeStore,
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
        validate_config_references(&package)?;
        Ok(Self { package })
    }

    pub fn version(&self) -> &str {
        &self.package.version
    }

    pub fn package(&self) -> &EdgeConfigPackage {
        &self.package
    }
}

pub(crate) fn validate_config_references(package: &EdgeConfigPackage) -> Result<()> {
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
    let connections = package
        .protocol_connections
        .iter()
        .map(|connection| (connection.connection_id.as_str(), connection))
        .collect::<BTreeMap<_, _>>();
    let sink_ids = package
        .mqtt_uplinks
        .iter()
        .map(|uplink| uplink.sink_id.as_str())
        .collect::<BTreeSet<_>>();
    let point_mappings = package
        .point_mappings
        .iter()
        .map(|mapping| (mapping.point_id.as_str(), mapping))
        .collect::<BTreeMap<_, _>>();
    let algorithm_ids = package
        .algorithms
        .iter()
        .map(|algorithm| algorithm.id.as_str())
        .collect::<BTreeSet<_>>();

    for connection in &package.protocol_connections {
        if let Err(message) = connection.validate() {
            bail!(
                "protocol connection {}: {}",
                connection.connection_id,
                message
            );
        }
    }

    for mapping in &package.point_mappings {
        validate_point_access(&mapping.address, mapping.access).map_err(anyhow::Error::msg)?;
        let connection = connections
            .get(mapping.protocol_connection_id.as_str())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "point {} references missing protocol connection {}",
                    mapping.point_id,
                    mapping.protocol_connection_id
                )
            })?;
        if connection.protocol == ProtocolType::SiemensS7 {
            validate_siemens_s7_point(&mapping.address, mapping.value_type, mapping.access)
                .map_err(anyhow::Error::msg)?;
        }
        if connection.protocol == ProtocolType::OmronFins {
            validate_omron_fins_point(&mapping.address, mapping.value_type, mapping.access)
                .map_err(anyhow::Error::msg)?;
        }
        if connection.protocol == ProtocolType::Iec101 {
            validate_iec101_point(
                &mapping.address,
                mapping.value_type,
                mapping.access,
                mapping.iec101,
            )
            .map_err(anyhow::Error::msg)?;
        }
        if connection.protocol == ProtocolType::Iec104 {
            validate_iec104_point(
                &mapping.address,
                mapping.value_type,
                mapping.access,
                mapping.iec104,
            )
            .map_err(anyhow::Error::msg)?;
        }
    }

    for task in &package.collection_tasks {
        if task.task_id.trim().is_empty() {
            bail!("collection task id is required");
        }
        if task.interval_ms == 0 {
            bail!(
                "collection task {} interval must be greater than zero",
                task.task_id
            );
        }
        if !device_ids.contains(task.device_id.as_str()) {
            bail!(
                "collection task {} references missing device {}",
                task.task_id,
                task.device_id
            );
        }
        if task.point_ids.is_empty() {
            bail!(
                "collection task {} must include at least one point",
                task.task_id
            );
        }
        for point_id in &task.point_ids {
            let Some(mapping) = point_mappings.get(point_id.as_str()) else {
                bail!(
                    "collection task {} references missing point {}",
                    task.task_id,
                    point_id
                );
            };
            if mapping.device_id != task.device_id {
                bail!(
                    "collection task {} point {} belongs to device {}, expected {}",
                    task.task_id,
                    point_id,
                    mapping.device_id,
                    task.device_id
                );
            }
            if !mapping.access.is_readable() {
                bail!(
                    "collection task {} references write-only point {}",
                    task.task_id,
                    point_id
                );
            }
        }
    }

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
        if data_config.collection.period_ms == 0 {
            bail!(
                "data config {} collection period must be greater than zero",
                data_config.config_id
            );
        }
        if data_config.collection.timeout_ms == 0
            || data_config.collection.timeout_ms > MAX_DATA_CONFIG_TIMEOUT_MS
        {
            bail!(
                "data config {} collection timeout must be between 1 and {} ms",
                data_config.config_id,
                MAX_DATA_CONFIG_TIMEOUT_MS
            );
        }
        if data_config.collection.retry_count > MAX_DATA_CONFIG_RETRY_COUNT {
            bail!(
                "data config {} collection retry count must not exceed {}",
                data_config.config_id,
                MAX_DATA_CONFIG_RETRY_COUNT
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
            let Some(mapping) = point_mappings.get(point.point_id.as_str()) else {
                bail!(
                    "data config {} references missing point {}",
                    data_config.config_id,
                    point.point_id
                );
            };
            if mapping.device_id != data_config.device_id {
                bail!(
                    "data config {} point {} belongs to device {}, expected {}",
                    data_config.config_id,
                    point.point_id,
                    mapping.device_id,
                    data_config.device_id
                );
            }
            if mapping.protocol_connection_id != data_config.protocol_connection_id {
                bail!(
                    "data config {} point {} uses protocol connection {}, expected {}",
                    data_config.config_id,
                    point.point_id,
                    mapping.protocol_connection_id,
                    data_config.protocol_connection_id
                );
            }
            if !mapping.access.is_readable() {
                bail!(
                    "data config {} references write-only point {}",
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

        validate_data_config_visual_graph(data_config).map_err(anyhow::Error::msg)?;
    }

    for command_flow in &package.command_flows {
        if !command_flow.protocol_connection_id.is_empty()
            && !connection_ids.contains(command_flow.protocol_connection_id.as_str())
        {
            bail!(
                "command flow {} references missing protocol connection {}",
                command_flow.flow_id,
                command_flow.protocol_connection_id
            );
        }
        if !sink_ids.contains(command_flow.mqtt_connection_id.as_str()) {
            bail!(
                "command flow {} references missing MQTT connection {}",
                command_flow.flow_id,
                command_flow.mqtt_connection_id
            );
        }
        validate_command_flow(command_flow, &package.point_mappings).map_err(anyhow::Error::msg)?;
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

    pub async fn collect_once_and_publish_mqtt_with_outbox<P>(
        &mut self,
        store: &RocksEdgeRuntimeStore,
        publisher: &mut P,
    ) -> Result<ConfiguredMqttCollectionReport>
    where
        P: MqttPublisher + ?Sized,
    {
        let samples = self.collect_samples_once().await;
        let mqtt_messages_published =
            publish_mqtt_samples_with_outbox(self.applied.package(), &samples, store, publisher)
                .await?;
        Ok(ConfiguredMqttCollectionReport {
            collection: CollectionReport {
                samples_collected: samples.len(),
            },
            mqtt_messages_published,
        })
    }

    pub async fn collect_data_configs_once_and_publish_mqtt_with_outbox<P>(
        &mut self,
        store: &RocksEdgeRuntimeStore,
        publisher: &mut P,
    ) -> Result<ConfiguredMqttCollectionReport>
    where
        P: MqttPublisher + ?Sized,
    {
        let samples = self.collect_data_config_samples_once().await;
        let mqtt_messages_published = publish_data_config_mqtt_samples_with_outbox(
            self.applied.package(),
            &samples,
            store,
            publisher,
        )
        .await?;
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
