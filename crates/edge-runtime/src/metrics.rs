use chrono::Utc;
use edge_core::{
    AlgorithmRuntimeMetrics, CloudSyncMetrics, CollectionRuntimeMetrics, EdgeHealth,
    EdgeRuntimeMetricsSnapshot, LocalStoreMetrics, ProtocolRuntimeMetrics, ProtocolType,
    SystemRuntimeMetrics,
};

use crate::{AppliedEdgeConfig, MqttOutboxStats};

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

pub struct SimulatedRuntimeMetricsCollector {
    runtime_id: String,
    applied: AppliedEdgeConfig,
    collection_metrics: Option<CollectionRuntimeMetrics>,
    mqtt_outbox_stats: Option<MqttOutboxStats>,
}

impl SimulatedRuntimeMetricsCollector {
    pub fn new(runtime_id: impl Into<String>, applied: AppliedEdgeConfig) -> Self {
        Self {
            runtime_id: runtime_id.into(),
            applied,
            collection_metrics: None,
            mqtt_outbox_stats: None,
        }
    }

    pub fn with_collection_metrics(mut self, metrics: CollectionRuntimeMetrics) -> Self {
        self.collection_metrics = Some(metrics);
        self
    }

    pub fn with_mqtt_outbox_stats(mut self, stats: MqttOutboxStats) -> Self {
        self.mqtt_outbox_stats = Some(stats);
        self
    }

    pub fn snapshot(&self) -> EdgeRuntimeMetricsSnapshot {
        let package = self.applied.package();

        EdgeRuntimeMetricsSnapshot {
            edge_id: package.edge_id.clone(),
            runtime_id: self.runtime_id.clone(),
            config_version: package.version.clone(),
            timestamp: Utc::now(),
            health: if self
                .mqtt_outbox_stats
                .is_some_and(|stats| stats.pending_messages > 0)
            {
                EdgeHealth::Degraded
            } else {
                EdgeHealth::Healthy
            },
            system: SystemRuntimeMetrics {
                cpu_percent: 18.5,
                memory_percent: 42.0,
                disk_percent: 61.0,
                process_uptime_seconds: 3600,
            },
            collection: self
                .collection_metrics
                .clone()
                .unwrap_or(CollectionRuntimeMetrics {
                    active_task_count: package
                        .collection_tasks
                        .iter()
                        .filter(|task| task.enabled)
                        .count(),
                    success_rate: 0.995,
                    average_latency_ms: 24,
                    bad_point_count: 0,
                }),
            protocols: package
                .protocol_connections
                .iter()
                .map(|connection| ProtocolRuntimeMetrics {
                    connection_id: connection.connection_id.clone(),
                    protocol: format_protocol(connection.protocol),
                    connected: true,
                    latency_ms: 18,
                    timeout_count: 0,
                    error_count: 0,
                    reconnect_count: 0,
                })
                .collect(),
            local_store: LocalStoreMetrics {
                backend: if self.mqtt_outbox_stats.is_some() {
                    "rocksdb-mqtt-outbox".to_string()
                } else {
                    "jsonl".to_string()
                },
                buffered_records: self
                    .mqtt_outbox_stats
                    .map(|stats| stats.pending_messages)
                    .unwrap_or(0),
                oldest_buffer_age_seconds: self
                    .mqtt_outbox_stats
                    .map(|stats| stats.oldest_message_age_seconds)
                    .unwrap_or(0),
                disk_usage_percent: 35.0,
            },
            algorithms: package
                .algorithms
                .iter()
                .map(|algorithm| AlgorithmRuntimeMetrics {
                    algorithm_id: algorithm.id.clone(),
                    healthy: true,
                    last_run_latency_ms: 11,
                    error_count: 0,
                    alert_count: 0,
                })
                .collect(),
            cloud_sync: CloudSyncMetrics {
                connected: true,
                last_sync_seconds_ago: 8,
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

fn format_protocol(protocol: ProtocolType) -> String {
    match protocol {
        ProtocolType::Simulated => "Simulated",
        ProtocolType::ModbusTcp => "Modbus TCP",
        ProtocolType::ModbusRtu => "Modbus RTU",
        ProtocolType::Dlt645 => "DL/T645",
        ProtocolType::Iec101 => "IEC-101",
        ProtocolType::CustomSerial => "Custom Serial",
        ProtocolType::OpcUa => "OPC UA",
        ProtocolType::SiemensS7 => "Siemens S7",
    }
    .to_string()
}
