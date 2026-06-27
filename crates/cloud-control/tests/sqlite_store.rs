use chrono::Utc;
use cloud_control::{EdgeNode, ReleaseRecord, ReleaseStatus, SqliteCloudStore};
use edge_core::{
    CloudSyncMetrics, CollectionRuntimeMetrics, CollectionTask, DeviceInstance, DiscoveredPoint,
    DiscoveryReport, EdgeConfigPackage, EdgeHealth, EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot,
    LocalStoreMetrics, MqttUplinkConfig, PointAddress, PointMappingSuggestion, ProtocolConnection,
    ProtocolType, RuntimeEventCategory, RuntimeEventSeverity, SystemRuntimeMetrics,
    TelemetryPointMapping, TelemetryType,
};

fn valid_package(version: &str) -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", version)
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

fn runtime_snapshot(edge_id: &str, health: EdgeHealth) -> EdgeRuntimeMetricsSnapshot {
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
            backend: "rocksdb".to_string(),
            buffered_records: 8,
            oldest_buffer_age_seconds: 12,
            disk_usage_percent: 35.0,
        },
        algorithms: Vec::new(),
        cloud_sync: CloudSyncMetrics {
            connected: true,
            last_sync_seconds_ago: 8,
            pending_uploads: 0,
            desired_version: "2026.06.26-002".to_string(),
            reported_version: "2026.06.26-002".to_string(),
        },
    }
}

#[tokio::test]
async fn sqlite_store_persists_cloud_control_state_across_reopen() {
    let tempdir = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite://{}", tempdir.path().join("cloud.db").display());

    {
        let store = SqliteCloudStore::connect(&database_url).await.unwrap();
        store
            .upsert_edge_node(
                EdgeNode::new("edge-dev", "研发实验室边端")
                    .at_site("研发/实验室")
                    .with_capability("protocol:modbus-tcp"),
            )
            .await
            .unwrap();
        store
            .upsert_config_package(valid_package("2026.06.26-001"))
            .await
            .unwrap();
        store
            .upsert_config_package(valid_package("2026.06.26-002"))
            .await
            .unwrap();

        let release = ReleaseRecord {
            release_id: uuid::Uuid::new_v4(),
            edge_id: "edge-dev".to_string(),
            desired_version: "2026.06.26-002".to_string(),
            reported_version: None,
            status: ReleaseStatus::Pending,
        };
        store.insert_release(release.clone()).await.unwrap();
        store
            .mark_release_reported(release.release_id, "2026.06.26-002")
            .await
            .unwrap();
        store
            .upsert_runtime_metrics(runtime_snapshot("edge-dev", EdgeHealth::Healthy))
            .await
            .unwrap();
        store
            .push_runtime_event(EdgeRuntimeEvent::new(
                "edge-dev",
                RuntimeEventSeverity::Warning,
                RuntimeEventCategory::Protocol,
                "modbus.timeout",
                "Modbus TCP read timeout",
            ))
            .await
            .unwrap();
    }

    let reopened = SqliteCloudStore::connect(&database_url).await.unwrap();
    let edges = reopened.edge_nodes().await.unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].edge_id, "edge-dev");

    let latest = reopened
        .latest_config_package_for_edge("edge-dev")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(latest.version, "2026.06.26-002");

    let releases = reopened.releases().await.unwrap();
    assert_eq!(releases.len(), 1);
    assert_eq!(
        releases[0].reported_version.as_deref(),
        Some("2026.06.26-002")
    );
    assert_eq!(releases[0].status, ReleaseStatus::Applied);

    let metrics = reopened.runtime_metrics("edge-dev").await.unwrap().unwrap();
    assert_eq!(metrics.runtime_id, "runtime-a");
    assert_eq!(metrics.local_store.backend, "rocksdb");

    let events = reopened.runtime_events().await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].code, "modbus.timeout");
}

#[tokio::test]
async fn sqlite_store_persists_mqtt_uplink_and_discovery_reports() {
    let tempdir = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite://{}", tempdir.path().join("cloud.db").display());

    let uplink = MqttUplinkConfig::velamq(
        "velamq-main",
        "mqtts://velamq.local:8883",
        "edge-dev-runtime",
    )
    .with_topic_template("edge/{edge_id}/device/{device_id}/telemetry")
    .with_qos(1);
    let report = DiscoveryReport::new("job-1", "meter-rs485-bus-1")
        .with_point(
            DiscoveredPoint::new(
                "meter-rs485-bus-1",
                PointAddress::modbus_holding_register(40001),
                TelemetryType::Float,
            )
            .with_sample_values(vec!["220.1".to_string(), "220.3".to_string()])
            .with_confidence(0.72),
        )
        .with_suggestion(
            PointMappingSuggestion::new(
                "meter_voltage_a",
                "meter-1",
                "electric.voltage_a",
                "meter-rs485-bus-1",
                PointAddress::modbus_holding_register(40001),
                TelemetryType::Float,
            )
            .with_unit("V")
            .with_confidence(0.82)
            .with_evidence("数值范围和波动特征符合 A 相电压"),
        );

    {
        let store = SqliteCloudStore::connect(&database_url).await.unwrap();
        store
            .upsert_mqtt_uplink("edge-dev", uplink.clone())
            .await
            .unwrap();
        store
            .insert_discovery_report("edge-dev", report.clone())
            .await
            .unwrap();
    }

    let reopened = SqliteCloudStore::connect(&database_url).await.unwrap();
    let saved_uplink = reopened
        .mqtt_uplink("edge-dev")
        .await
        .unwrap()
        .expect("uplink should be saved");
    assert_eq!(saved_uplink, uplink);

    let reports = reopened.discovery_reports("edge-dev").await.unwrap();
    assert_eq!(reports, vec![report]);
    let suggestions = reopened.discovery_suggestions("edge-dev").await.unwrap();
    assert_eq!(suggestions.len(), 1);
    assert_eq!(suggestions[0].point_id, "meter_voltage_a");
}

#[tokio::test]
async fn sqlite_store_migrates_legacy_mqtt_protocol_packages_on_read() {
    let tempdir = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite://{}", tempdir.path().join("cloud.db").display());
    let store = SqliteCloudStore::connect(&database_url).await.unwrap();
    let legacy_package = serde_json::json!({
        "edge_id": "edge-dev",
        "version": "2026.06.26-legacy",
        "device_models": [],
        "devices": [],
        "protocol_connections": [
            {
                "connection_id": "legacy-mqtt-line",
                "protocol": "Mqtt",
                "endpoint": "mqtt://legacy-broker"
            }
        ],
        "point_mappings": [],
        "collection_tasks": [],
        "algorithms": []
    });

    sqlx::query(
        r#"
        INSERT INTO config_packages (edge_id, version, package_json)
        VALUES (?1, ?2, ?3)
        "#,
    )
    .bind("edge-dev")
    .bind("2026.06.26-legacy")
    .bind(legacy_package.to_string())
    .execute(store.pool())
    .await
    .unwrap();

    let package = store
        .latest_config_package_for_edge("edge-dev")
        .await
        .unwrap()
        .expect("legacy package should remain readable");

    assert_eq!(package.version, "2026.06.26-legacy");
    assert_eq!(
        package.protocol_connections[0].protocol,
        ProtocolType::CustomSerial
    );
}
