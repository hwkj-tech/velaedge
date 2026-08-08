use std::path::{Path, PathBuf};

use chrono::Utc;
use edge_core::{
    AlgorithmRuntimeMetrics, CloudSyncMetrics, CollectionRuntimeMetrics, EdgeHealth,
    EdgeRuntimeMetricsSnapshot, LocalStoreMetrics, ProtocolRuntimeMetrics, ProtocolType,
    SystemRuntimeMetrics,
};
use sysinfo::{get_current_pid, Disks, Pid, ProcessesToUpdate, System};

use crate::{AppliedEdgeConfig, MqttOutboxStats};

pub struct HostSystemMetricsSampler {
    system: System,
    disks: Disks,
    pid: Pid,
    storage_path: PathBuf,
}

impl HostSystemMetricsSampler {
    pub fn new(storage_path: impl AsRef<Path>) -> Self {
        let storage_path = absolute_path(storage_path.as_ref());
        Self {
            system: System::new_all(),
            disks: Disks::new_with_refreshed_list(),
            pid: get_current_pid().unwrap_or_else(|_| Pid::from_u32(std::process::id())),
            storage_path,
        }
    }

    pub fn sample(&mut self) -> SystemRuntimeMetrics {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.system
            .refresh_processes(ProcessesToUpdate::Some(&[self.pid]), false);
        self.disks.refresh();

        SystemRuntimeMetrics {
            cpu_percent: bounded_percent(f64::from(self.system.global_cpu_usage())),
            memory_percent: usage_percent(self.system.used_memory(), self.system.total_memory()),
            disk_percent: disk_usage_percent(&self.disks, &self.storage_path),
            process_uptime_seconds: self
                .system
                .process(self.pid)
                .map(|process| process.run_time())
                .unwrap_or_default(),
        }
    }
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    }
}

fn disk_usage_percent(disks: &Disks, path: &Path) -> f64 {
    let selected = disks
        .iter()
        .filter(|disk| path.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().components().count())
        .or_else(|| disks.iter().max_by_key(|disk| disk.total_space()));

    selected
        .map(|disk| {
            usage_percent(
                disk.total_space().saturating_sub(disk.available_space()),
                disk.total_space(),
            )
        })
        .unwrap_or_default()
}

fn usage_percent(used: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    bounded_percent(used as f64 * 100.0 / total as f64)
}

