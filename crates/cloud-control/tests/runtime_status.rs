use chrono::Utc;
use cloud_control::CloudControlStore;
use edge_core::{
    CloudSyncMetrics, CollectionRuntimeMetrics, EdgeHealth, EdgeRuntimeEvent,
    EdgeRuntimeMetricsSnapshot, LocalStoreMetrics, RuntimeEventCategory, RuntimeEventSeverity,
    SystemRuntimeMetrics,
};

fn snapshot(edge_id: &str, health: EdgeHealth) -> EdgeRuntimeMetricsSnapshot {
    EdgeRuntimeMetricsSnapshot {
        edge_id: edge_id.to_string(),
        runtime_id: "runtime-a".to_string(),
        config_version: "2026.06.26-002".to_string(),
        timestamp: Utc::now(),
        health,
        system: SystemRuntimeMetrics {
            cpu_percent: 18.5,
            memory_percent: 42.0,
            disk_percent: 61.0,
            process_uptime_seconds: 3600,
        },
        collection: CollectionRuntimeMetrics {
            active_task_count: 1,
            success_rate: 0.995,
            average_latency_ms: 24,
            bad_point_count: 0,
        },
        protocols: Vec::new(),
        local_store: LocalStoreMetrics {
            backend: "jsonl".to_string(),
            buffered_records: 0,
            oldest_buffer_age_seconds: 0,
            disk_usage_percent: 35.0,
        },
        algorithms: Vec::new(),
        mqtt: Default::default(),
        cloud_sync: CloudSyncMetrics {
            connected: true,
            last_sync_seconds_ago: 8,
            pending_uploads: 0,
            desired_version: "2026.06.26-002".to_string(),
            reported_version: "2026.06.26-002".to_string(),
        },
    }
}

#[test]
fn store_keeps_latest_runtime_snapshot_per_edge() {
    let mut store = CloudControlStore::default();

    store.upsert_runtime_metrics(snapshot("edge-dev", EdgeHealth::Healthy));
    store.upsert_runtime_metrics(snapshot("edge-dev", EdgeHealth::Degraded));

    let latest = store.runtime_metrics("edge-dev").unwrap();

    assert_eq!(latest.edge_id, "edge-dev");
    assert_eq!(latest.health, EdgeHealth::Degraded);
    assert_eq!(store.runtime_metrics_snapshots().count(), 1);
}

#[test]
fn store_appends_runtime_events() {
    let mut store = CloudControlStore::default();
    let event = EdgeRuntimeEvent::new(
        "edge-dev",
        RuntimeEventSeverity::Warning,
        RuntimeEventCategory::Protocol,
        "modbus.timeout",
        "Modbus TCP read timeout",
    );

    store.push_runtime_event(event);

    assert_eq!(store.runtime_events().len(), 1);
    assert_eq!(store.runtime_events()[0].code, "modbus.timeout");
}
