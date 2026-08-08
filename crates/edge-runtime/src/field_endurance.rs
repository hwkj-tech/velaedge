use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use edge_core::{
    EdgeConfigPackage, MqttUplinkConfig, ProtocolCircuitState, ProtocolRuntimeMetrics,
    ProtocolType, TelemetryValue,
};
use serde::Serialize;

use crate::{
    ConfiguredEdgeRuntime, DataConfigSchedule, MqttSinkRuntimeStatus, PersistentMqttPublisher,
    RecordingMqttPublisher, RocksEdgeRuntimeStore, TokioSerialBusFactory,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDeviceIdentity {
    pub site_id: String,
    pub operator: String,
    pub connection_id: String,
    pub manufacturer: String,
    pub model: String,
    pub serial_number: String,
}

#[derive(Clone, Debug)]
pub struct FieldEnduranceOptions {
    pub package: EdgeConfigPackage,
    pub package_sha256: Option<String>,
    pub duration: Duration,
    pub scheduler_interval: Duration,
    pub minimum_cycles: u64,
    pub maximum_failure_ratio: f64,
    pub maximum_progress_gap: Duration,
    pub require_recovery: bool,
    pub changing_points: BTreeSet<String>,
    pub exercise_mqtt: bool,
    pub physical_device_exercised: bool,
    pub physical_device: Option<FieldDeviceIdentity>,
    pub rocksdb_path: PathBuf,
}

impl FieldEnduranceOptions {
    pub fn laboratory(package: EdgeConfigPackage, rocksdb_path: impl Into<PathBuf>) -> Self {
        Self {
            package,
            package_sha256: None,
            duration: Duration::from_secs(10),
            scheduler_interval: Duration::from_millis(100),
            minimum_cycles: 5,
            maximum_failure_ratio: 0.05,
            maximum_progress_gap: Duration::from_secs(5),
            require_recovery: false,
            changing_points: BTreeSet::new(),
            exercise_mqtt: false,
            physical_device_exercised: false,
            physical_device: None,
            rocksdb_path: rocksdb_path.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldEnduranceStatus {
    Passed,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldEnduranceReport {
    pub schema_version: u32,
    pub status: FieldEnduranceStatus,
    pub mode: &'static str,
    pub physical_device_exercised: bool,
    pub physical_device: Option<FieldDeviceIdentity>,
    pub edge_id: String,
    pub config_version: String,
    pub package_sha256: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub configured_duration_ms: u64,
    pub observed_duration_ms: u64,
    pub scheduler_interval_ms: u64,
    pub cycles: FieldCycleEvidence,
    pub latency: FieldLatencyEvidence,
    pub points: BTreeMap<String, FieldPointEvidence>,
    pub protocols: Vec<ProtocolRuntimeMetrics>,
    pub protocol_acceptance: Vec<FieldProtocolAcceptanceEvidence>,
    pub mqtt: FieldMqttEvidence,
    pub criteria: FieldAcceptanceCriteria,
    pub recent_errors: Vec<String>,
    pub limitations: Vec<String>,
}

impl FieldEnduranceReport {
    pub fn passed(&self) -> bool {
        self.status == FieldEnduranceStatus::Passed
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldProtocolAcceptanceEvidence {
    pub connection_id: String,
    pub protocol: String,
    pub connected_at_finish: bool,
    pub circuit_state_at_finish: ProtocolCircuitState,
    pub collection_attempt_count: u64,
    pub collection_success_count: u64,
    pub collection_failure_count: u64,
    pub failure_ratio: f64,
    pub activity_observed: bool,
    pub failure_ratio_within_limit: bool,
    pub maximum_observed_success_gap_ms: u64,
    pub maximum_allowed_success_gap_ms: u64,
    pub counter_reset_observed: bool,
    pub continuous_activity: bool,
    pub passed: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldCycleEvidence {
    pub scheduler_ticks: u64,
    pub attempted: u64,
    pub succeeded: u64,
    pub failed: u64,
    pub samples_collected: u64,
    pub mqtt_messages_published: u64,
    pub recovered_after_failure: u64,
    pub failure_ratio: f64,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldLatencyEvidence {
    pub minimum_ms: u64,
    pub average_ms: u64,
    pub p95_ms: u64,
    pub maximum_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldPointEvidence {
    pub device_id: String,
    pub point_id: String,
    pub observations: u64,
    pub distinct_values: usize,
    pub first_value: Option<TelemetryValue>,
    pub last_value: Option<TelemetryValue>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldMqttEvidence {
    pub exercised: bool,
    pub configured_sink_count: usize,
    pub connected_sink_count: usize,
    pub connection_generation: u64,
    pub publish_success_count: u64,
    pub publish_failure_count: u64,
    pub published_bytes: u64,
    pub pending_outbox_messages: u64,
    pub oldest_outbox_message_age_seconds: u64,
    pub retained_acknowledgements: usize,
    pub sinks: Vec<MqttSinkRuntimeStatus>,
    pub sink_acceptance: Vec<FieldMqttSinkAcceptanceEvidence>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldMqttSinkAcceptanceEvidence {
    pub sink_id: String,
    pub publish_success_count: u64,
    pub maximum_observed_success_gap_ms: u64,
    pub maximum_allowed_success_gap_ms: u64,
    pub counter_reset_observed: bool,
    pub continuous_activity: bool,
    pub passed: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldAcceptanceCriteria {
    pub configured_duration_met: bool,
    pub minimum_cycles_met: bool,
    pub failure_ratio_within_limit: bool,
    pub all_configured_points_observed: bool,
    pub changing_points_observed: bool,
    pub protocols_connected_at_finish: bool,
    pub protocol_activity_observed: bool,
    pub protocols_individually_healthy: bool,
    pub mqtt_sinks_continuously_publishing: Option<bool>,
    pub recovery_observed: bool,
    pub mqtt_puback_complete: Option<bool>,
    pub mqtt_sinks_connected: Option<bool>,
    pub outbox_drained: Option<bool>,
    pub physical_identity_complete: Option<bool>,
    pub production_protocols_only: Option<bool>,
}

#[derive(Default)]
struct PointObservation {
    observations: u64,
    values: BTreeSet<String>,
    first_value: Option<TelemetryValue>,
    last_value: Option<TelemetryValue>,
}

#[derive(Default)]
struct ProgressGapTracker {
    previous_count: u64,
    first_progress_at_ms: Option<u64>,
    last_progress_at_ms: Option<u64>,
    maximum_progress_gap_ms: u64,
    counter_reset_observed: bool,
}

impl ProgressGapTracker {
    fn observe_counter(&mut self, count: u64, now_ms: u64) {
        if count < self.previous_count {
            self.counter_reset_observed = true;
        } else if count > self.previous_count {
            if let Some(previous) = self.last_progress_at_ms {
                self.maximum_progress_gap_ms = self
                    .maximum_progress_gap_ms
                    .max(now_ms.saturating_sub(previous));
            } else {
                self.first_progress_at_ms = Some(now_ms);
            }
            self.last_progress_at_ms = Some(now_ms);
        }
        self.previous_count = count;
    }

    fn maximum_observed_gap_ms(&self, finished_at_ms: u64) -> u64 {
        let leading_gap = self.first_progress_at_ms.unwrap_or(finished_at_ms);
        let trailing_gap = self
            .last_progress_at_ms
            .map_or(finished_at_ms, |last| finished_at_ms.saturating_sub(last));
        self.maximum_progress_gap_ms
            .max(leading_gap)
            .max(trailing_gap)
    }

    fn activity_observed(&self) -> bool {
        self.last_progress_at_ms.is_some()
    }

    fn current_gap_ms(&self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.last_progress_at_ms.unwrap_or_default())
    }
}

pub async fn run_field_endurance_acceptance(
    options: FieldEnduranceOptions,
) -> Result<FieldEnduranceReport> {
    let acceptance = validate_options(&options)?;
    let started_at = Utc::now();
    let started = Instant::now();
    let package = options.package.clone();
    let mut runtime = ConfiguredEdgeRuntime::new(package.clone(), TokioSerialBusFactory)?;
    let mut schedule = DataConfigSchedule::from_package(&package)?;
    let store = options
        .exercise_mqtt
        .then(|| RocksEdgeRuntimeStore::open(&options.rocksdb_path))
        .transpose()?;
    let mut mqtt_publisher = PersistentMqttPublisher::new();
    let mut recording_publisher = RecordingMqttPublisher::default();

    let mut scheduler_ticks = 0_u64;
    let mut attempted = 0_u64;
    let mut succeeded = 0_u64;
    let mut failed = 0_u64;
    let mut samples_collected = 0_u64;
    let mut mqtt_messages_published = 0_u64;
    let mut recovered_after_failure = 0_u64;
    let mut previous_cycle_failed = false;
    let mut latencies = Vec::new();
    let mut observations = BTreeMap::<String, PointObservation>::new();
    let mut recent_errors = Vec::new();
    let mut protocol_progress = acceptance
        .used_connection_ids
        .iter()
        .cloned()
        .map(|connection_id| (connection_id, ProgressGapTracker::default()))
        .collect::<BTreeMap<_, _>>();
    let mut mqtt_progress = acceptance
        .used_sink_ids
        .iter()
        .cloned()
        .map(|sink_id| (sink_id, ProgressGapTracker::default()))
        .collect::<BTreeMap<_, _>>();

    loop {
        scheduler_ticks = scheduler_ticks.saturating_add(1);
        let tick_started = Instant::now();
        let now_ms = duration_millis(started.elapsed());
        let tick = if options.exercise_mqtt {
            let publisher = mqtt_publisher
                .configure(&package.mqtt_uplinks)?
                .context("field endurance MQTT publisher is not configured")?;
            runtime
                .collect_due_data_configs_resilient_once_with_outbox(
                    &mut schedule,
                    now_ms,
                    store.as_ref().expect("MQTT store is configured"),
                    publisher,
                )
                .await?
        } else {
            runtime
                .collect_due_data_configs_resilient_once(
                    &mut schedule,
                    now_ms,
                    &mut recording_publisher,
                )
                .await?
        };

        if tick.data_configs_run > 0 {
            latencies.push(duration_millis(tick_started.elapsed()));
            attempted = attempted.saturating_add(tick.data_configs_run as u64);
            succeeded = succeeded.saturating_add(tick.data_configs_succeeded as u64);
            failed = failed.saturating_add(tick.data_configs_failed as u64);
            samples_collected = samples_collected.saturating_add(tick.samples_collected as u64);
            mqtt_messages_published =
                mqtt_messages_published.saturating_add(tick.mqtt_messages_published as u64);
            if previous_cycle_failed && tick.data_configs_succeeded > 0 {
                recovered_after_failure = recovered_after_failure.saturating_add(1);
            }
            previous_cycle_failed = tick.data_configs_failed > 0;
            for failure in tick.failures {
                recent_errors.push(format!("{}: {}", failure.config_id, failure.reason));
                if recent_errors.len() > 50 {
                    recent_errors.remove(0);
                }
            }
            observe_shadows(&runtime, &acceptance.expected_points, &mut observations)?;
        }

        let progress_at_ms = duration_millis(started.elapsed());
        for metrics in runtime.protocol_runtime_metrics() {
            if let Some(tracker) = protocol_progress.get_mut(&metrics.connection_id) {
                tracker.observe_counter(metrics.collection_success_count, progress_at_ms);
            }
        }
        if options.exercise_mqtt {
            for status in mqtt_publisher.status().sinks {
                if let Some(tracker) = mqtt_progress.get_mut(&status.sink_id) {
                    tracker.observe_counter(status.publish_success_count, progress_at_ms);
                }
            }
        }

        if started.elapsed() >= options.duration {
            break;
        }
        let stalled_sources = stalled_progress_sources(
            &protocol_progress,
            &mqtt_progress,
            options.exercise_mqtt,
            progress_at_ms,
            duration_millis(options.maximum_progress_gap),
        );
        if !stalled_sources.is_empty() {
            recent_errors.push(format!(
                "field endurance terminated early because successful progress stalled: {}",
                stalled_sources.join(", ")
            ));
            break;
        }
        tokio::time::sleep(options.scheduler_interval).await;
    }

    let observed_duration_ms = duration_millis(started.elapsed());
    let maximum_progress_gap_ms = duration_millis(options.maximum_progress_gap);
    let protocols = runtime.protocol_runtime_metrics();
    let protocol_acceptance = protocol_acceptance_evidence(
        &acceptance.used_connection_ids,
        &protocols,
        &protocol_progress,
        observed_duration_ms,
        maximum_progress_gap_ms,
        options.maximum_failure_ratio,
    );
    let protocols_individually_healthy = protocol_acceptance.iter().all(|evidence| evidence.passed);
    let protocol_by_id = protocols
        .iter()
        .map(|metrics| (metrics.connection_id.as_str(), metrics))
        .collect::<BTreeMap<_, _>>();
    let protocols_connected_at_finish =
        acceptance.used_connection_ids.iter().all(|connection_id| {
            protocol_by_id
                .get(connection_id.as_str())
                .is_some_and(|metrics| metrics.connected)
        });
    let protocol_activity_observed = acceptance.used_connection_ids.iter().all(|connection_id| {
        protocol_by_id
            .get(connection_id.as_str())
            .is_some_and(|metrics| {
                metrics.collection_attempt_count > 0
                    && metrics.collection_success_count > 0
                    && metrics.collection_success_count <= metrics.collection_attempt_count
            })
    });
    let failure_ratio = if attempted == 0 {
        1.0
    } else {
        failed as f64 / attempted as f64
    };
    let points = observations
        .into_iter()
        .map(|(key, observation)| {
            let (device_id, point_id) = split_point_key(&key);
            let device_id = device_id.to_string();
            let point_id = point_id.to_string();
            (
                key,
                FieldPointEvidence {
                    device_id,
                    point_id,
                    observations: observation.observations,
                    distinct_values: observation.values.len(),
                    first_value: observation.first_value,
                    last_value: observation.last_value,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let all_configured_points_observed = acceptance.expected_points.iter().all(|point| {
        points
            .get(point)
            .is_some_and(|evidence| evidence.observations > 0)
    });
    let changing_points_observed = options.changing_points.iter().all(|point| {
        points
            .get(point)
            .is_some_and(|evidence| evidence.distinct_values >= 2)
    });
    let mqtt = mqtt_evidence(
        &options,
        &mqtt_publisher,
        store.as_ref(),
        &acceptance.used_sink_ids,
        &mqtt_progress,
        observed_duration_ms,
        maximum_progress_gap_ms,
    )?;
    let used_mqtt_statuses = mqtt
        .sinks
        .iter()
        .filter(|status| acceptance.used_sink_ids.contains(&status.sink_id))
        .collect::<Vec<_>>();
    let mqtt_puback_complete = options.exercise_mqtt.then_some({
        mqtt.publish_success_count == mqtt_messages_published
            && mqtt.publish_failure_count == 0
            && mqtt_messages_published > 0
    });
    let mqtt_sinks_connected = options.exercise_mqtt.then(|| {
        used_mqtt_statuses.len() == acceptance.used_sink_ids.len()
            && used_mqtt_statuses.iter().all(|status| status.connected)
    });
    let mqtt_sinks_continuously_publishing = options
        .exercise_mqtt
        .then(|| mqtt.sink_acceptance.iter().all(|evidence| evidence.passed));
    let outbox_drained = options
        .exercise_mqtt
        .then_some(mqtt.pending_outbox_messages == 0);
    let physical_identity_complete = options
        .physical_device_exercised
        .then(|| physical_identity_complete(options.physical_device.as_ref()));
    let production_protocols_only = options
        .physical_device_exercised
        .then_some(acceptance.production_protocols_only);
    let criteria = FieldAcceptanceCriteria {
        configured_duration_met: observed_duration_ms >= duration_millis(options.duration),
        minimum_cycles_met: attempted >= options.minimum_cycles,
        failure_ratio_within_limit: failure_ratio <= options.maximum_failure_ratio,
        all_configured_points_observed,
        changing_points_observed,
        protocols_connected_at_finish,
        protocol_activity_observed,
        protocols_individually_healthy,
        mqtt_sinks_continuously_publishing,
        recovery_observed: !options.require_recovery || recovered_after_failure > 0,
        mqtt_puback_complete,
        mqtt_sinks_connected,
        outbox_drained,
        physical_identity_complete,
        production_protocols_only,
    };
    let passed = criteria.configured_duration_met
        && criteria.minimum_cycles_met
        && criteria.failure_ratio_within_limit
        && criteria.all_configured_points_observed
        && criteria.changing_points_observed
        && criteria.protocols_connected_at_finish
        && criteria.protocol_activity_observed
        && criteria.protocols_individually_healthy
        && criteria.mqtt_sinks_continuously_publishing.unwrap_or(true)
        && criteria.recovery_observed
        && criteria.mqtt_puback_complete.unwrap_or(true)
        && criteria.mqtt_sinks_connected.unwrap_or(true)
        && criteria.outbox_drained.unwrap_or(true)
        && criteria.physical_identity_complete.unwrap_or(true)
        && criteria.production_protocols_only.unwrap_or(true);
    let mut limitations = Vec::new();
    if !options.physical_device_exercised {
        limitations.push(
            "Laboratory evidence does not replace a 24-hour run against the target physical device."
                .to_string(),
        );
    }
    if !options.exercise_mqtt {
        limitations.push(
            "MQTT was recorded in process; no broker PUBACK or outbox-drain evidence was produced."
                .to_string(),
        );
    }

    Ok(FieldEnduranceReport {
        schema_version: 4,
        status: if passed {
            FieldEnduranceStatus::Passed
        } else {
            FieldEnduranceStatus::Failed
        },
        mode: if options.physical_device_exercised {
            "physical_field_endurance"
        } else {
            "laboratory_endurance"
        },
        physical_device_exercised: options.physical_device_exercised,
        physical_device: options.physical_device,
        edge_id: package.edge_id,
        config_version: package.version,
        package_sha256: options.package_sha256,
        started_at,
        finished_at: Utc::now(),
        configured_duration_ms: duration_millis(options.duration),
        observed_duration_ms,
        scheduler_interval_ms: duration_millis(options.scheduler_interval),
        cycles: FieldCycleEvidence {
            scheduler_ticks,
            attempted,
            succeeded,
            failed,
            samples_collected,
            mqtt_messages_published,
            recovered_after_failure,
            failure_ratio,
        },
        latency: latency_evidence(latencies),
        points,
        protocols,
        protocol_acceptance,
        mqtt,
        criteria,
        recent_errors,
        limitations,
    })
}

/// Validates a field endurance run without opening protocol, storage, or MQTT resources.
///
/// Deployment tooling uses this before starting a long-running physical campaign so malformed
/// identity, point, connection, MQTT, and acceptance-policy inputs fail before the evidence window
/// begins.
pub fn validate_field_endurance_options(options: &FieldEnduranceOptions) -> Result<()> {
    validate_options(options).map(|_| ())
}

fn stalled_progress_sources(
    protocol_progress: &BTreeMap<String, ProgressGapTracker>,
    mqtt_progress: &BTreeMap<String, ProgressGapTracker>,
    exercise_mqtt: bool,
    now_ms: u64,
    maximum_progress_gap_ms: u64,
) -> Vec<String> {
    let mut stalled = protocol_progress
        .iter()
        .filter(|(_, tracker)| tracker.current_gap_ms(now_ms) > maximum_progress_gap_ms)
        .map(|(connection_id, tracker)| {
            format!(
                "protocol connection {connection_id} ({} ms)",
                tracker.current_gap_ms(now_ms)
            )
        })
        .collect::<Vec<_>>();
    if exercise_mqtt {
        stalled.extend(
            mqtt_progress
                .iter()
                .filter(|(_, tracker)| tracker.current_gap_ms(now_ms) > maximum_progress_gap_ms)
                .map(|(sink_id, tracker)| {
                    format!(
                        "MQTT sink {sink_id} ({} ms)",
                        tracker.current_gap_ms(now_ms)
                    )
                }),
        );
    }
    stalled
}

struct ValidatedAcceptance {
    expected_points: BTreeSet<String>,
    used_connection_ids: BTreeSet<String>,
    used_sink_ids: BTreeSet<String>,
    production_protocols_only: bool,
}

fn protocol_acceptance_evidence(
    used_connection_ids: &BTreeSet<String>,
    protocols: &[ProtocolRuntimeMetrics],
    progress: &BTreeMap<String, ProgressGapTracker>,
    observed_duration_ms: u64,
    maximum_progress_gap_ms: u64,
    maximum_failure_ratio: f64,
) -> Vec<FieldProtocolAcceptanceEvidence> {
    let protocol_by_id = protocols
        .iter()
        .map(|metrics| (metrics.connection_id.as_str(), metrics))
        .collect::<BTreeMap<_, _>>();

    used_connection_ids
        .iter()
        .map(|connection_id| {
            let metrics = protocol_by_id.get(connection_id.as_str()).copied();
            let collection_attempt_count = metrics
                .map(|metrics| metrics.collection_attempt_count)
                .unwrap_or_default();
            let collection_success_count = metrics
                .map(|metrics| metrics.collection_success_count)
                .unwrap_or_default();
            let collection_failure_count =
                collection_attempt_count.saturating_sub(collection_success_count);
            let failure_ratio = if collection_attempt_count == 0 {
                1.0
            } else {
                collection_failure_count as f64 / collection_attempt_count as f64
            };
            let connected_at_finish = metrics.is_some_and(|metrics| metrics.connected);
            let circuit_state_at_finish = metrics
                .map(|metrics| metrics.circuit_state)
                .unwrap_or(ProtocolCircuitState::Open);
            let activity_observed = collection_attempt_count > 0
                && collection_success_count > 0
                && collection_success_count <= collection_attempt_count;
            let failure_ratio_within_limit =
                failure_ratio.is_finite() && failure_ratio <= maximum_failure_ratio;
            let tracker = progress.get(connection_id);
            let maximum_observed_success_gap_ms = tracker
                .map(|tracker| tracker.maximum_observed_gap_ms(observed_duration_ms))
                .unwrap_or(observed_duration_ms);
            let counter_reset_observed =
                tracker.is_none_or(|tracker| tracker.counter_reset_observed);
            let continuous_activity = tracker.is_some_and(|tracker| {
                tracker.activity_observed()
                    && !tracker.counter_reset_observed
                    && maximum_observed_success_gap_ms <= maximum_progress_gap_ms
            });
            let passed = connected_at_finish
                && circuit_state_at_finish == ProtocolCircuitState::Closed
                && activity_observed
                && failure_ratio_within_limit
                && continuous_activity;

            FieldProtocolAcceptanceEvidence {
                connection_id: connection_id.clone(),
                protocol: metrics
                    .map(|metrics| metrics.protocol.clone())
                    .unwrap_or_else(|| "unknown".to_string()),
                connected_at_finish,
                circuit_state_at_finish,
                collection_attempt_count,
                collection_success_count,
                collection_failure_count,
                failure_ratio,
                activity_observed,
                failure_ratio_within_limit,
                maximum_observed_success_gap_ms,
                maximum_allowed_success_gap_ms: maximum_progress_gap_ms,
                counter_reset_observed,
                continuous_activity,
                passed,
            }
        })
        .collect()
}

fn validate_options(options: &FieldEnduranceOptions) -> Result<ValidatedAcceptance> {
    if options.duration.is_zero() {
        bail!("field endurance duration must be greater than zero");
    }
    if options.scheduler_interval.is_zero() {
        bail!("field endurance scheduler interval must be greater than zero");
    }
    if options.minimum_cycles == 0 {
        bail!("field endurance minimum cycles must be greater than zero");
    }
    if !(0.0..=1.0).contains(&options.maximum_failure_ratio) {
        bail!("field endurance maximum failure ratio must be between 0 and 1");
    }
    if options.maximum_progress_gap.is_zero() {
        bail!("field endurance maximum progress gap must be greater than zero");
    }
    let enabled = options
        .package
        .data_configs
        .iter()
        .filter(|config| config.enabled)
        .collect::<Vec<_>>();
    if enabled.is_empty() {
        bail!("field endurance requires at least one enabled data configuration");
    }
    let expected_points = enabled
        .iter()
        .flat_map(|config| {
            config
                .points
                .iter()
                .map(|point| point_key(&config.device_id, &point.point_id))
        })
        .collect::<BTreeSet<_>>();
    if expected_points.is_empty() {
        bail!("field endurance requires at least one configured point");
    }
    let used_connection_ids = enabled
        .iter()
        .map(|config| config.protocol_connection_id.clone())
        .collect::<BTreeSet<_>>();
    let used_sink_ids = enabled
        .iter()
        .map(|config| config.publish.sink_id.clone())
        .collect::<BTreeSet<_>>();
    for point in &options.changing_points {
        if !expected_points.contains(point) {
            bail!("required changing point is not configured: {point}");
        }
    }
    let production_protocols_only = options
        .package
        .protocol_connections
        .iter()
        .filter(|connection| used_connection_ids.contains(&connection.connection_id))
        .all(|connection| connection.protocol != ProtocolType::Simulated);
    if options.physical_device_exercised {
        if !physical_identity_complete(options.physical_device.as_ref()) {
            bail!(
                "physical field endurance requires complete site, operator, connection and device identity"
            );
        }
        let connection_id = options
            .physical_device
            .as_ref()
            .expect("complete physical identity is present")
            .connection_id
            .trim();
        if !used_connection_ids.contains(connection_id) {
            bail!(
                "physical device connection {connection_id} is not used by an enabled data configuration"
            );
        }
        if !production_protocols_only {
            bail!("physical field endurance cannot use the simulated protocol");
        }
        if !options.exercise_mqtt {
            bail!("physical field endurance requires configured MQTT broker evidence");
        }
    }
    if options.exercise_mqtt {
        validate_mqtt(&options.package.mqtt_uplinks, &used_sink_ids)?;
    }
    Ok(ValidatedAcceptance {
        expected_points,
        used_connection_ids,
        used_sink_ids,
        production_protocols_only,
    })
}

fn validate_mqtt(uplinks: &[MqttUplinkConfig], used_sink_ids: &BTreeSet<String>) -> Result<()> {
    if uplinks.is_empty() {
        bail!("field endurance MQTT acceptance requires at least one uplink");
    }
    for sink_id in used_sink_ids {
        let uplink = uplinks
            .iter()
            .find(|uplink| &uplink.sink_id == sink_id)
            .with_context(|| {
                format!("data configuration references missing MQTT sink {sink_id}")
            })?;
        if uplink.qos != 1 {
            bail!("field endurance MQTT sink {sink_id} must use QoS 1");
        }
    }
    Ok(())
}

fn physical_identity_complete(identity: Option<&FieldDeviceIdentity>) -> bool {
    identity.is_some_and(|identity| {
        [
            identity.site_id.as_str(),
            identity.operator.as_str(),
            identity.connection_id.as_str(),
            identity.manufacturer.as_str(),
            identity.model.as_str(),
            identity.serial_number.as_str(),
        ]
        .iter()
        .all(|value| !value.trim().is_empty())
    })
}

fn observe_shadows(
    runtime: &ConfiguredEdgeRuntime<TokioSerialBusFactory>,
    expected_points: &BTreeSet<String>,
    observations: &mut BTreeMap<String, PointObservation>,
) -> Result<()> {
    for key in expected_points {
        let (device_id, point_id) = split_point_key(key);
        let Some(point) = runtime
            .shadow(device_id)
            .and_then(|shadow| shadow.telemetry().get(point_id))
        else {
            continue;
        };
        let entry = observations.entry(key.clone()).or_default();
        entry.observations = entry.observations.saturating_add(1);
        entry.values.insert(serde_json::to_string(&point.value)?);
        entry.first_value.get_or_insert_with(|| point.value.clone());
        entry.last_value = Some(point.value.clone());
    }
    Ok(())
}

fn mqtt_evidence(
    options: &FieldEnduranceOptions,
    publisher: &PersistentMqttPublisher,
    store: Option<&RocksEdgeRuntimeStore>,
    used_sink_ids: &BTreeSet<String>,
    progress: &BTreeMap<String, ProgressGapTracker>,
    observed_duration_ms: u64,
    maximum_progress_gap_ms: u64,
) -> Result<FieldMqttEvidence> {
    let status = publisher.status();
    let outbox = store
        .map(RocksEdgeRuntimeStore::mqtt_outbox_stats)
        .transpose()?
        .unwrap_or_default();
    let retained_acknowledgements = store
        .map(|store| store.mqtt_publish_acknowledgements(10_000))
        .transpose()?
        .map_or(0, |acknowledgements| acknowledgements.len());
    let sink_by_id = status
        .sinks
        .iter()
        .map(|sink| (sink.sink_id.as_str(), sink))
        .collect::<BTreeMap<_, _>>();
    let sink_acceptance = if options.exercise_mqtt {
        used_sink_ids
            .iter()
            .map(|sink_id| {
                let publish_success_count = sink_by_id
                    .get(sink_id.as_str())
                    .map(|sink| sink.publish_success_count)
                    .unwrap_or_default();
                let tracker = progress.get(sink_id);
                let maximum_observed_success_gap_ms = tracker
                    .map(|tracker| tracker.maximum_observed_gap_ms(observed_duration_ms))
                    .unwrap_or(observed_duration_ms);
                let counter_reset_observed =
                    tracker.is_none_or(|tracker| tracker.counter_reset_observed);
                let continuous_activity = tracker.is_some_and(|tracker| {
                    tracker.activity_observed()
                        && !tracker.counter_reset_observed
                        && maximum_observed_success_gap_ms <= maximum_progress_gap_ms
                });
                FieldMqttSinkAcceptanceEvidence {
                    sink_id: sink_id.clone(),
                    publish_success_count,
                    maximum_observed_success_gap_ms,
                    maximum_allowed_success_gap_ms: maximum_progress_gap_ms,
                    counter_reset_observed,
                    continuous_activity,
                    passed: publish_success_count > 0 && continuous_activity,
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(FieldMqttEvidence {
        exercised: options.exercise_mqtt,
        configured_sink_count: status.configured_sink_count,
        connected_sink_count: status.connected_sink_count,
        connection_generation: status.connection_generation,
        publish_success_count: status.publish_success_count,
        publish_failure_count: status.publish_failure_count,
        published_bytes: status.published_bytes,
        pending_outbox_messages: outbox.pending_messages,
        oldest_outbox_message_age_seconds: outbox.oldest_message_age_seconds,
        retained_acknowledgements,
        sinks: status.sinks,
        sink_acceptance,
    })
}

fn latency_evidence(mut latencies: Vec<u64>) -> FieldLatencyEvidence {
    if latencies.is_empty() {
        return FieldLatencyEvidence::default();
    }
    latencies.sort_unstable();
    let sum = latencies.iter().copied().sum::<u64>();
    let p95_index = ((latencies.len() * 95).div_ceil(100)).saturating_sub(1);
    FieldLatencyEvidence {
        minimum_ms: latencies[0],
        average_ms: sum / latencies.len() as u64,
        p95_ms: latencies[p95_index],
        maximum_ms: latencies[latencies.len() - 1],
    }
}

fn point_key(device_id: &str, point_id: &str) -> String {
    format!("{device_id}/{point_id}")
}

fn split_point_key(key: &str) -> (&str, &str) {
    key.split_once('/').unwrap_or(("", key))
}

fn duration_millis(duration: Duration) -> u64 {
    if duration.is_zero() {
        0
    } else {
        duration.as_millis().max(1).min(u128::from(u64::MAX)) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stalled_progress_reports_protocol_and_mqtt_sources_independently() {
        let mut protocol_tracker = ProgressGapTracker::default();
        protocol_tracker.observe_counter(1, 10);
        let protocol_progress = BTreeMap::from([("modbus-main".to_string(), protocol_tracker)]);
        let mqtt_progress =
            BTreeMap::from([("velamq-main".to_string(), ProgressGapTracker::default())]);

        assert!(
            stalled_progress_sources(&protocol_progress, &mqtt_progress, true, 100, 100).is_empty()
        );

        let stalled = stalled_progress_sources(&protocol_progress, &mqtt_progress, true, 111, 100);
        assert_eq!(stalled.len(), 2);
        assert!(stalled[0].contains("protocol connection modbus-main"));
        assert!(stalled[1].contains("MQTT sink velamq-main"));

        let protocol_only =
            stalled_progress_sources(&protocol_progress, &mqtt_progress, false, 111, 100);
        assert_eq!(protocol_only.len(), 1);
        assert!(protocol_only[0].contains("protocol connection modbus-main"));
    }
}
