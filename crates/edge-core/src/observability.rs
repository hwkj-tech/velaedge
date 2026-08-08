use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::DataQualityCode;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum EdgeHealth {
    Healthy,
    Degraded,
    Critical,
    Offline,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EdgeRuntimeMetricsSnapshot {
    pub edge_id: String,
    pub runtime_id: String,
    pub config_version: String,
    pub timestamp: DateTime<Utc>,
    pub health: EdgeHealth,
    pub system: SystemRuntimeMetrics,
    pub collection: CollectionRuntimeMetrics,
    pub protocols: Vec<ProtocolRuntimeMetrics>,
    pub local_store: LocalStoreMetrics,
    pub algorithms: Vec<AlgorithmRuntimeMetrics>,
    #[serde(default)]
    pub mqtt: MqttRuntimeMetrics,
    pub cloud_sync: CloudSyncMetrics,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SystemRuntimeMetrics {
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub disk_percent: f64,
    pub process_uptime_seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CollectionRuntimeMetrics {
    pub active_task_count: usize,
    pub success_rate: f64,
    pub average_latency_ms: u64,
    pub bad_point_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProtocolRuntimeMetrics {
    pub connection_id: String,
    pub protocol: String,
    pub connected: bool,
    pub latency_ms: u64,
    pub timeout_count: u64,
    pub error_count: u64,
    pub reconnect_count: u64,
    #[serde(default)]
    pub collection_attempt_count: u64,
    #[serde(default)]
    pub collection_success_count: u64,
    #[serde(default)]
    pub write_attempt_count: u64,
    #[serde(default)]
    pub write_success_count: u64,
    #[serde(default)]
    pub circuit_state: ProtocolCircuitState,
    #[serde(default)]
    pub consecutive_failure_count: u32,
    #[serde(default)]
    pub circuit_open_count: u64,
    #[serde(default)]
    pub circuit_rejected_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_quality_code: Option<DataQualityCode>,
    #[serde(default)]
    pub good_value_count: u64,
    #[serde(default)]
    pub uncertain_value_count: u64,
    #[serde(default)]
    pub bad_value_count: u64,
    #[serde(default)]
    pub subscription_count: usize,
    #[serde(default)]
    pub notification_count: u64,
    #[serde(default)]
    pub subscription_error_count: u64,
    #[serde(default)]
    pub fallback_poll_count: u64,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProtocolCircuitState {
    #[default]
    Closed,
    Open,
    HalfOpen,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LocalStoreMetrics {
    pub backend: String,
    pub buffered_records: u64,
    pub oldest_buffer_age_seconds: u64,
    pub disk_usage_percent: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AlgorithmRuntimeMetrics {
    pub algorithm_id: String,
    pub healthy: bool,
    pub last_run_latency_ms: u64,
    pub error_count: u64,
    pub alert_count: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MqttRuntimeMetrics {
    pub configured_sink_count: usize,
    pub connected_sink_count: usize,
    pub connection_generation: u64,
    pub publish_success_count: u64,
    pub publish_failure_count: u64,
    pub published_bytes: u64,
    pub sinks: Vec<MqttSinkRuntimeMetrics>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MqttSinkRuntimeMetrics {
    pub sink_id: String,
    pub broker: String,
    pub client_id: String,
    pub connected: bool,
    pub publish_success_count: u64,
    pub publish_failure_count: u64,
    pub published_bytes: u64,
    pub average_ack_latency_ms: u64,
    pub last_ack_latency_ms: Option<u64>,
    pub last_publish_at: Option<DateTime<Utc>>,
    pub last_topic: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudSyncMetrics {
    pub connected: bool,
    pub last_sync_seconds_ago: u64,
    pub pending_uploads: u64,
    pub desired_version: String,
    pub reported_version: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuntimeEventSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuntimeEventCategory {
    System,
    Protocol,
    Collection,
    Storage,
    Algorithm,
    Sync,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgeRuntimeEvent {
    pub edge_id: String,
    pub severity: RuntimeEventSeverity,
    pub category: RuntimeEventCategory,
    pub code: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub context: BTreeMap<String, String>,
}

impl EdgeRuntimeEvent {
    pub fn new(
        edge_id: impl Into<String>,
        severity: RuntimeEventSeverity,
        category: RuntimeEventCategory,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            edge_id: edge_id.into(),
            severity,
            category,
            code: code.into(),
            message: message.into(),
            timestamp: Utc::now(),
            context: BTreeMap::new(),
        }
    }

    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }
}
