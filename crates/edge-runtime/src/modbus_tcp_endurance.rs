use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use edge_core::{
    DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint, DataConfigPublish,
    DeviceInstance, EdgeConfigPackage, MqttUplinkConfig, PointAddress,
    ProtocolCircuitBreakerConfig, ProtocolConnection, TelemetryPointMapping, TelemetryType,
    TelemetryValue,
};
use serde::Serialize;

use crate::{
    ConfiguredEdgeRuntime, RecordingMqttPublisher, RocksEdgeRuntimeStore, RumqttcMqttPublisher,
    ScriptedSerialBusFactory,
};

const CONNECTION_ID: &str = "modbus-endurance";
const DEVICE_ID: &str = "pump-1";
const DATA_CONFIG_ID: &str = "pump-telemetry";
const RECORDING_SINK_ID: &str = "acceptance-recording";
const DYNAMIC_POINT_IDS: [&str; 2] = ["pressure", "flow"];

#[derive(Clone, Debug)]
pub struct ModbusTcpEnduranceOptions {
    pub endpoint: String,
    pub duration: Duration,
    pub interval: Duration,
    pub minimum_cycles: u64,
    pub maximum_failure_ratio: f64,
    pub require_dynamic_values: bool,
    pub require_recovery: bool,
    pub physical_device_exercised: bool,
    pub mqtt_uplink: Option<MqttUplinkConfig>,
    pub rocksdb_path: PathBuf,
}

