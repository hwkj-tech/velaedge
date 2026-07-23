use edge_core::{
    CollectionRuntimeMetrics, CollectionTask, DeviceInstance, EdgeConfigPackage, EdgeHealth,
    PointAddress, ProtocolConnection, TelemetryPointMapping, TelemetryType,
};
use edge_runtime::{
    AppliedEdgeConfig, HostSystemMetricsSampler, MqttOutboxStats, RuntimeMetricsCollector,
};

fn package() -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", "2026.06.26-002")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_point_mapping(TelemetryPointMapping::new(
            "pressure",
            "pump-1",
            "pressure",
            "sim-main",
            PointAddress::simulated("pressure"),
            TelemetryType::Float,
        ))
        .with_collection_task(CollectionTask::interval(
            "pump-main",
            "pump-1",
            vec!["pressure".to_string()],
            1000,
        ))
}

#[test]
fn simulated_metrics_collector_reports_runtime_health_from_applied_config() {
    let applied = AppliedEdgeConfig::apply(package()).unwrap();
    let collector = RuntimeMetricsCollector::new("runtime-a", applied);

    let snapshot = collector.snapshot();

    assert_eq!(snapshot.edge_id, "edge-dev");
    assert_eq!(snapshot.runtime_id, "runtime-a");
    assert_eq!(snapshot.config_version, "2026.06.26-002");
    assert_eq!(snapshot.health, EdgeHealth::Healthy);
    assert_eq!(snapshot.collection.active_task_count, 1);
    assert_eq!(snapshot.protocols[0].connection_id, "sim-main");
    assert!(!snapshot.protocols[0].connected);
    assert_eq!(snapshot.collection.success_rate, 0.0);
    assert!(snapshot.algorithms.is_empty());
    assert!(snapshot.cloud_sync.connected);
    assert_eq!(snapshot.cloud_sync.reported_version, "2026.06.26-002");
}

#[test]
fn metrics_collector_accepts_observed_protocol_and_algorithm_metrics() {
    let applied = AppliedEdgeConfig::apply(package()).unwrap();
    let snapshot = RuntimeMetricsCollector::new("runtime-a", applied)
        .with_protocol_metrics(vec![edge_core::ProtocolRuntimeMetrics {
            connection_id: "sim-main".to_string(),
            protocol: "Simulated".to_string(),
            connected: true,
            latency_ms: 3,
            timeout_count: 0,
            error_count: 0,
            reconnect_count: 0,
        }])
        .with_algorithm_metrics(vec![edge_core::AlgorithmRuntimeMetrics {
            algorithm_id: "normalize".to_string(),
            healthy: true,
            last_run_latency_ms: 2,
            error_count: 0,
            alert_count: 0,
        }])
        .snapshot();

    assert!(snapshot.protocols[0].connected);
    assert_eq!(snapshot.protocols[0].latency_ms, 3);
    assert_eq!(snapshot.algorithms[0].algorithm_id, "normalize");
    assert_eq!(snapshot.algorithms[0].last_run_latency_ms, 2);
}

#[test]
fn metrics_collector_uses_real_collection_metrics_when_provided() {
    let applied = AppliedEdgeConfig::apply(package()).unwrap();
    let collector = RuntimeMetricsCollector::new("runtime-a", applied).with_collection_metrics(
        CollectionRuntimeMetrics {
            active_task_count: 1,
            success_rate: 0.5,
            average_latency_ms: 37,
            bad_point_count: 2,
        },
    );

    let snapshot = collector.snapshot();

    assert_eq!(snapshot.collection.success_rate, 0.5);
    assert_eq!(snapshot.collection.average_latency_ms, 37);
    assert_eq!(snapshot.collection.bad_point_count, 2);
    assert_eq!(snapshot.health, EdgeHealth::Degraded);
}

#[test]
fn metrics_collector_reports_mqtt_outbox_backlog_as_degraded() {
    let applied = AppliedEdgeConfig::apply(package()).unwrap();
    let snapshot = RuntimeMetricsCollector::new("runtime-a", applied)
        .with_mqtt_outbox_stats(MqttOutboxStats {
            pending_messages: 7,
            oldest_message_age_seconds: 42,
        })
        .snapshot();

    assert_eq!(snapshot.health, EdgeHealth::Degraded);
    assert_eq!(snapshot.local_store.backend, "rocksdb-mqtt-outbox");
    assert_eq!(snapshot.local_store.buffered_records, 7);
    assert_eq!(snapshot.local_store.oldest_buffer_age_seconds, 42);
    assert_eq!(snapshot.cloud_sync.pending_uploads, 7);
}

#[test]
fn host_system_metrics_sampler_reports_bounded_real_values() {
    let directory = tempfile::tempdir().unwrap();
    let mut sampler = HostSystemMetricsSampler::new(directory.path());

    let first = sampler.sample();
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    let second = sampler.sample();

    for value in [
        second.cpu_percent,
        second.memory_percent,
        second.disk_percent,
    ] {
        assert!(value.is_finite());
        assert!((0.0..=100.0).contains(&value));
    }
    assert!(second.process_uptime_seconds >= first.process_uptime_seconds);
    assert!(second.memory_percent > 0.0);
    assert!(second.disk_percent > 0.0);
}
