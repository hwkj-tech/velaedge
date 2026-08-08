use chrono::Utc;
use edge_core::{
    AlgorithmRuntimeMetrics, CloudSyncMetrics, CollectionRuntimeMetrics, EdgeHealth,
    EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot, LocalStoreMetrics, MqttRuntimeMetrics,
    MqttSinkRuntimeMetrics, ProtocolRuntimeMetrics, RuntimeEventCategory, RuntimeEventSeverity,
    SystemRuntimeMetrics,
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
            collection_attempt_count: 120,
            collection_success_count: 119,
            write_attempt_count: 3,
            write_success_count: 3,
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
        mqtt: MqttRuntimeMetrics {
            configured_sink_count: 1,
            connected_sink_count: 1,
            connection_generation: 2,
            publish_success_count: 119,
            publish_failure_count: 1,
            published_bytes: 4096,
            sinks: vec![MqttSinkRuntimeMetrics {
                sink_id: "velamq-main".to_string(),
                broker: "mqtt://127.0.0.1:1883".to_string(),
                client_id: "runtime-a".to_string(),
                connected: true,
                publish_success_count: 119,
                publish_failure_count: 1,
                published_bytes: 4096,
                average_ack_latency_ms: 8,
                last_ack_latency_ms: Some(6),
                last_publish_at: Some(Utc::now()),
                last_topic: Some("factory/edge-dev/telemetry".to_string()),
                last_error: None,
            }],
        },
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
    assert_eq!(payload["protocols"][0]["collection_attempt_count"], 120);
    assert_eq!(payload["protocols"][0]["collection_success_count"], 119);
    assert_eq!(payload["protocols"][0]["write_attempt_count"], 3);
    assert_eq!(payload["protocols"][0]["write_success_count"], 3);
    assert_eq!(payload["mqtt"]["connected_sink_count"], 1);
    assert_eq!(payload["mqtt"]["publish_success_count"], 119);
    assert_eq!(payload["mqtt"]["sinks"][0]["sink_id"], "velamq-main");
    assert_eq!(payload["cloud_sync"]["reported_version"], "2026.06.26-002");
}

#[test]
fn mqtt_runtime_metrics_default_for_older_runtime_payloads() {
    let payload = serde_json::json!({
        "edge_id": "legacy-edge",
        "runtime_id": "legacy-runtime",
        "config_version": "v1",
        "timestamp": "2026-06-26T10:00:00Z",
        "health": "Healthy",
        "system": {
            "cpu_percent": 10.0,
            "memory_percent": 20.0,
            "disk_percent": 30.0,
            "process_uptime_seconds": 60
        },
        "collection": {
            "active_task_count": 1,
            "success_rate": 1.0,
            "average_latency_ms": 5,
            "bad_point_count": 0
        },
        "protocols": [],
        "local_store": {
            "backend": "rocksdb",
            "buffered_records": 0,
            "oldest_buffer_age_seconds": 0,
            "disk_usage_percent": 1.0
        },
        "algorithms": [],
        "cloud_sync": {
            "connected": true,
            "last_sync_seconds_ago": 0,
            "pending_uploads": 0,
            "desired_version": "v1",
            "reported_version": "v1"
        }
    });

    let snapshot: EdgeRuntimeMetricsSnapshot = serde_json::from_value(payload).unwrap();

    assert_eq!(snapshot.mqtt, MqttRuntimeMetrics::default());
}

#[test]
fn protocol_operation_counters_default_to_zero_for_older_runtime_payloads() {
    let metrics: ProtocolRuntimeMetrics = serde_json::from_value(serde_json::json!({
        "connection_id": "legacy-modbus",
        "protocol": "Modbus TCP",
        "connected": true,
        "latency_ms": 5,
        "timeout_count": 0,
        "error_count": 0,
        "reconnect_count": 0
    }))
    .unwrap();

    assert_eq!(metrics.collection_attempt_count, 0);
    assert_eq!(metrics.collection_success_count, 0);
    assert_eq!(metrics.write_attempt_count, 0);
    assert_eq!(metrics.write_success_count, 0);
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