fn bounded_percent(value: f64) -> f64 {
    if value.is_finite() {
        (value.clamp(0.0, 100.0) * 10.0).round() / 10.0
    } else {
        0.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CollectionRunStats {
    active_task_count: usize,
    attempted_tasks: usize,
    succeeded_tasks: usize,
    failed_tasks: usize,
    total_latency_ms: u64,
    latency_samples: usize,
}

impl CollectionRunStats {
    pub fn new(active_task_count: usize) -> Self {
        Self {
            active_task_count,
            attempted_tasks: 0,
            succeeded_tasks: 0,
            failed_tasks: 0,
            total_latency_ms: 0,
            latency_samples: 0,
        }
    }

    pub fn record_tick(
        &mut self,
        tasks_run: usize,
        tasks_succeeded: usize,
        tasks_failed: usize,
        latency_ms: u64,
    ) {
        self.attempted_tasks += tasks_run;
        self.succeeded_tasks += tasks_succeeded;
        self.failed_tasks += tasks_failed;
        self.total_latency_ms = self.total_latency_ms.saturating_add(latency_ms);
        self.latency_samples += 1;
    }

    pub fn metrics(&self) -> CollectionRuntimeMetrics {
        CollectionRuntimeMetrics {
            active_task_count: self.active_task_count,
            success_rate: if self.attempted_tasks == 0 {
                1.0
            } else {
                self.succeeded_tasks as f64 / self.attempted_tasks as f64
            },
            average_latency_ms: if self.latency_samples == 0 {
                0
            } else {
                self.total_latency_ms / self.latency_samples as u64
            },
            bad_point_count: self.failed_tasks,
        }
    }
}

pub struct RuntimeMetricsCollector {
    runtime_id: String,
    applied: AppliedEdgeConfig,
    collection_metrics: Option<CollectionRuntimeMetrics>,
    protocol_metrics: Option<Vec<ProtocolRuntimeMetrics>>,
    algorithm_metrics: Option<Vec<AlgorithmRuntimeMetrics>>,
    mqtt_outbox_stats: Option<MqttOutboxStats>,
    system_metrics: Option<SystemRuntimeMetrics>,
}

impl RuntimeMetricsCollector {
    pub fn new(runtime_id: impl Into<String>, applied: AppliedEdgeConfig) -> Self {
        Self {
            runtime_id: runtime_id.into(),
            applied,
            collection_metrics: None,
            protocol_metrics: None,
            algorithm_metrics: None,
            mqtt_outbox_stats: None,
            system_metrics: None,
        }
    }

    pub fn with_collection_metrics(mut self, metrics: CollectionRuntimeMetrics) -> Self {
        self.collection_metrics = Some(metrics);
        self
    }

    pub fn with_protocol_metrics(mut self, metrics: Vec<ProtocolRuntimeMetrics>) -> Self {
        self.protocol_metrics = Some(metrics);
        self
    }

    pub fn with_algorithm_metrics(mut self, metrics: Vec<AlgorithmRuntimeMetrics>) -> Self {
        self.algorithm_metrics = Some(metrics);
        self
    }

    pub fn with_mqtt_outbox_stats(mut self, stats: MqttOutboxStats) -> Self {
        self.mqtt_outbox_stats = Some(stats);
        self
    }

    pub fn with_system_metrics(mut self, metrics: SystemRuntimeMetrics) -> Self {
        self.system_metrics = Some(metrics);
        self
    }

    pub fn snapshot(&self) -> EdgeRuntimeMetricsSnapshot {
        let package = self.applied.package();
        let collection = self
            .collection_metrics
            .clone()
            .unwrap_or(CollectionRuntimeMetrics {
                active_task_count: package
                    .collection_tasks
                    .iter()
                    .filter(|task| task.enabled)
                    .count(),
                success_rate: 0.0,
                average_latency_ms: 0,
                bad_point_count: 0,
            });
        let protocols = self.protocol_metrics.clone().unwrap_or_else(|| {
            package
                .protocol_connections
                .iter()
                .map(|connection| ProtocolRuntimeMetrics {
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
                    circuit_state: Default::default(),
                    consecutive_failure_count: 0,
                    circuit_open_count: 0,
                    circuit_rejected_count: 0,
                    last_quality_code: None,
                    good_value_count: 0,
                    uncertain_value_count: 0,
                    bad_value_count: 0,
                    subscription_count: 0,
                    notification_count: 0,
                    subscription_error_count: 0,
                    fallback_poll_count: 0,
                })
                .collect()
        });
        let algorithms = self.algorithm_metrics.clone().unwrap_or_default();
        let buffered_records = self
            .mqtt_outbox_stats
            .map(|stats| stats.pending_messages)
            .unwrap_or(0);

        EdgeRuntimeMetricsSnapshot {
            edge_id: package.edge_id.clone(),
            runtime_id: self.runtime_id.clone(),
            config_version: package.version.clone(),
            timestamp: Utc::now(),
            health: evaluate_runtime_health(
                &collection,
                &protocols,
                &algorithms,
                buffered_records,
                self.collection_metrics.is_some(),
            ),
            system: self
                .system_metrics
                .clone()
                .unwrap_or_else(|| HostSystemMetricsSampler::new(".").sample()),
            collection,
            protocols,
            local_store: LocalStoreMetrics {
                backend: if self.mqtt_outbox_stats.is_some() {
                    "rocksdb-mqtt-outbox".to_string()
                } else {
                    "jsonl".to_string()
                },
                buffered_records,
                oldest_buffer_age_seconds: self
                    .mqtt_outbox_stats
                    .map(|stats| stats.oldest_message_age_seconds)
                    .unwrap_or(0),
                disk_usage_percent: self
                    .system_metrics
                    .as_ref()
                    .map(|metrics| metrics.disk_percent)
                    .unwrap_or_default(),
            },
            algorithms,
            mqtt: Default::default(),
            cloud_sync: CloudSyncMetrics {
                connected: true,
                last_sync_seconds_ago: 0,
                pending_uploads: self
                    .mqtt_outbox_stats
                    .map(|stats| stats.pending_messages)
                    .unwrap_or(0),
                desired_version: package.version.clone(),
                reported_version: package.version.clone(),
            },
        }
    }
}

pub(crate) fn evaluate_runtime_health(
    collection: &CollectionRuntimeMetrics,
    protocols: &[ProtocolRuntimeMetrics],
    algorithms: &[AlgorithmRuntimeMetrics],
    buffered_records: u64,
    collection_observed: bool,
) -> EdgeHealth {
    if collection_observed
        && collection.active_task_count > 0
        && collection.success_rate <= f64::EPSILON
    {
        return EdgeHealth::Critical;
    }

    let collection_degraded =
        collection_observed && (collection.bad_point_count > 0 || collection.success_rate < 1.0);
    let protocol_degraded = protocols.iter().any(|protocol| {
        protocol.error_count > 0 || protocol.timeout_count > 0 || protocol.reconnect_count > 0
    });
    let algorithm_degraded = algorithms
        .iter()
        .any(|algorithm| !algorithm.healthy || algorithm.error_count > 0);

    if collection_degraded || protocol_degraded || algorithm_degraded || buffered_records > 0 {
        EdgeHealth::Degraded
    } else {
        EdgeHealth::Healthy
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
