use std::collections::BTreeMap;

use anyhow::{bail, Result};
use edge_core::{
    CollectionTask, DataConfig, DataQuality, DeviceShadow, EdgeConfigPackage, EdgeRuntimeEvent,
    ProtocolConnection, ProtocolType, RuntimeEventCategory, RuntimeEventSeverity,
    TelemetryPointMapping, TelemetrySample, TelemetryValue,
};

use crate::{
    config::validate_config_references, publish_data_config_mqtt_samples, publish_mqtt_samples,
    AlgorithmEngine, CollectionReport, ConfiguredMqttCollectionReport, ModbusRtuAdapter,
    MqttPublisher, ProtocolAdapter, SerialBusFactory,
};
use crate::{CollectionSchedule, DataConfigSchedule};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduledCollectionReport {
    pub tasks_run: usize,
    pub samples_collected: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledCollectionFailure {
    pub task_id: String,
    pub reason: String,
}

impl ScheduledCollectionFailure {
    pub fn to_runtime_event(&self, edge_id: &str) -> EdgeRuntimeEvent {
        EdgeRuntimeEvent::new(
            edge_id,
            RuntimeEventSeverity::Warning,
            RuntimeEventCategory::Collection,
            "collection.task_failed",
            format!("Collection task {} failed", self.task_id),
        )
        .with_context("task_id", self.task_id.clone())
        .with_context("reason", self.reason.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResilientScheduledCollectionReport {
    pub tasks_run: usize,
    pub tasks_succeeded: usize,
    pub tasks_failed: usize,
    pub samples_collected: usize,
    pub failures: Vec<ScheduledCollectionFailure>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledDataConfigPublishReport {
    pub data_configs_run: usize,
    pub samples_collected: usize,
    pub mqtt_messages_published: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledDataConfigFailure {
    pub config_id: String,
    pub reason: String,
}

impl ScheduledDataConfigFailure {
    pub fn to_runtime_event(&self, edge_id: &str) -> EdgeRuntimeEvent {
        EdgeRuntimeEvent::new(
            edge_id,
            RuntimeEventSeverity::Warning,
            RuntimeEventCategory::Collection,
            "data_config.publish_failed",
            format!("Data config {} failed", self.config_id),
        )
        .with_context("config_id", self.config_id.clone())
        .with_context("reason", self.reason.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResilientScheduledDataConfigPublishReport {
    pub data_configs_run: usize,
    pub data_configs_succeeded: usize,
    pub data_configs_failed: usize,
    pub samples_collected: usize,
    pub mqtt_messages_published: usize,
    pub failures: Vec<ScheduledDataConfigFailure>,
}

pub struct ConfiguredEdgeRuntime<F> {
    package: EdgeConfigPackage,
    serial_bus_factory: F,
    shadows: BTreeMap<String, DeviceShadow>,
    algorithm_engine: AlgorithmEngine,
}

impl<F> ConfiguredEdgeRuntime<F>
where
    F: SerialBusFactory,
{
    pub fn new(package: EdgeConfigPackage, serial_bus_factory: F) -> Result<Self> {
        if package.edge_id.trim().is_empty() {
            bail!("edge id is required");
        }
        if package.version.trim().is_empty() {
            bail!("config version is required");
        }
        validate_config_references(&package)?;

        let mut shadows = BTreeMap::new();
        for device in &package.devices {
            shadows.insert(
                device.device_id.clone(),
                DeviceShadow::new(&package.edge_id, &device.device_id),
            );
        }

        let algorithm_engine = AlgorithmEngine::new(package.algorithms.clone())?;

        Ok(Self {
            package,
            serial_bus_factory,
            shadows,
            algorithm_engine,
        })
    }

    pub async fn collect_once(&mut self) -> Result<CollectionReport> {
        let samples = self.collect_samples_once().await?;
        Ok(CollectionReport {
            samples_collected: samples.len(),
        })
    }

    pub async fn collect_task_once(&mut self, task_id: &str) -> Result<CollectionReport> {
        let task = self
            .package
            .collection_tasks
            .iter()
            .find(|task| task.task_id == task_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("collection task not found: {task_id}"))?;
        if !task.enabled {
            return Ok(CollectionReport {
                samples_collected: 0,
            });
        }

        let samples = self.collect_samples_for_task(&task).await?;
        Ok(CollectionReport {
            samples_collected: samples.len(),
        })
    }

    pub async fn collect_due_tasks_once(
        &mut self,
        schedule: &mut CollectionSchedule,
        now_ms: u64,
    ) -> Result<ScheduledCollectionReport> {
        let due_task_ids = schedule
            .due_task_ids(now_ms)
            .into_iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let mut samples_collected = 0;

        for task_id in &due_task_ids {
            let report = self.collect_task_once(task_id).await?;
            samples_collected += report.samples_collected;
            schedule.mark_ran(task_id, now_ms)?;
        }

        Ok(ScheduledCollectionReport {
            tasks_run: due_task_ids.len(),
            samples_collected,
        })
    }

    pub async fn collect_due_tasks_resilient_once(
        &mut self,
        schedule: &mut CollectionSchedule,
        now_ms: u64,
    ) -> Result<ResilientScheduledCollectionReport> {
        let due_task_ids = schedule
            .due_task_ids(now_ms)
            .into_iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let mut tasks_succeeded = 0;
        let mut samples_collected = 0;
        let mut failures = Vec::new();

        for task_id in &due_task_ids {
            match self.collect_task_once(task_id).await {
                Ok(report) => {
                    tasks_succeeded += 1;
                    samples_collected += report.samples_collected;
                }
                Err(error) => failures.push(ScheduledCollectionFailure {
                    task_id: task_id.clone(),
                    reason: error.to_string(),
                }),
            }
            schedule.mark_ran(task_id, now_ms)?;
        }

        Ok(ResilientScheduledCollectionReport {
            tasks_run: due_task_ids.len(),
            tasks_succeeded,
            tasks_failed: failures.len(),
            samples_collected,
            failures,
        })
    }

    pub async fn collect_once_and_publish_mqtt<P>(
        &mut self,
        publisher: &mut P,
    ) -> Result<ConfiguredMqttCollectionReport>
    where
        P: MqttPublisher + ?Sized,
    {
        let samples = self.collect_samples_once().await?;
        let mqtt_messages_published =
            publish_mqtt_samples(&self.package, &samples, publisher).await?;
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
        let samples = self.collect_data_config_samples_once().await?;
        let mqtt_messages_published =
            publish_data_config_mqtt_samples(&self.package, &samples, publisher).await?;
        Ok(ConfiguredMqttCollectionReport {
            collection: CollectionReport {
                samples_collected: samples.len(),
            },
            mqtt_messages_published,
        })
    }

    pub async fn collect_due_data_configs_once_and_publish_mqtt<P>(
        &mut self,
        schedule: &mut DataConfigSchedule,
        now_ms: u64,
        publisher: &mut P,
    ) -> Result<ScheduledDataConfigPublishReport>
    where
        P: MqttPublisher + ?Sized,
    {
        let due_config_ids = schedule
            .due_config_ids(now_ms)
            .into_iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let mut samples_collected = 0;
        let mut mqtt_messages_published = 0;

        for config_id in &due_config_ids {
            let samples = self.collect_data_config_samples(config_id).await?;
            samples_collected += samples.len();
            mqtt_messages_published += self
                .publish_data_config_samples(config_id, &samples, publisher)
                .await?;
            schedule.mark_ran(config_id, now_ms)?;
        }

        Ok(ScheduledDataConfigPublishReport {
            data_configs_run: due_config_ids.len(),
            samples_collected,
            mqtt_messages_published,
        })
    }

    pub async fn collect_due_data_configs_resilient_once<P>(
        &mut self,
        schedule: &mut DataConfigSchedule,
        now_ms: u64,
        publisher: &mut P,
    ) -> Result<ResilientScheduledDataConfigPublishReport>
    where
        P: MqttPublisher + ?Sized,
    {
        let due_config_ids = schedule
            .due_config_ids(now_ms)
            .into_iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let mut data_configs_succeeded = 0;
        let mut samples_collected = 0;
        let mut mqtt_messages_published = 0;
        let mut failures = Vec::new();

        for config_id in &due_config_ids {
            match self.collect_data_config_samples(config_id).await {
                Ok(samples) => {
                    samples_collected += samples.len();
                    match self
                        .publish_data_config_samples(config_id, &samples, publisher)
                        .await
                    {
                        Ok(published) => {
                            data_configs_succeeded += 1;
                            mqtt_messages_published += published;
                        }
                        Err(error) => failures.push(ScheduledDataConfigFailure {
                            config_id: config_id.clone(),
                            reason: error.to_string(),
                        }),
                    }
                }
                Err(error) => failures.push(ScheduledDataConfigFailure {
                    config_id: config_id.clone(),
                    reason: error.to_string(),
                }),
            }
            schedule.mark_ran(config_id, now_ms)?;
        }

        Ok(ResilientScheduledDataConfigPublishReport {
            data_configs_run: due_config_ids.len(),
            data_configs_succeeded,
            data_configs_failed: failures.len(),
            samples_collected,
            mqtt_messages_published,
            failures,
        })
    }

    pub fn reported_version(&self) -> &str {
        &self.package.version
    }

    pub fn shadow(&self, device_id: &str) -> Option<&DeviceShadow> {
        self.shadows.get(device_id)
    }

    async fn collect_samples_once(&mut self) -> Result<Vec<TelemetrySample>> {
        let mappings = self.package.point_mappings.clone();
        let samples = self.collect_mappings(mappings).await?;
        self.apply_algorithm_samples(samples)
    }

    async fn collect_samples_for_task(
        &mut self,
        task: &CollectionTask,
    ) -> Result<Vec<TelemetrySample>> {
        let mappings = self
            .package
            .point_mappings
            .iter()
            .filter(|mapping| {
                mapping.device_id == task.device_id && task.point_ids.contains(&mapping.point_id)
            })
            .cloned()
            .collect();
        let samples = self.collect_mappings(mappings).await?;
        self.apply_algorithm_samples(samples)
    }

    async fn collect_data_config_samples_once(&mut self) -> Result<Vec<TelemetrySample>> {
        let data_configs = self.package.data_configs.clone();
        let mut samples = Vec::new();

        for data_config in data_configs {
            if !data_config.enabled {
                continue;
            }
            samples.append(&mut self.collect_samples_for_data_config(&data_config).await?);
        }

        self.apply_algorithm_samples(samples)
    }

    async fn collect_data_config_samples(
        &mut self,
        config_id: &str,
    ) -> Result<Vec<TelemetrySample>> {
        let data_config = self
            .package
            .data_configs
            .iter()
            .find(|data_config| data_config.config_id == config_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("data config not found: {config_id}"))?;
        if !data_config.enabled {
            return Ok(Vec::new());
        }
        let samples = self.collect_samples_for_data_config(&data_config).await?;
        self.apply_algorithm_samples(samples)
    }

    async fn collect_samples_for_data_config(
        &mut self,
        data_config: &DataConfig,
    ) -> Result<Vec<TelemetrySample>> {
        let mappings = data_config
            .points
            .iter()
            .map(|point| {
                TelemetryPointMapping::new(
                    point.point_id.clone(),
                    data_config.device_id.clone(),
                    point.semantic_id.clone(),
                    data_config.protocol_connection_id.clone(),
                    point.address.clone(),
                    point.value_type,
                )
                .with_interval_ms(data_config.collection.period_ms)
            })
            .collect::<Vec<_>>();

        self.collect_mappings(mappings).await
    }

    async fn publish_data_config_samples<P>(
        &self,
        config_id: &str,
        samples: &[TelemetrySample],
        publisher: &mut P,
    ) -> Result<usize>
    where
        P: MqttPublisher + ?Sized,
    {
        let mut package = self.package.clone();
        package
            .data_configs
            .retain(|data_config| data_config.config_id == config_id);
        publish_data_config_mqtt_samples(&package, samples, publisher).await
    }

    async fn collect_mappings(
        &mut self,
        selected_mappings: Vec<TelemetryPointMapping>,
    ) -> Result<Vec<TelemetrySample>> {
        let mut samples = Vec::new();
        let connections = self.package.protocol_connections.clone();
        for connection in connections {
            let mappings = mappings_for_connection(&selected_mappings, &connection);
            if mappings.is_empty() {
                continue;
            }

            let mut connection_samples = match connection.protocol {
                ProtocolType::Simulated => collect_simulated_samples(&mappings),
                ProtocolType::ModbusRtu => {
                    let bus = self.serial_bus_factory.open(&connection)?;
                    let mut adapter = ModbusRtuAdapter::new(connection, mappings, bus);
                    adapter.read_telemetry().await?
                }
                unsupported => bail!("unsupported runtime protocol: {unsupported:?}"),
            };
            for sample in &connection_samples {
                if let Some(shadow) = self.shadows.get_mut(&sample.device_id) {
                    shadow.update(sample.clone());
                }
            }
            samples.append(&mut connection_samples);
        }

        Ok(samples)
    }

    fn apply_algorithm_samples(
        &mut self,
        mut samples: Vec<TelemetrySample>,
    ) -> Result<Vec<TelemetrySample>> {
        let mut algorithm_report = self.algorithm_engine.apply_samples(&samples)?;
        for sample in &algorithm_report.samples {
            if let Some(shadow) = self.shadows.get_mut(&sample.device_id) {
                shadow.update(sample.clone());
            }
        }
        samples.append(&mut algorithm_report.samples);
        Ok(samples)
    }
}

fn mappings_for_connection(
    mappings: &[TelemetryPointMapping],
    connection: &ProtocolConnection,
) -> Vec<TelemetryPointMapping> {
    mappings
        .iter()
        .filter(|mapping| mapping.protocol_connection_id == connection.connection_id)
        .cloned()
        .collect()
}

fn collect_simulated_samples(mappings: &[TelemetryPointMapping]) -> Vec<TelemetrySample> {
    mappings
        .iter()
        .map(|mapping| {
            TelemetrySample::new(
                &mapping.device_id,
                &mapping.point_id,
                TelemetryValue::Float(1.0),
                DataQuality::Good,
                chrono::Utc::now(),
            )
        })
        .collect()
}