impl ModbusTcpEnduranceOptions {
    pub fn laboratory(endpoint: impl Into<String>, rocksdb_path: impl Into<PathBuf>) -> Self {
        Self {
            endpoint: endpoint.into(),
            duration: Duration::from_secs(8),
            interval: Duration::from_millis(200),
            minimum_cycles: 8,
            maximum_failure_ratio: 0.4,
            require_dynamic_values: true,
            require_recovery: true,
            physical_device_exercised: false,
            mqtt_uplink: None,
            rocksdb_path: rocksdb_path.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModbusTcpEnduranceStatus {
    Passed,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModbusTcpEnduranceReport {
    pub status: ModbusTcpEnduranceStatus,
    pub mode: &'static str,
    pub physical_device_exercised: bool,
    pub endpoint: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub configured_duration_ms: u64,
    pub observed_duration_ms: u64,
    pub interval_ms: u64,
    pub cycles: ModbusTcpCycleEvidence,
    pub latency: ModbusTcpLatencyEvidence,
    pub points: BTreeMap<String, ModbusTcpPointEvidence>,
    pub protocol: ModbusTcpProtocolEvidence,
    pub mqtt: ModbusTcpMqttEvidence,
    pub criteria: ModbusTcpAcceptanceCriteria,
    pub recent_errors: Vec<String>,
    pub limitation: Option<String>,
}

impl ModbusTcpEnduranceReport {
    pub fn passed(&self) -> bool {
        self.status == ModbusTcpEnduranceStatus::Passed
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModbusTcpCycleEvidence {
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
pub struct ModbusTcpLatencyEvidence {
    pub minimum_ms: u64,
    pub average_ms: u64,
    pub p95_ms: u64,
    pub maximum_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModbusTcpPointEvidence {
    pub observations: u64,
    pub distinct_values: usize,
    pub first_value: Option<TelemetryValue>,
    pub last_value: Option<TelemetryValue>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModbusTcpProtocolEvidence {
    pub connected_at_finish: bool,
    pub latency_ms: u64,
    pub reconnect_count: u64,
    pub timeout_count: u64,
    pub error_count: u64,
    pub circuit_open_count: u64,
    pub circuit_rejected_count: u64,
    pub good_value_count: u64,
    pub uncertain_value_count: u64,
    pub bad_value_count: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModbusTcpMqttEvidence {
    pub broker_exercised: bool,
    pub sink_id: Option<String>,
    pub broker: Option<String>,
    pub qos: Option<u8>,
    pub connected_at_finish: Option<bool>,
    pub publish_success_count: u64,
    pub publish_failure_count: u64,
    pub published_bytes: u64,
    pub average_ack_latency_ms: Option<u64>,
    pub retained_acknowledgements: usize,
    pub pending_outbox_messages: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModbusTcpAcceptanceCriteria {
    pub minimum_cycles_met: bool,
    pub failure_ratio_within_limit: bool,
    pub dynamic_values_observed: bool,
    pub recovery_observed: bool,
    pub mqtt_puback_complete: Option<bool>,
    pub outbox_drained: Option<bool>,
}

#[derive(Default)]
struct PointObservation {
    observations: u64,
    values: BTreeSet<String>,
    first_value: Option<TelemetryValue>,
    last_value: Option<TelemetryValue>,
}

pub async fn run_modbus_tcp_endurance_acceptance(
    options: ModbusTcpEnduranceOptions,
) -> Result<ModbusTcpEnduranceReport> {
    validate_options(&options)?;

    let started_at = Utc::now();
    let started = Instant::now();
    let package = build_acceptance_package(&options);
    let mut runtime =
        ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(Vec::new()))?;
    let mut mqtt_publisher = options
        .mqtt_uplink
        .as_ref()
        .map(|uplink| {
            RumqttcMqttPublisher::connect_from_uplink_with_ack_timeout(
                uplink,
                Duration::from_secs(10),
            )
        })
        .transpose()?;
    let store = options
        .mqtt_uplink
        .as_ref()
        .map(|_| RocksEdgeRuntimeStore::open(&options.rocksdb_path))
        .transpose()?;
    let mut recording_publisher = RecordingMqttPublisher::default();

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

    loop {
        attempted = attempted.saturating_add(1);
        let cycle_started = Instant::now();
        let result = match (&store, &mut mqtt_publisher) {
            (Some(store), Some(publisher)) => {
                runtime
                    .collect_data_configs_once_and_publish_mqtt_with_outbox(store, publisher)
                    .await
            }
            _ => {
                runtime
                    .collect_data_configs_once_and_publish_mqtt(&mut recording_publisher)
                    .await
            }
        };
        latencies.push(duration_millis(cycle_started.elapsed()));

        match result {
            Ok(report) => {
                succeeded = succeeded.saturating_add(1);
                samples_collected =
                    samples_collected.saturating_add(report.collection.samples_collected as u64);
                mqtt_messages_published =
                    mqtt_messages_published.saturating_add(report.mqtt_messages_published as u64);
                if previous_cycle_failed {
                    recovered_after_failure = recovered_after_failure.saturating_add(1);
                }
                previous_cycle_failed = false;
                observe_shadow(&runtime, &mut observations)?;
            }
            Err(error) => {
                failed = failed.saturating_add(1);
                previous_cycle_failed = true;
                recent_errors.push(error.to_string());
                if recent_errors.len() > 20 {
                    recent_errors.remove(0);
                }
            }
        }

        if started.elapsed() >= options.duration && attempted >= options.minimum_cycles {
            break;
        }
        tokio::time::sleep(options.interval).await;
    }

    let failure_ratio = if attempted == 0 {
        1.0
    } else {
        failed as f64 / attempted as f64
    };
    let protocol = runtime
        .protocol_runtime_metrics()
        .into_iter()
        .find(|metric| metric.connection_id == CONNECTION_ID)
        .map(|metric| ModbusTcpProtocolEvidence {
            connected_at_finish: metric.connected,
            latency_ms: metric.latency_ms,
            reconnect_count: metric.reconnect_count,
            timeout_count: metric.timeout_count,
            error_count: metric.error_count,
            circuit_open_count: metric.circuit_open_count,
            circuit_rejected_count: metric.circuit_rejected_count,
            good_value_count: metric.good_value_count,
            uncertain_value_count: metric.uncertain_value_count,
            bad_value_count: metric.bad_value_count,
        })
        .unwrap_or_default();
    let mqtt = mqtt_evidence(&options, mqtt_publisher.as_ref(), store.as_ref())?;
    let points = observations
        .into_iter()
        .map(|(point_id, observation)| {
            (
                point_id,
                ModbusTcpPointEvidence {
                    observations: observation.observations,
                    distinct_values: observation.values.len(),
                    first_value: observation.first_value,
                    last_value: observation.last_value,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let dynamic_values_observed = !options.require_dynamic_values
        || DYNAMIC_POINT_IDS.iter().all(|point_id| {
            points
                .get(*point_id)
                .is_some_and(|evidence| evidence.distinct_values >= 2)
        });
    let mqtt_puback_complete = options.mqtt_uplink.as_ref().map(|_| {
        mqtt.publish_success_count == mqtt_messages_published
            && mqtt.publish_failure_count == 0
            && mqtt_messages_published > 0
    });
    let outbox_drained = options
        .mqtt_uplink
        .as_ref()
        .map(|_| mqtt.pending_outbox_messages == 0);
    let criteria = ModbusTcpAcceptanceCriteria {
        minimum_cycles_met: attempted >= options.minimum_cycles,
        failure_ratio_within_limit: failure_ratio <= options.maximum_failure_ratio,
        dynamic_values_observed,
        recovery_observed: !options.require_recovery || recovered_after_failure > 0,
        mqtt_puback_complete,
        outbox_drained,
    };
    let passed = criteria.minimum_cycles_met
        && criteria.failure_ratio_within_limit
        && criteria.dynamic_values_observed
        && criteria.recovery_observed
        && criteria.mqtt_puback_complete.unwrap_or(true)
        && criteria.outbox_drained.unwrap_or(true)
        && protocol.connected_at_finish;

    Ok(ModbusTcpEnduranceReport {
        status: if passed {
            ModbusTcpEnduranceStatus::Passed
        } else {
            ModbusTcpEnduranceStatus::Failed
        },
        mode: "modbus_tcp_endurance",
        physical_device_exercised: options.physical_device_exercised,
        endpoint: options.endpoint,
        started_at,
        finished_at: Utc::now(),
        configured_duration_ms: duration_millis(options.duration),
        observed_duration_ms: duration_millis(started.elapsed()),
        interval_ms: duration_millis(options.interval),
        cycles: ModbusTcpCycleEvidence {
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
        protocol,
        mqtt,
        criteria,
        recent_errors,
        limitation: (!options.physical_device_exercised).then(|| {
            "Laboratory evidence does not replace a 24-hour run against the target physical device."
                .to_string()
        }),
    })
}

fn validate_options(options: &ModbusTcpEnduranceOptions) -> Result<()> {
    if options.endpoint.trim().is_empty() {
        bail!("Modbus TCP endurance endpoint is required");
    }
    if options.duration.is_zero() {
        bail!("Modbus TCP endurance duration must be greater than zero");
    }
    if options.interval.is_zero() {
        bail!("Modbus TCP endurance interval must be greater than zero");
    }
    if options.minimum_cycles == 0 {
        bail!("Modbus TCP endurance minimum cycles must be greater than zero");
    }
    if !(0.0..=1.0).contains(&options.maximum_failure_ratio) {
        bail!("Modbus TCP endurance maximum failure ratio must be between 0 and 1");
    }
    if let Some(uplink) = &options.mqtt_uplink {
        if uplink.qos != 1 {
            bail!("Modbus TCP endurance MQTT acceptance requires QoS 1");
        }
    }
    Ok(())
}

fn build_acceptance_package(options: &ModbusTcpEnduranceOptions) -> EdgeConfigPackage {
    let mqtt_uplink = options.mqtt_uplink.clone().unwrap_or_else(|| {
        MqttUplinkConfig::velamq(
            RECORDING_SINK_ID,
            "mqtt://127.0.0.1:1883",
            "modbus-endurance-recording",
        )
        .with_qos(1)
    });
    let sink_id = mqtt_uplink.sink_id.clone();
    let points = acceptance_points();
    let connection = ProtocolConnection::modbus_tcp(CONNECTION_ID, &options.endpoint)
        .with_circuit_breaker(ProtocolCircuitBreakerConfig {
            enabled: true,
            failure_threshold: 2,
            open_duration_ms: 500,
            half_open_success_threshold: 1,
        });
    let mut data_config = DataConfig::new(
        DATA_CONFIG_ID,
        "Modbus endurance telemetry",
        DEVICE_ID,
        CONNECTION_ID,
        DataConfigCollection::new(duration_millis(options.interval))
            .with_timeout_ms(800)
            .with_retry_count(2),
        DataConfigPublish::new(
            sink_id,
            "acceptance/{edge_id}/{device_id}/telemetry",
            DataConfigPayload::object(),
        )
        .with_qos(1),
    );
    let mut package = EdgeConfigPackage::new("modbus-endurance-edge", "acceptance-v1")
        .with_device(DeviceInstance::new(DEVICE_ID, "pump"))
        .with_protocol_connection(connection)
        .with_mqtt_uplink(mqtt_uplink);

    for (point_id, semantic_id, address, value_type, json_field) in points {
        data_config = data_config.with_point(DataConfigPoint::new(
            point_id,
            semantic_id,
            address.clone(),
            value_type,
            json_field,
        ));
        package = package.with_point_mapping(TelemetryPointMapping::new(
            point_id,
            DEVICE_ID,
            semantic_id,
            CONNECTION_ID,
            address,
            value_type,
        ));
    }
    package.with_data_config(data_config)
}

fn acceptance_points() -> Vec<(
    &'static str,
    &'static str,
    PointAddress,
    TelemetryType,
    &'static str,
)> {
    vec![
        (
            "pressure",
            "pump.pressure",
            PointAddress::modbus_holding_register(40011),
            TelemetryType::Float,
            "pressure",
        ),
        (
            "flow",
            "pump.flow",
            PointAddress::modbus_holding_register(40013),
            TelemetryType::Float,
            "flow",
        ),
        (
            "running",
            "pump.running",
            modbus_address("coil", "00001"),
            TelemetryType::Boolean,
            "running",
        ),
        (
            "alarm",
            "pump.alarm",
            modbus_address("coil", "00007"),
            TelemetryType::Boolean,
            "alarm",
        ),
        (
            "temperature",
            "pump.temperature",
            modbus_address("input_register", "30001"),
            TelemetryType::Integer,
            "temperature",
        ),
    ]
}

fn modbus_address(kind: &str, value: &str) -> PointAddress {
    PointAddress {
        kind: kind.to_string(),
        value: value.to_string(),
        modbus: None,
    }
}

fn observe_shadow(
    runtime: &ConfiguredEdgeRuntime<ScriptedSerialBusFactory>,
    observations: &mut BTreeMap<String, PointObservation>,
) -> Result<()> {
    let shadow = runtime
        .shadow(DEVICE_ID)
        .context("Modbus endurance device shadow is missing")?;
    for (point_id, point) in shadow.telemetry() {
        let entry = observations.entry(point_id.clone()).or_default();
        entry.observations = entry.observations.saturating_add(1);
        entry.values.insert(serde_json::to_string(&point.value)?);
        entry.first_value.get_or_insert_with(|| point.value.clone());
        entry.last_value = Some(point.value.clone());
    }
    Ok(())
}

fn mqtt_evidence(
    options: &ModbusTcpEnduranceOptions,
    publisher: Option<&RumqttcMqttPublisher>,
    store: Option<&RocksEdgeRuntimeStore>,
) -> Result<ModbusTcpMqttEvidence> {
    let status = publisher.map(RumqttcMqttPublisher::runtime_status);
    let outbox = store
        .map(RocksEdgeRuntimeStore::mqtt_outbox_stats)
        .transpose()?
        .unwrap_or_default();
    let retained_acknowledgements = store
        .map(|store| store.mqtt_publish_acknowledgements(1_000))
        .transpose()?
        .map_or(0, |acknowledgements| acknowledgements.len());
    let uplink = options.mqtt_uplink.as_ref();

    Ok(ModbusTcpMqttEvidence {
        broker_exercised: uplink.is_some(),
        sink_id: uplink.map(|uplink| uplink.sink_id.clone()),
        broker: uplink.map(|uplink| uplink.broker.clone()),
        qos: uplink.map(|uplink| uplink.qos),
        connected_at_finish: status.as_ref().map(|status| status.connected),
        publish_success_count: status
            .as_ref()
            .map_or(0, |status| status.publish_success_count),
        publish_failure_count: status
            .as_ref()
            .map_or(0, |status| status.publish_failure_count),
        published_bytes: status.as_ref().map_or(0, |status| status.published_bytes),
        average_ack_latency_ms: status.as_ref().map(|status| status.average_ack_latency_ms),
        retained_acknowledgements,
        pending_outbox_messages: outbox.pending_messages,
    })
}

fn latency_evidence(mut latencies: Vec<u64>) -> ModbusTcpLatencyEvidence {
    if latencies.is_empty() {
        return ModbusTcpLatencyEvidence::default();
    }
    latencies.sort_unstable();
    let sum = latencies.iter().copied().sum::<u64>();
    let p95_index = ((latencies.len() * 95).div_ceil(100)).saturating_sub(1);
    ModbusTcpLatencyEvidence {
        minimum_ms: latencies[0],
        average_ms: sum / latencies.len() as u64,
        p95_ms: latencies[p95_index],
        maximum_ms: latencies[latencies.len() - 1],
    }
}

fn duration_millis(duration: Duration) -> u64 {
    duration.as_millis().max(1).min(u128::from(u64::MAX)) as u64
}
