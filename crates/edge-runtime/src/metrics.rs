use chrono::Utc;
use edge_core::{
    AlgorithmRuntimeMetrics, CloudSyncMetrics, CollectionRuntimeMetrics, EdgeHealth,
    EdgeRuntimeMetricsSnapshot, LocalStoreMetrics, ProtocolRuntimeMetrics, ProtocolType,
    SystemRuntimeMetrics,
};

use crate::AppliedEdgeConfig;

pub struct SimulatedRuntimeMetricsCollector {
    runtime_id: String,
    applied: AppliedEdgeConfig,
}

impl SimulatedRuntimeMetricsCollector {
    pub fn new(runtime_id: impl Into<String>, applied: AppliedEdgeConfig) -> Self {
        Self {
            runtime_id: runtime_id.into(),
            applied,
        }
    }

    pub fn snapshot(&self) -> EdgeRuntimeMetricsSnapshot {
        let package = self.applied.package();

        EdgeRuntimeMetricsSnapshot {
            edge_id: package.edge_id.clone(),
            runtime_id: self.runtime_id.clone(),
            config_version: package.version.clone(),
            timestamp: Utc::now(),
            health: EdgeHealth::Healthy,
            system: SystemRuntimeMetrics {
                cpu_percent: 18.5,
                memory_percent: 42.0,
                disk_percent: 61.0,
                process_uptime_seconds: 3600,
            },
            collection: CollectionRuntimeMetrics {
                active_task_count: package
                    .collection_tasks
                    .iter()
                    .filter(|task| task.enabled)
                    .count(),
                success_rate: 0.995,
                average_latency_ms: 24,
                bad_point_count: 0,
            },
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
                backend: "jsonl".to_string(),
                buffered_records: 0,
                oldest_buffer_age_seconds: 0,
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
                pending_uploads: 0,
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
        ProtocolType::OpcUa => "OPC UA",
        ProtocolType::Mqtt => "MQTT",
        ProtocolType::SiemensS7 => "Siemens S7",
    }
    .to_string()
}
