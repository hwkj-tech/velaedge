use std::{
    collections::{BTreeMap, VecDeque},
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use edge_core::{
    AlgorithmRuntimeMetrics, CollectionTask, DataConfig, DataQuality, DataQualityCode,
    DeviceShadow, EdgeConfigPackage, EdgeRuntimeEvent, ProtocolCircuitState, ProtocolConnection,
    ProtocolRuntimeMetrics, ProtocolType, RuntimeEventCategory, RuntimeEventSeverity,
    TelemetryPointMapping, TelemetrySample, TelemetryType, TelemetryValue,
};

use crate::circuit_breaker::{
    is_circuit_open_error, CircuitBreakerSnapshot, ProtocolCircuitBreakerRegistry,
};
use crate::local_db::{CommandRateDecision, CommandRateLimit};
use crate::{
    build_command_reply_messages, command_values_match, config::validate_config_references,
    flush_mqtt_outbox, plan_command_execution, publish_data_config_mqtt_samples,
    publish_data_config_mqtt_samples_with_outbox, publish_mqtt_samples,
    publish_mqtt_samples_with_outbox, AlgorithmEngine, BacnetIpAdapter, CollectionReport,
    CommandClaim, CommandExecutionPlan, CommandExecutionReport, CommandWriteVerification,
    ConfiguredMqttCollectionReport, CustomSerialAdapter, Dlt645Adapter, Dlt645ReadFailure,
    Iec101Adapter, Iec104Adapter, ModbusRtuAdapter, ModbusTcpAdapter, MqttCommandMessage,
    MqttPublisher, OmronFinsAdapter, OpcUaAdapter, PlannedPointWrite, ProtocolAdapter,
    ProtocolCommandAdapter, ProtocolPointWrite, ProtocolWriteResult, RocksEdgeRuntimeStore,
    SerialBusFactory, SiemensS7Adapter,
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
    protocol_metrics: BTreeMap<String, ProtocolRuntimeMetrics>,
    iec104_adapters: BTreeMap<String, Iec104Adapter>,
    opc_ua_adapters: BTreeMap<String, OpcUaAdapter>,
    bacnet_ip_adapters: BTreeMap<String, BacnetIpAdapter>,
    siemens_s7_adapters: BTreeMap<String, SiemensS7Adapter>,
    omron_fins_adapters: BTreeMap<String, OmronFinsAdapter>,
    circuit_breakers: ProtocolCircuitBreakerRegistry,
    command_rate_windows: BTreeMap<(String, String), VecDeque<Instant>>,
}

impl<F> ConfiguredEdgeRuntime<F>
where
    F: SerialBusFactory,
{
    pub fn new(package: EdgeConfigPackage, serial_bus_factory: F) -> Result<Self> {
        Self::new_with_circuit_breakers(
            package,
            serial_bus_factory,
            ProtocolCircuitBreakerRegistry::default(),
        )
    }

    pub fn new_with_circuit_breakers(
        package: EdgeConfigPackage,
        serial_bus_factory: F,
        circuit_breakers: ProtocolCircuitBreakerRegistry,
    ) -> Result<Self> {
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
        circuit_breakers.configure(&package.protocol_connections);
        let protocol_metrics = package
            .protocol_connections
            .iter()
            .map(|connection| {
                let circuit = circuit_breakers.snapshot(&connection.connection_id);
                (
                    connection.connection_id.clone(),
                    ProtocolRuntimeMetrics {
                        connection_id: connection.connection_id.clone(),
                        protocol: format_protocol(connection.protocol),
                        connected: false,
                        latency_ms: 0,
                        timeout_count: 0,
                        error_count: 0,
                        reconnect_count: 0,
                        collection_attempt_count: 0,
                        collection_success_count: 0,
                        write_attempt_count: 0,
                        write_success_count: 0,
                        circuit_state: circuit.state,
                        consecutive_failure_count: circuit.consecutive_failures,
                        circuit_open_count: circuit.open_count,
                        circuit_rejected_count: circuit.rejected_count,
                        last_quality_code: None,
                        good_value_count: 0,
                        uncertain_value_count: 0,
                        bad_value_count: 0,
                        subscription_count: 0,
                        notification_count: 0,
                        subscription_error_count: 0,
                        fallback_poll_count: 0,
                    },
                )
            })
            .collect();

        Ok(Self {
            package,
            serial_bus_factory,
            shadows,
            algorithm_engine,
            protocol_metrics,
            iec104_adapters: BTreeMap::new(),
            opc_ua_adapters: BTreeMap::new(),
            bacnet_ip_adapters: BTreeMap::new(),
            siemens_s7_adapters: BTreeMap::new(),
            omron_fins_adapters: BTreeMap::new(),
            circuit_breakers,
            command_rate_windows: BTreeMap::new(),
        })
    }

    pub fn protocol_runtime_metrics(&self) -> Vec<ProtocolRuntimeMetrics> {
        self.protocol_metrics.values().cloned().collect()
    }

    pub fn algorithm_runtime_metrics(&self) -> Vec<AlgorithmRuntimeMetrics> {
        self.algorithm_engine.runtime_metrics()
    }

    fn allow_protocol_request(&mut self, connection_id: &str) -> Result<()> {
        let result = self
            .circuit_breakers
            .allow_request(connection_id, Instant::now());
        match result {
            Ok(snapshot) => {
                self.sync_circuit_metric(connection_id, snapshot);
                Ok(())
            }
            Err(error) => {
                let snapshot = self.circuit_breakers.snapshot(connection_id);
                self.sync_circuit_metric(connection_id, snapshot);
                let metric = self
                    .protocol_metrics
                    .get_mut(connection_id)
                    .expect("configured connection must have a metric entry");
                metric.connected = false;
                metric.last_quality_code = Some(DataQualityCode::BadOutOfService);
                Err(anyhow::Error::new(error))
            }
        }
    }

    fn record_protocol_success(&mut self, connection_id: &str) {
        let snapshot = self.circuit_breakers.record_success(connection_id);
        self.sync_circuit_metric(connection_id, snapshot);
    }

    fn record_protocol_failure(&mut self, connection_id: &str) {
        let snapshot = self
            .circuit_breakers
            .record_failure(connection_id, Instant::now());
        self.sync_circuit_metric(connection_id, snapshot);
    }

    fn sync_circuit_metric(&mut self, connection_id: &str, snapshot: CircuitBreakerSnapshot) {
        let metric = self
            .protocol_metrics
            .get_mut(connection_id)
            .expect("configured connection must have a metric entry");
        metric.circuit_state = snapshot.state;
        metric.consecutive_failure_count = snapshot.consecutive_failures;
        metric.circuit_open_count = snapshot.open_count;
        metric.circuit_rejected_count = snapshot.rejected_count;
    }

    fn record_sample_quality(&mut self, connection_id: &str, samples: &[TelemetrySample]) {
        let metric = self
            .protocol_metrics
            .get_mut(connection_id)
            .expect("configured connection must have a metric entry");
        for sample in samples {
            match sample.quality {
                DataQuality::Good => {
                    metric.good_value_count = metric.good_value_count.saturating_add(1)
                }
                DataQuality::Uncertain => {
                    metric.uncertain_value_count = metric.uncertain_value_count.saturating_add(1)
                }
                DataQuality::Bad => {
                    metric.bad_value_count = metric.bad_value_count.saturating_add(1)
                }
            }
        }
        metric.last_quality_code = samples
            .iter()
            .filter_map(|sample| sample.quality_code)
            .max_by_key(|quality_code| quality_severity(*quality_code));
    }

    fn record_bad_values(
        &mut self,
        connection_id: &str,
        quality_code: DataQualityCode,
        point_count: usize,
    ) {
        let metric = self
            .protocol_metrics
            .get_mut(connection_id)
            .expect("configured connection must have a metric entry");
        metric.last_quality_code = Some(quality_code);
        metric.bad_value_count = metric
            .bad_value_count
            .saturating_add(point_count.max(1) as u64);
    }

    pub async fn execute_command_flow_message(
        &mut self,
        flow_id: &str,
        payload: &[u8],
    ) -> Result<CommandExecutionReport> {
        let plan = plan_command_execution(&self.package, flow_id, payload)?;
        self.execute_command_plan(plan).await
    }

    async fn execute_command_plan(
        &mut self,
        plan: CommandExecutionPlan,
    ) -> Result<CommandExecutionReport> {
        if let Err(error) = self.enforce_command_safety(&plan) {
            return self.command_safety_failure_report(&plan, error);
        }
        self.execute_command_plan_after_safety(plan).await
    }

    async fn execute_command_plan_after_safety(
        &mut self,
        plan: CommandExecutionPlan,
    ) -> Result<CommandExecutionReport> {
        let package = self.package.clone();
        let mut report = CommandExecutionReport::new(&plan.flow.flow_id, &plan.command_id);
        report.source = plan.command_source.clone();

        let mut cursor = 0;
        while cursor < plan.writes.len() {
            let write = &plan.writes[cursor];
            let outcome = if self.is_modbus_response_write(write) {
                self.write_modbus_command_prefix(&plan.writes[cursor..])
                    .await
            } else {
                self.write_command_point(write)
                    .await
                    .map(|result| (1, vec![result]))
            };
            match outcome {
                Ok((consumed, results)) => {
                    if consumed == 0 || results.len() != consumed {
                        report.fail("protocol command adapter returned an invalid batch result");
                        break;
                    }
                    let mut failed_verification = false;
                    for (offset, result) in results.into_iter().enumerate() {
                        let write = &plan.writes[cursor + offset];
                        let verified = result.verified;
                        let readback_value = result.readback_value.clone();
                        report.record_write(write, result);
                        if !verified {
                            report.fail(format!(
                                "point {} readback verification failed: expected {:?}, actual {:?}",
                                write.mapping.point_id, write.value, readback_value
                            ));
                            failed_verification = true;
                            break;
                        }
                    }
                    if failed_verification {
                        break;
                    }
                    cursor += consumed;
                }
                Err(error) => {
                    report.fail(error.to_string());
                    break;
                }
            }
        }
        report.completed_at = chrono::Utc::now();
        report.replies = build_command_reply_messages(&package, &plan, &report)?;
        Ok(report)
    }

    fn command_safety_failure_report(
        &self,
        plan: &CommandExecutionPlan,
        error: anyhow::Error,
    ) -> Result<CommandExecutionReport> {
        let mut report = CommandExecutionReport::new(&plan.flow.flow_id, &plan.command_id);
        report.source = plan.command_source.clone();
        report.fail(error.to_string());
        report.replies = build_command_reply_messages(&self.package, plan, &report)?;
        Ok(report)
    }

    fn enforce_command_safety(&mut self, plan: &CommandExecutionPlan) -> Result<()> {
        self.enforce_command_sources(plan)?;

        let now = Instant::now();
        for gate in &plan.safety_gates {
            let (Some(max_commands), Some(window_ms)) = (gate.max_commands, gate.window_ms) else {
                continue;
            };
            let key = (plan.flow.flow_id.clone(), gate.node_id.clone());
            let window = Duration::from_millis(window_ms);
            let executions = self.command_rate_windows.entry(key).or_default();
            while executions
                .front()
                .is_some_and(|accepted_at| now.saturating_duration_since(*accepted_at) >= window)
            {
                executions.pop_front();
            }
            if executions.len() >= max_commands as usize {
                bail!(
                    "safety gate {} rate limit exceeded: {} commands per {}ms",
                    gate.node_id,
                    max_commands,
                    window_ms
                );
            }
        }

        for gate in &plan.safety_gates {
            if gate.max_commands.is_some() && gate.window_ms.is_some() {
                self.command_rate_windows
                    .entry((plan.flow.flow_id.clone(), gate.node_id.clone()))
                    .or_default()
                    .push_back(now);
            }
        }
        Ok(())
    }

    fn enforce_command_sources(&self, plan: &CommandExecutionPlan) -> Result<()> {
        for gate in &plan.safety_gates {
            if !gate.allowed_sources.is_empty() {
                let source = gate.source.as_deref().with_context(|| {
                    format!(
                        "safety gate {} requires command source at {}",
                        gate.node_id, gate.source_path
                    )
                })?;
                if !gate.allowed_sources.iter().any(|allowed| allowed == source) {
                    bail!(
                        "safety gate {} rejected command source {}",
                        gate.node_id,
                        source
                    );
                }
            }
        }
        Ok(())
    }

    fn enforce_persistent_command_safety(
        &self,
        plan: &CommandExecutionPlan,
        store: &RocksEdgeRuntimeStore,
    ) -> Result<()> {
        self.enforce_command_sources(plan)?;
        let limits = plan
            .safety_gates
            .iter()
            .filter_map(|gate| {
                Some(CommandRateLimit {
                    gate_id: gate.node_id.clone(),
                    max_commands: gate.max_commands?,
                    window_ms: gate.window_ms?,
                })
            })
            .collect::<Vec<_>>();
        match store.consume_command_rate_slots(
            &self.package.edge_id,
            &plan.flow.flow_id,
            &limits,
            Utc::now(),
        )? {
            CommandRateDecision::Accepted => Ok(()),
            CommandRateDecision::Rejected(limit) => bail!(
                "safety gate {} rate limit exceeded: {} commands per {}ms",
                limit.gate_id,
                limit.max_commands,
                limit.window_ms
            ),
        }
    }

    pub async fn execute_mqtt_command_message<P>(
        &mut self,
        message: &MqttCommandMessage,
        publisher: &mut P,
    ) -> Result<Vec<CommandExecutionReport>>
    where
        P: MqttPublisher + ?Sized,
    {
        let mut reports = Vec::new();
        for flow_id in &message.flow_ids {
            let flow = self
                .package
                .command_flows
                .iter()
                .find(|flow| flow.flow_id == *flow_id)
                .with_context(|| format!("command flow not found: {flow_id}"))?;
            if flow.mqtt_connection_id != message.sink_id {
                bail!(
                    "command flow {flow_id} belongs to MQTT connection {}, not {}",
                    flow.mqtt_connection_id,
                    message.sink_id
                );
            }
            let report = self
                .execute_command_flow_message(flow_id, &message.payload)
                .await?;
            for reply in report.replies.iter().cloned() {
                publisher
                    .publish(reply)
                    .await
                    .with_context(|| format!("publish command reply for flow {flow_id}"))?;
            }
            reports.push(report);
        }
        Ok(reports)
    }

    pub async fn execute_mqtt_command_message_with_store<P>(
        &mut self,
        message: &MqttCommandMessage,
        store: &RocksEdgeRuntimeStore,
        publisher: &mut P,
    ) -> Result<Vec<CommandExecutionReport>>
    where
        P: MqttPublisher + ?Sized,
    {
        let mut reports = Vec::new();
        for flow_id in &message.flow_ids {
            let flow = self
                .package
                .command_flows
                .iter()
                .find(|flow| flow.flow_id == *flow_id)
                .with_context(|| format!("command flow not found: {flow_id}"))?;
            if flow.mqtt_connection_id != message.sink_id {
                bail!(
                    "command flow {flow_id} belongs to MQTT connection {}, not {}",
                    flow.mqtt_connection_id,
                    message.sink_id
                );
            }

            let plan = plan_command_execution(&self.package, flow_id, &message.payload)?;
            let report = match store.claim_command(
                &self.package.edge_id,
                flow_id,
                &plan.command_id,
                &message.payload,
            )? {
                CommandClaim::Started(_) => {
                    let report = match self.enforce_persistent_command_safety(&plan, store) {
                        Ok(()) => self.execute_command_plan_after_safety(plan).await?,
                        Err(error) => self.command_safety_failure_report(&plan, error)?,
                    };
                    store.complete_command(&self.package.edge_id, &message.payload, &report)?;
                    report
                }
                CommandClaim::Duplicate(record) => record
                    .execution_report()
                    .context("completed command audit has no execution report")?,
                CommandClaim::InProgress(_) => {
                    bail!("command {} is already processing", plan.command_id)
                }
                CommandClaim::Conflict(_) => {
                    bail!(
                        "command {} was reused with a different payload",
                        plan.command_id
                    )
                }
            };
            for reply in report.replies.iter().cloned() {
                store.enqueue_mqtt_message(reply)?;
            }
            reports.push(report);
        }
        flush_mqtt_outbox(store, publisher).await?;
        Ok(reports)
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

    pub async fn collect_once_and_publish_mqtt_with_outbox<P>(
        &mut self,
        store: &RocksEdgeRuntimeStore,
        publisher: &mut P,
    ) -> Result<ConfiguredMqttCollectionReport>
    where
        P: MqttPublisher + ?Sized,
    {
        let samples = self.collect_samples_once().await?;
        let mqtt_messages_published =
            publish_mqtt_samples_with_outbox(&self.package, &samples, store, publisher).await?;
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
        let samples = self.collect_data_config_samples_once().await?;
        let mqtt_messages_published =
            publish_data_config_mqtt_samples_with_outbox(&self.package, &samples, store, publisher)
                .await?;
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

    pub async fn collect_due_data_configs_resilient_once_with_outbox<P>(
        &mut self,
        schedule: &mut DataConfigSchedule,
        now_ms: u64,
        store: &RocksEdgeRuntimeStore,
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
                        .publish_data_config_samples_with_outbox(
                            config_id, &samples, store, publisher,
                        )
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

    async fn write_command_point(
        &mut self,
        write: &PlannedPointWrite,
    ) -> Result<ProtocolWriteResult> {
        let mapping = &write.mapping;
        let value = write.value.clone();
        let connection = self
            .package
            .protocol_connections
            .iter()
            .find(|connection| connection.connection_id == mapping.protocol_connection_id)
            .cloned()
            .with_context(|| {
                format!(
                    "point {} protocol connection not found: {}",
                    mapping.point_id, mapping.protocol_connection_id
                )
            })?;
        let connection_id = connection.connection_id.clone();
        let metric = self
            .protocol_metrics
            .get_mut(&connection_id)
            .expect("configured connection must have a metric entry");
        metric.write_attempt_count = metric.write_attempt_count.saturating_add(1);
        self.allow_protocol_request(&connection_id)?;
        let started = Instant::now();
        let write_result = match connection.protocol {
            ProtocolType::Simulated => Ok(ProtocolWriteResult {
                point_id: mapping.point_id.clone(),
                value: value.clone(),
                verified: true,
                readback_value: matches!(write.verification, CommandWriteVerification::Readback)
                    .then(|| value.clone()),
            }),
            ProtocolType::ModbusTcp => {
                let mut adapter = ModbusTcpAdapter::new(connection, vec![mapping.clone()]);
                match adapter.write_point(mapping, value.clone()).await {
                    Ok(result) => verify_command_write(&mut adapter, write, result).await,
                    Err(error) => Err(error),
                }
            }
            ProtocolType::ModbusRtu => match self.serial_bus_factory.open(&connection) {
                Ok(bus) => {
                    let mut adapter = ModbusRtuAdapter::new(connection, vec![mapping.clone()], bus);
                    match adapter.write_point(mapping, value.clone()).await {
                        Ok(result) => verify_command_write(&mut adapter, write, result).await,
                        Err(error) => Err(error),
                    }
                }
                Err(error) => Err(error),
            },
            ProtocolType::SiemensS7 => {
                if !self.siemens_s7_adapters.contains_key(&connection_id) {
                    let adapter = SiemensS7Adapter::new(connection, vec![mapping.clone()])?;
                    self.siemens_s7_adapters
                        .insert(connection_id.clone(), adapter);
                }
                let adapter = self
                    .siemens_s7_adapters
                    .get_mut(&connection_id)
                    .expect("Siemens S7 adapter was inserted");
                adapter.set_mappings(vec![mapping.clone()])?;
                match adapter.write_point(mapping, value.clone()).await {
                    Ok(result) => verify_command_write(adapter, write, result).await,
                    Err(error) => Err(error),
                }
            }
            ProtocolType::OmronFins => {
                if !self.omron_fins_adapters.contains_key(&connection_id) {
                    let adapter = OmronFinsAdapter::new(connection, vec![mapping.clone()])?;
                    self.omron_fins_adapters
                        .insert(connection_id.clone(), adapter);
                }
                let adapter = self
                    .omron_fins_adapters
                    .get_mut(&connection_id)
                    .expect("Omron FINS adapter was inserted");
                adapter.set_mappings(vec![mapping.clone()])?;
                match adapter.write_point(mapping, value.clone()).await {
                    Ok(result) => verify_command_write(adapter, write, result).await,
                    Err(error) => Err(error),
                }
            }
            ProtocolType::Iec101 => match self.serial_bus_factory.open(&connection) {
                Ok(bus) => {
                    let mut adapter = Iec101Adapter::new(connection, vec![mapping.clone()], bus);
                    match adapter.write_point(mapping, value.clone()).await {
                        Ok(result) => verify_command_write(&mut adapter, write, result).await,
                        Err(error) => Err(error),
                    }
                }
                Err(error) => Err(error),
            },
            ProtocolType::Iec104 => {
                if !self.iec104_adapters.contains_key(&connection_id) {
                    let adapter = Iec104Adapter::new(connection, vec![mapping.clone()])?;
                    self.iec104_adapters.insert(connection_id.clone(), adapter);
                }
                let adapter = self
                    .iec104_adapters
                    .get_mut(&connection_id)
                    .expect("IEC 104 adapter was inserted");
                adapter.set_mappings(vec![mapping.clone()])?;
                match adapter.write_point(mapping, value.clone()).await {
                    Ok(result) => verify_command_write(adapter, write, result).await,
                    Err(error) => Err(error),
                }
            }
            ProtocolType::OpcUa => {
                if !self.opc_ua_adapters.contains_key(&connection_id) {
                    let adapter = OpcUaAdapter::new(connection, vec![mapping.clone()])?;
                    self.opc_ua_adapters.insert(connection_id.clone(), adapter);
                }
                let adapter = self
                    .opc_ua_adapters
                    .get_mut(&connection_id)
                    .expect("OPC UA adapter was inserted");
                adapter.set_mappings(vec![mapping.clone()])?;
                match adapter.write_point(mapping, value.clone()).await {
                    Ok(result) => verify_command_write(adapter, write, result).await,
                    Err(error) => Err(error),
                }
            }
            ProtocolType::BacnetIp => {
                if !self.bacnet_ip_adapters.contains_key(&connection_id) {
                    let adapter = BacnetIpAdapter::new(connection, vec![mapping.clone()])?;
                    self.bacnet_ip_adapters
                        .insert(connection_id.clone(), adapter);
                }
                let adapter = self
                    .bacnet_ip_adapters
                    .get_mut(&connection_id)
                    .expect("BACnet/IP adapter was inserted");
                adapter.set_mappings(vec![mapping.clone()])?;
                match adapter.write_point(mapping, value.clone()).await {
                    Ok(result) => verify_command_write(adapter, write, result).await,
                    Err(error) => Err(error),
                }
            }
            unsupported => Err(anyhow::anyhow!(
                "protocol {unsupported:?} does not support command writes"
            )),
        };
        self.protocol_metrics
            .get_mut(&connection_id)
            .expect("configured connection must have a metric entry")
            .latency_ms = elapsed_millis(started.elapsed());
        match write_result {
            Ok(result) => {
                self.record_protocol_success(&connection_id);
                let metric = self
                    .protocol_metrics
                    .get_mut(&connection_id)
                    .expect("configured connection must have a metric entry");
                metric.connected = true;
                metric.write_success_count = metric.write_success_count.saturating_add(1);
                update_command_shadow(&mut self.shadows, mapping, &result);
                Ok(result)
            }
            Err(error) => {
                self.record_protocol_failure(&connection_id);
                let metric = self
                    .protocol_metrics
                    .get_mut(&connection_id)
                    .expect("configured connection must have a metric entry");
                metric.connected = false;
                metric.error_count = metric.error_count.saturating_add(1);
                let message = error.to_string().to_ascii_lowercase();
                if message.contains("timeout") || message.contains("timed out") {
                    metric.timeout_count = metric.timeout_count.saturating_add(1);
                }
                Err(error)
            }
        }
    }

    fn is_modbus_response_write(&self, write: &PlannedPointWrite) -> bool {
        if write.verification != CommandWriteVerification::Response {
            return false;
        }
        self.package
            .protocol_connections
            .iter()
            .find(|connection| connection.connection_id == write.mapping.protocol_connection_id)
            .is_some_and(|connection| {
                matches!(
                    connection.protocol,
                    ProtocolType::ModbusTcp | ProtocolType::ModbusRtu
                )
            })
    }

    async fn write_modbus_command_prefix(
        &mut self,
        writes: &[PlannedPointWrite],
    ) -> Result<(usize, Vec<ProtocolWriteResult>)> {
        let first = writes
            .first()
            .context("command write batch cannot be empty")?;
        let connection = self
            .package
            .protocol_connections
            .iter()
            .find(|connection| connection.connection_id == first.mapping.protocol_connection_id)
            .cloned()
            .with_context(|| {
                format!(
                    "point {} protocol connection not found: {}",
                    first.mapping.point_id, first.mapping.protocol_connection_id
                )
            })?;
        let protocol_writes = writes
            .iter()
            .take_while(|write| {
                write.verification == CommandWriteVerification::Response
                    && write.mapping.protocol_connection_id == connection.connection_id
            })
            .map(|write| ProtocolPointWrite::new(write.mapping.clone(), write.value.clone()))
            .collect::<Vec<_>>();
        let connection_id = connection.connection_id.clone();
        let metric = self
            .protocol_metrics
            .get_mut(&connection_id)
            .expect("configured connection must have a metric entry");
        metric.write_attempt_count = metric.write_attempt_count.saturating_add(1);
        self.allow_protocol_request(&connection_id)?;
        let started = Instant::now();
        let write_result = match connection.protocol {
            ProtocolType::ModbusTcp => {
                let mut adapter = ModbusTcpAdapter::new(
                    connection,
                    protocol_writes
                        .iter()
                        .map(|write| write.mapping.clone())
                        .collect(),
                );
                let consumed = adapter.batchable_write_prefix(&protocol_writes);
                adapter
                    .write_points(&protocol_writes[..consumed])
                    .await
                    .map(|results| (consumed, results))
            }
            ProtocolType::ModbusRtu => match self.serial_bus_factory.open(&connection) {
                Ok(bus) => {
                    let mut adapter = ModbusRtuAdapter::new(
                        connection,
                        protocol_writes
                            .iter()
                            .map(|write| write.mapping.clone())
                            .collect(),
                        bus,
                    );
                    let consumed = adapter.batchable_write_prefix(&protocol_writes);
                    adapter
                        .write_points(&protocol_writes[..consumed])
                        .await
                        .map(|results| (consumed, results))
                }
                Err(error) => Err(error),
            },
            unsupported => Err(anyhow::anyhow!(
                "protocol {unsupported:?} does not support Modbus batch writes"
            )),
        };

        self.protocol_metrics
            .get_mut(&connection_id)
            .expect("configured connection must have a metric entry")
            .latency_ms = elapsed_millis(started.elapsed());
        match write_result {
            Ok((consumed, results)) => {
                self.record_protocol_success(&connection_id);
                let metric = self
                    .protocol_metrics
                    .get_mut(&connection_id)
                    .expect("configured connection must have a metric entry");
                metric.connected = true;
                metric.write_success_count = metric.write_success_count.saturating_add(1);
                for (write, result) in writes.iter().zip(&results).take(consumed) {
                    update_command_shadow(&mut self.shadows, &write.mapping, result);
                }
                Ok((consumed, results))
            }
            Err(error) => {
                self.record_protocol_failure(&connection_id);
                let metric = self
                    .protocol_metrics
                    .get_mut(&connection_id)
                    .expect("configured connection must have a metric entry");
                metric.connected = false;
                metric.error_count = metric.error_count.saturating_add(1);
                let message = error.to_string().to_ascii_lowercase();
                if message.contains("timeout") || message.contains("timed out") {
                    metric.timeout_count = metric.timeout_count.saturating_add(1);
                }
                Err(error)
            }
        }
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
            samples.append(
                &mut self
                    .collect_samples_for_data_config_with_recovery(&data_config)
                    .await?,
            );
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
        let samples = self
            .collect_samples_for_data_config_with_recovery(&data_config)
            .await?;
        self.apply_algorithm_samples(samples)
    }

    async fn collect_samples_for_data_config_with_recovery(
        &mut self,
        data_config: &DataConfig,
    ) -> Result<Vec<TelemetrySample>> {
        let attempts = data_config.collection.retry_count.saturating_add(1);
        let timeout_duration = Duration::from_millis(data_config.collection.timeout_ms);
        let mut last_error = None;

        for attempt in 0..attempts {
            let started = Instant::now();
            match tokio::time::timeout(
                timeout_duration,
                self.collect_samples_for_data_config(data_config),
            )
            .await
            {
                Ok(Ok(samples)) => return Ok(samples),
                Ok(Err(error)) if is_circuit_open_error(&error) => return Err(error),
                Ok(Err(error)) => last_error = Some(error),
                Err(_) => {
                    self.record_protocol_failure(&data_config.protocol_connection_id);
                    let metric = self
                        .protocol_metrics
                        .get_mut(&data_config.protocol_connection_id)
                        .expect("validated data config connection must have a metric entry");
                    metric.connected = false;
                    metric.latency_ms = elapsed_millis(started.elapsed());
                    metric.error_count = metric.error_count.saturating_add(1);
                    metric.timeout_count = metric.timeout_count.saturating_add(1);
                    metric.last_quality_code = Some(DataQualityCode::BadTimeout);
                    metric.bad_value_count = metric
                        .bad_value_count
                        .saturating_add(data_config.points.len() as u64);
                    last_error = Some(anyhow::anyhow!(
                        "data config {} collection timed out after {} ms",
                        data_config.config_id,
                        data_config.collection.timeout_ms
                    ));
                }
            }

            if attempt + 1 < attempts {
                if self
                    .protocol_metrics
                    .get(&data_config.protocol_connection_id)
                    .is_some_and(|metric| metric.circuit_state == ProtocolCircuitState::Open)
                {
                    break;
                }
                let metric = self
                    .protocol_metrics
                    .get_mut(&data_config.protocol_connection_id)
                    .expect("validated data config connection must have a metric entry");
                metric.reconnect_count = metric.reconnect_count.saturating_add(1);
                tokio::time::sleep(collection_retry_backoff(attempt)).await;
            }
        }

        Err(last_error.unwrap_or_else(|| {
            anyhow::anyhow!("data config {} collection failed", data_config.config_id)
        }))
    }

    async fn collect_samples_for_data_config(
        &mut self,
        data_config: &DataConfig,
    ) -> Result<Vec<TelemetrySample>> {
        let mappings = data_config
            .points
            .iter()
            .map(|point| {
                let mut mapping = TelemetryPointMapping::new(
                    point.point_id.clone(),
                    data_config.device_id.clone(),
                    point.semantic_id.clone(),
                    data_config.protocol_connection_id.clone(),
                    point.address.clone(),
                    point.value_type,
                )
                .with_interval_ms(data_config.collection.period_ms);
                if let Some(source) = self.package.point_mappings.iter().find(|source| {
                    source.point_id == point.point_id
                        && source.device_id == data_config.device_id
                        && source.protocol_connection_id == data_config.protocol_connection_id
                }) {
                    mapping.access = source.access;
                    mapping.unit = source.unit.clone();
                    mapping.range = source.range;
                }
                mapping
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

    async fn publish_data_config_samples_with_outbox<P>(
        &self,
        config_id: &str,
        samples: &[TelemetrySample],
        store: &RocksEdgeRuntimeStore,
        publisher: &mut P,
    ) -> Result<usize>
    where
        P: MqttPublisher + ?Sized,
    {
        let mut package = self.package.clone();
        package
            .data_configs
            .retain(|data_config| data_config.config_id == config_id);
        publish_data_config_mqtt_samples_with_outbox(&package, samples, store, publisher).await
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

            let connection_id = connection.connection_id.clone();
            let metric = self
                .protocol_metrics
                .get_mut(&connection_id)
                .expect("configured connection must have a metric entry");
            metric.collection_attempt_count = metric.collection_attempt_count.saturating_add(1);
            if let Err(error) = self.allow_protocol_request(&connection_id) {
                self.record_bad_values(
                    &connection_id,
                    DataQualityCode::BadOutOfService,
                    mappings.len(),
                );
                return Err(error);
            }
            let started = Instant::now();
            let mut partial_read_failures: Vec<Dlt645ReadFailure> = Vec::new();
            let read_result = match connection.protocol {
                ProtocolType::Simulated => Ok(collect_simulated_samples(&mappings)),
                ProtocolType::ModbusTcp => {
                    let mut adapter = ModbusTcpAdapter::new(connection, mappings.clone());
                    adapter.read_telemetry().await
                }
                ProtocolType::ModbusRtu => {
                    let bus = self.serial_bus_factory.open(&connection);
                    match bus {
                        Ok(bus) => {
                            let mut adapter =
                                ModbusRtuAdapter::new(connection, mappings.clone(), bus);
                            adapter.read_telemetry().await
                        }
                        Err(error) => Err(error),
                    }
                }
                ProtocolType::Dlt645 => {
                    let bus = self.serial_bus_factory.open(&connection);
                    match bus {
                        Ok(bus) => {
                            let mut adapter = Dlt645Adapter::new(connection, mappings.clone(), bus);
                            let result = adapter.read_telemetry().await;
                            if result.is_ok() {
                                partial_read_failures.extend_from_slice(adapter.read_failures());
                            }
                            result
                        }
                        Err(error) => Err(error),
                    }
                }
                ProtocolType::Iec101 => {
                    let bus = self.serial_bus_factory.open(&connection);
                    match bus {
                        Ok(bus) => {
                            let mut adapter = Iec101Adapter::new(connection, mappings.clone(), bus);
                            adapter.read_telemetry().await
                        }
                        Err(error) => Err(error),
                    }
                }
                ProtocolType::Iec104 => {
                    if !self.iec104_adapters.contains_key(&connection_id) {
                        let adapter = Iec104Adapter::new(connection, mappings.clone())?;
                        self.iec104_adapters.insert(connection_id.clone(), adapter);
                    }
                    let adapter = self
                        .iec104_adapters
                        .get_mut(&connection_id)
                        .expect("IEC 104 adapter was inserted");
                    adapter.set_mappings(mappings.clone())?;
                    adapter.read_telemetry().await
                }
                ProtocolType::CustomSerial => {
                    let bus = self.serial_bus_factory.open(&connection);
                    match bus {
                        Ok(bus) => {
                            let mut adapter =
                                CustomSerialAdapter::new(connection, mappings.clone(), bus);
                            adapter.read_telemetry().await
                        }
                        Err(error) => Err(error),
                    }
                }
                ProtocolType::OpcUa => {
                    if !self.opc_ua_adapters.contains_key(&connection_id) {
                        let adapter = OpcUaAdapter::new(connection, mappings.clone())?;
                        self.opc_ua_adapters.insert(connection_id.clone(), adapter);
                    }
                    let adapter = self
                        .opc_ua_adapters
                        .get_mut(&connection_id)
                        .expect("OPC UA adapter was inserted");
                    adapter.set_mappings(mappings.clone())?;
                    adapter.read_telemetry().await
                }
                ProtocolType::BacnetIp => {
                    if !self.bacnet_ip_adapters.contains_key(&connection_id) {
                        let adapter = BacnetIpAdapter::new(connection, mappings.clone())?;
                        self.bacnet_ip_adapters
                            .insert(connection_id.clone(), adapter);
                    }
                    let adapter = self
                        .bacnet_ip_adapters
                        .get_mut(&connection_id)
                        .expect("BACnet/IP adapter was inserted");
                    adapter.set_mappings(mappings.clone())?;
                    let result = adapter.read_telemetry().await;
                    let cov_metrics = adapter.cov_runtime_metrics();
                    if let Some(metrics) = self.protocol_metrics.get_mut(&connection_id) {
                        metrics.subscription_count = cov_metrics.active_subscriptions;
                        metrics.notification_count = cov_metrics.notifications_received;
                        metrics.subscription_error_count = cov_metrics.subscription_failures;
                        metrics.fallback_poll_count = cov_metrics.fallback_polls;
                    }
                    result
                }
                ProtocolType::SiemensS7 => {
                    if !self.siemens_s7_adapters.contains_key(&connection_id) {
                        let adapter = SiemensS7Adapter::new(connection, mappings.clone())?;
                        self.siemens_s7_adapters
                            .insert(connection_id.clone(), adapter);
                    }
                    let adapter = self
                        .siemens_s7_adapters
                        .get_mut(&connection_id)
                        .expect("Siemens S7 adapter was inserted");
                    adapter.set_mappings(mappings.clone())?;
                    adapter.read_telemetry().await
                }
                ProtocolType::OmronFins => {
                    if !self.omron_fins_adapters.contains_key(&connection_id) {
                        let adapter = OmronFinsAdapter::new(connection, mappings.clone())?;
                        self.omron_fins_adapters
                            .insert(connection_id.clone(), adapter);
                    }
                    let adapter = self
                        .omron_fins_adapters
                        .get_mut(&connection_id)
                        .expect("Omron FINS adapter was inserted");
                    adapter.set_mappings(mappings.clone())?;
                    adapter.read_telemetry().await
                }
            };
            let latency_ms = elapsed_millis(started.elapsed());
            self.protocol_metrics
                .get_mut(&connection_id)
                .expect("configured connection must have a metric entry")
                .latency_ms = latency_ms;
            let mut connection_samples = match read_result {
                Ok(mut samples) => {
                    apply_mapping_quality(&mappings, &mut samples);
                    self.record_protocol_success(&connection_id);
                    self.record_sample_quality(&connection_id, &samples);
                    for failure in &partial_read_failures {
                        self.record_bad_values(
                            &connection_id,
                            failure.quality_code,
                            failure.point_count,
                        );
                    }
                    let metric = self
                        .protocol_metrics
                        .get_mut(&connection_id)
                        .expect("configured connection must have a metric entry");
                    metric.connected = true;
                    metric.collection_success_count =
                        metric.collection_success_count.saturating_add(1);
                    metric.error_count = metric
                        .error_count
                        .saturating_add(partial_read_failures.len() as u64);
                    metric.timeout_count = metric.timeout_count.saturating_add(
                        partial_read_failures
                            .iter()
                            .filter(|failure| failure.quality_code == DataQualityCode::BadTimeout)
                            .count() as u64,
                    );
                    samples
                }
                Err(error) => {
                    self.record_protocol_failure(&connection_id);
                    let quality_code = classify_protocol_error(&error);
                    self.record_bad_values(&connection_id, quality_code, mappings.len());
                    let metric = self
                        .protocol_metrics
                        .get_mut(&connection_id)
                        .expect("configured connection must have a metric entry");
                    metric.connected = false;
                    metric.error_count = metric.error_count.saturating_add(1);
                    let message = error.to_string().to_ascii_lowercase();
                    if message.contains("timeout") || message.contains("timed out") {
                        metric.timeout_count = metric.timeout_count.saturating_add(1);
                    }
                    return Err(error);
                }
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

fn update_command_shadow(
    shadows: &mut BTreeMap<String, DeviceShadow>,
    mapping: &TelemetryPointMapping,
    result: &ProtocolWriteResult,
) {
    if let Some(shadow) = shadows.get_mut(&mapping.device_id) {
        shadow.update(TelemetrySample::new(
            &mapping.device_id,
            &mapping.point_id,
            result
                .readback_value
                .clone()
                .unwrap_or_else(|| result.value.clone()),
            DataQuality::Good,
            chrono::Utc::now(),
        ));
    }
}

fn collection_retry_backoff(attempt: u32) -> Duration {
    let multiplier = 1_u64 << attempt.min(4);
    Duration::from_millis((25 * multiplier).min(400))
}

fn apply_mapping_quality(mappings: &[TelemetryPointMapping], samples: &mut [TelemetrySample]) {
    for sample in samples {
        if sample.quality != DataQuality::Good {
            continue;
        }
        let Some(mapping) = mappings.iter().find(|mapping| {
            mapping.device_id == sample.device_id && mapping.point_id == sample.telemetry_id
        }) else {
            sample.quality = DataQuality::Bad;
            sample.quality_code = Some(DataQualityCode::BadConfiguration);
            continue;
        };
        if matches!(sample.value, TelemetryValue::Float(value) if !value.is_finite()) {
            sample.quality = DataQuality::Bad;
            sample.quality_code = Some(DataQualityCode::BadDecode);
            continue;
        }
        if let (Some(range), Some(value)) = (mapping.range, sample.value.as_f64()) {
            if !range.contains(value) {
                sample.quality = DataQuality::Uncertain;
                sample.quality_code = Some(DataQualityCode::UncertainOutOfRange);
            }
        }
    }
}

fn classify_protocol_error(error: &anyhow::Error) -> DataQualityCode {
    if is_circuit_open_error(error) {
        return DataQualityCode::BadOutOfService;
    }
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("timeout") || message.contains("timed out") {
        DataQualityCode::BadTimeout
    } else if message.contains("unsupported")
        || message.contains("configuration")
        || message.contains("missing")
        || message.contains("required")
    {
        DataQualityCode::BadConfiguration
    } else if message.contains("decode")
        || message.contains("encoding")
        || message.contains("non-finite")
    {
        DataQualityCode::BadDecode
    } else if message.contains("crc")
        || message.contains("checksum")
        || message.contains("exception")
        || message.contains("response")
        || message.contains("frame")
        || message.contains("function")
        || message.contains("protocol")
    {
        DataQualityCode::BadProtocol
    } else {
        DataQualityCode::BadCommunication
    }
}

const fn quality_severity(quality_code: DataQualityCode) -> u8 {
    match quality_code.quality() {
        DataQuality::Good => 0,
        DataQuality::Uncertain => 1,
        DataQuality::Bad => 2,
    }
}

async fn verify_command_write<A>(
    adapter: &mut A,
    write: &PlannedPointWrite,
    mut result: ProtocolWriteResult,
) -> Result<ProtocolWriteResult>
where
    A: ProtocolAdapter,
{
    if write.verification == CommandWriteVerification::Response {
        return Ok(result);
    }

    let samples = adapter
        .read_telemetry()
        .await
        .with_context(|| format!("read back point {} after write", write.mapping.point_id))?;
    let readback = samples
        .into_iter()
        .find(|sample| {
            sample.telemetry_id == write.mapping.point_id
                && sample.device_id == write.mapping.device_id
        })
        .with_context(|| {
            format!(
                "readback did not return point {} for device {}",
                write.mapping.point_id, write.mapping.device_id
            )
        })?
        .value;
    result.verified = command_values_match(&write.value, &readback, write.readback_tolerance);
    result.readback_value = Some(readback);
    Ok(result)
}

fn elapsed_millis(duration: std::time::Duration) -> u64 {
    if duration.is_zero() {
        0
    } else {
        duration.as_millis().max(1).min(u128::from(u64::MAX)) as u64
    }
}

fn format_protocol(protocol: ProtocolType) -> String {
    match protocol {
        ProtocolType::Simulated => "Simulated",
        ProtocolType::ModbusTcp => "Modbus TCP",
        ProtocolType::ModbusRtu => "Modbus RTU",
        ProtocolType::Dlt645 => "DL/T645",
        ProtocolType::Iec101 => "IEC-101",
        ProtocolType::Iec104 => "IEC-104",
        ProtocolType::CustomSerial => "Custom Serial",
        ProtocolType::OpcUa => "OPC UA",
        ProtocolType::BacnetIp => "BACnet/IP",
        ProtocolType::SiemensS7 => "Siemens S7",
        ProtocolType::OmronFins => "Omron FINS",
    }
    .to_string()
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
    let timestamp = chrono::Utc::now();
    mappings
        .iter()
        .map(|mapping| {
            TelemetrySample::new(
                &mapping.device_id,
                &mapping.point_id,
                simulated_value(mapping, timestamp),
                DataQuality::Good,
                timestamp,
            )
        })
        .collect()
}

fn simulated_value(
    mapping: &TelemetryPointMapping,
    timestamp: chrono::DateTime<chrono::Utc>,
) -> TelemetryValue {
    let seed = mapping.point_id.bytes().fold(0_u64, |value, byte| {
        value.wrapping_mul(31).wrapping_add(u64::from(byte))
    });
    let seconds = timestamp.timestamp_millis() as f64 / 1_000.0;
    let wave = (seconds / 8.0 + (seed % 360) as f64).sin();
    let semantic = mapping.semantic_id.to_ascii_lowercase();

    match mapping.value_type {
        TelemetryType::Boolean => {
            TelemetryValue::Boolean(((timestamp.timestamp() / 30) + seed as i64) % 2 == 0)
        }
        TelemetryType::Integer => {
            TelemetryValue::Integer((seed % 80) as i64 + timestamp.timestamp().rem_euclid(20))
        }
        TelemetryType::Text => TelemetryValue::Text("simulated-ok".to_string()),
        TelemetryType::Float => {
            let (base, amplitude) = if semantic.contains("pressure") {
                (2.40, 0.18)
            } else if semantic.contains("flow") {
                (128.0, 6.0)
            } else if semantic.contains("voltage") {
                (220.0, 2.5)
            } else if semantic.contains("current") {
                (8.4, 0.6)
            } else if semantic.contains("temperature") || semantic.contains("temp") {
                (36.5, 1.8)
            } else {
                ((seed % 100) as f64, 1.0)
            };
            let mut value = base + wave * amplitude;
            if let Some(range) = mapping.range {
                value = value.clamp(range.min, range.max);
            }
            TelemetryValue::Float((value * 1_000.0).round() / 1_000.0)
        }
    }
}

#[cfg(test)]
mod simulated_value_tests {
    use chrono::{TimeZone, Utc};
    use edge_core::{PointAddress, TelemetryPointMapping, TelemetryType, TelemetryValue};

    use super::simulated_value;

    fn mapping(
        point_id: &str,
        semantic_id: &str,
        value_type: TelemetryType,
    ) -> TelemetryPointMapping {
        TelemetryPointMapping::new(
            point_id,
            "device-1",
            semantic_id,
            "simulated-main",
            PointAddress::simulated(point_id),
            value_type,
        )
    }

    #[test]
    fn simulated_values_preserve_declared_types_and_use_realistic_pressure_range() {
        let timestamp = Utc.timestamp_opt(1_720_000_000, 0).single().unwrap();

        let pressure = simulated_value(
            &mapping("pressure", "pump.pressure", TelemetryType::Float),
            timestamp,
        );
        let running = simulated_value(
            &mapping("running", "pump.running", TelemetryType::Boolean),
            timestamp,
        );
        let status = simulated_value(
            &mapping("status", "pump.status", TelemetryType::Text),
            timestamp,
        );

        assert!(matches!(pressure, TelemetryValue::Float(value) if (2.22..=2.58).contains(&value)));
        assert!(matches!(running, TelemetryValue::Boolean(_)));
        assert_eq!(status, TelemetryValue::Text("simulated-ok".to_string()));
    }

    #[test]
    fn simulated_numeric_values_change_over_time() {
        let pressure = mapping("pressure", "pump.pressure", TelemetryType::Float);
        let first = Utc.timestamp_opt(1_720_000_000, 0).single().unwrap();
        let second = Utc.timestamp_opt(1_720_000_004, 0).single().unwrap();

        assert_ne!(
            simulated_value(&pressure, first),
            simulated_value(&pressure, second)
        );
    }
}
