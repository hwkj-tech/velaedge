use chrono::Utc;
use edge_core::{
    AlgorithmRuntimeMetrics, CloudSyncMetrics, CollectionRuntimeMetrics, EdgeHealth,
    EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot, LocalStoreMetrics, ProtocolRuntimeMetrics,
    RuntimeEventCategory, RuntimeEventSeverity, SystemRuntimeMetrics,
};

#[test]
fn runtime_metrics_snapshot_preserves_edge_health_and_runtime_groups() {
    let snapshot = EdgeRuntimeMetricsSnapshot {
        edge_id: "edge-dev".to_string(),
        runtime_id: "runtime-a".to_string(),
        config_version: "2026.06.26-002".to_string(),
        timestamp: Utc::now(),
        health: EdgeHealth::Healthy,
        system: SystemRuntimeMetrics {
            cpu_percent: 18.5,
            memory_percent: 42.0,
            disk_percent: 61.0,
            process_uptime_seconds: 3600,
        },
        collection: CollectionRuntimeMetrics {
            active_task_count: 2,
            success_rate: 0.995,
            average_latency_ms: 24,
            bad_point_count: 1,
        },
        protocols: vec![ProtocolRuntimeMetrics {
            connection_id: "modbus-line-a".to_string(),
            protocol: "Modbus TCP".to_string(),
            connected: true,
            latency_ms: 18,
            timeout_count: 0,
            error_count: 0,
            reconnect_count: 1,
        }],
        local_store: LocalStoreMetrics {
            backend: "jsonl".to_string(),
            buffered_records: 12,
            oldest_buffer_age_seconds: 30,
            disk_usage_percent: 35.0,
        },
        algorithms: vec![AlgorithmRuntimeMetrics {
            algorithm_id: "pump-anomaly-v1".to_string(),
            healthy: true,
            last_run_latency_ms: 11,
            error_count: 0,
            alert_count: 2,
        }],
        cloud_sync: CloudSyncMetrics {
            connected: true,
            last_sync_seconds_ago: 8,
            pending_uploads: 0,
            desired_version: "2026.06.26-002".to_string(),
            reported_version: "2026.06.26-002".to_string(),
        },
    };

    let payload = serde_json::to_value(&snapshot).unwrap();

    assert_eq!(payload["edge_id"], "edge-dev");
    assert_eq!(payload["health"], "Healthy");
    assert_eq!(payload["system"]["cpu_percent"], 18.5);
    assert_eq!(payload["protocols"][0]["connection_id"], "modbus-line-a");
    assert_eq!(payload["cloud_sync"]["reported_version"], "2026.06.26-002");
}

#[test]
fn runtime_event_preserves_severity_category_and_context() {
    let event = EdgeRuntimeEvent::new(
        "edge-dev",
        RuntimeEventSeverity::Warning,
        RuntimeEventCategory::Protocol,
        "modbus.timeout",
        "Modbus TCP read timeout",
    )
    .with_context("connection_id", "modbus-line-a");

    let payload = serde_json::to_value(&event).unwrap();

    assert_eq!(payload["edge_id"], "edge-dev");
    assert_eq!(payload["severity"], "Warning");
    assert_eq!(payload["category"], "Protocol");
    assert_eq!(payload["code"], "modbus.timeout");
    assert_eq!(payload["context"]["connection_id"], "modbus-line-a");
}
