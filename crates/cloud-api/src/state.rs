use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use chrono::Utc;
use cloud_control::{CloudControlStore, EdgeNode, ReleaseRecord, ReleaseService, SqliteCloudStore};
use edge_core::{
    AlgorithmDsl, AlgorithmKind, AlgorithmRuntime, AlgorithmSpec, CloudSyncMetrics,
    CollectionRuntimeMetrics, CollectionTask, CommandRisk, CommandSpec, DataConfig,
    DataConfigCollection, DataConfigPayload, DataConfigPoint, DataConfigPublish, DeviceInstance,
    DeviceSpec, EdgeConfigPackage, EdgeHealth, EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot,
    EventSeverity, EventSpec, LocalStoreMetrics, MqttUplinkConfig, NumberRange, PointAddress,
    ProtocolConnection, ProtocolRuntimeMetrics, ProtocolType, SystemRuntimeMetrics, TelemetryPoint,
    TelemetryPointMapping, TelemetryType,
};

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Mutex<CloudControlStore>>,
    pub sqlite_store: Option<SqliteCloudStore>,
}

impl AppState {
    pub async fn with_sqlite(database_url: &str) -> Result<Self> {
        ensure_sqlite_parent(database_url).await?;
        let sqlite_store = SqliteCloudStore::connect(database_url).await?;
        let mut store = CloudControlStore::default();
        hydrate_from_sqlite(&sqlite_store, &mut store).await?;

        if store.edge_nodes().next().is_none() {
            store = demo_store();
            persist_store_snapshot(&sqlite_store, &store).await?;
        } else {
            ensure_default_mqtt_uplinks(&sqlite_store, &mut store).await?;
            ensure_default_data_configs(&sqlite_store, &mut store).await?;
        }

        Ok(Self {
            store: Arc::new(Mutex::new(store)),
            sqlite_store: Some(sqlite_store),
        })
    }

    pub async fn persist_config_package(&self, package: EdgeConfigPackage) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.upsert_config_package(package).await?;
        }
        Ok(())
    }

    pub async fn persist_edge_node(&self, node: EdgeNode) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.upsert_edge_node(node).await?;
        }
        Ok(())
    }

    pub async fn delete_edge_node(&self, edge_id: &str) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.delete_edge_node(edge_id).await?;
        }
        Ok(())
    }

    pub async fn persist_device_model(&self, model: DeviceSpec) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.upsert_device_model(model).await?;
        }
        Ok(())
    }

    pub async fn delete_device_model(&self, device_type: &str) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.delete_device_model(device_type).await?;
        }
        Ok(())
    }

    pub async fn persist_release(&self, release: ReleaseRecord) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.insert_release(release).await?;
        }
        Ok(())
    }

    pub async fn persist_release_report(
        &self,
        release_id: uuid::Uuid,
        reported_version: String,
    ) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store
                .mark_release_reported(release_id, reported_version)
                .await?;
        }
        Ok(())
    }

    pub async fn persist_runtime_metrics(
        &self,
        snapshot: EdgeRuntimeMetricsSnapshot,
    ) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.upsert_runtime_metrics(snapshot).await?;
        }
        Ok(())
    }

    pub async fn persist_runtime_event(&self, event: EdgeRuntimeEvent) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.push_runtime_event(event).await?;
        }
        Ok(())
    }

    pub async fn persist_mqtt_uplink(&self, edge_id: &str, uplink: MqttUplinkConfig) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.upsert_mqtt_uplink(edge_id, uplink).await?;
        }
        Ok(())
    }

    pub async fn persist_discovery_report(
        &self,
        edge_id: &str,
        report: edge_core::DiscoveryReport,
    ) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.insert_discovery_report(edge_id, report).await?;
        }
        Ok(())
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            store: Arc::new(Mutex::new(demo_store())),
            sqlite_store: None,
        }
    }
}

fn demo_store() -> CloudControlStore {
    let mut store = CloudControlStore::default();

    store.register_edge(
        EdgeNode::new("edge-dev", "研发实验室边端")
            .at_site("研发/实验室")
            .with_capability("protocol:modbus-rtu")
            .with_capability("transport:serial")
            .with_capability("uplink:mqtt")
            .with_capability("local-store:rocksdb"),
    );

    let pump_model = DeviceSpec::new("pump", "v1")
        .with_telemetry(vec![
            TelemetryPoint::new("pressure", TelemetryType::Float)
                .with_unit("MPa")
                .with_range(NumberRange::new(0.0, 20.0))
                .with_description("泵出口压力"),
            TelemetryPoint::new("running", TelemetryType::Boolean)
                .with_description("设备运行布尔量"),
        ])
        .with_commands(vec![CommandSpec::new("start", CommandRisk::Medium)])
        .with_events(vec![EventSpec {
            id: "pressure_high".to_string(),
            severity: EventSeverity::Warning,
        }]);

    let mut package = EdgeConfigPackage::new("edge-dev", "2026.06.26-001")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection {
            connection_id: "modbus-line-a".to_string(),
            protocol: ProtocolType::ModbusTcp,
            endpoint: Some("10.12.0.20:502".to_string()),
            serial: None,
        })
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtts://velamq.local:8883",
            "edge-dev-runtime-dev",
        ))
        .with_point_mapping(
            TelemetryPointMapping::new(
                "pressure",
                "pump-1",
                "pump.pressure",
                "modbus-line-a",
                PointAddress::modbus_holding_register(40001),
                TelemetryType::Float,
            )
            .with_unit("MPa")
            .with_interval_ms(1000),
        )
        .with_point_mapping(
            TelemetryPointMapping::new(
                "running",
                "pump-1",
                "pump.running",
                "modbus-line-a",
                PointAddress {
                    kind: "coil".to_string(),
                    value: "00001".to_string(),
                },
                TelemetryType::Boolean,
            )
            .with_interval_ms(1000),
        )
        .with_collection_task(CollectionTask::interval(
            "pump-main",
            "pump-1",
            vec!["pressure".to_string(), "running".to_string()],
            1000,
        ))
        .with_data_config(
            DataConfig::new(
                "pump_status",
                "泵状态上报",
                "pump-1",
                "modbus-line-a",
                DataConfigCollection::new(1000),
                DataConfigPublish::new(
                    "velamq-main",
                    "factory/{edge_id}/{device_id}/status",
                    DataConfigPayload::object(),
                ),
            )
            .with_point(
                DataConfigPoint::new(
                    "pressure",
                    "pump.pressure",
                    PointAddress::modbus_holding_register(40001),
                    TelemetryType::Float,
                    "pressure",
                )
                .with_unit("MPa"),
            )
            .with_point(DataConfigPoint::new(
                "running",
                "pump.running",
                PointAddress {
                    kind: "coil".to_string(),
                    value: "00001".to_string(),
                },
                TelemetryType::Boolean,
                "running",
            ))
            .with_algorithm("pump-anomaly-v1"),
        );
    package.device_models.push(pump_model.clone());
    package.algorithms.push(AlgorithmSpec {
        id: "pump-anomaly-v1".to_string(),
        version: "1.0.0".to_string(),
        kind: AlgorithmKind::ChangeReport,
        dsl: AlgorithmDsl::default(),
        runtime: AlgorithmRuntime::Onnx,
        inputs: vec!["pressure".to_string(), "running".to_string()],
        outputs: vec!["pump.anomaly_score".to_string()],
    });
    store.upsert_device_model(pump_model);
    store.upsert_mqtt_uplink("edge-dev", default_mqtt_uplink("edge-dev"));

    let release = ReleaseService::create_release(&mut store, package)
        .expect("demo config package should be valid");
    ReleaseService::mark_reported(&mut store, release.release_id, "2026.06.26-001")
        .expect("demo release should be reportable");

    store.upsert_runtime_metrics(EdgeRuntimeMetricsSnapshot {
        edge_id: "edge-dev".to_string(),
        runtime_id: "runtime-dev".to_string(),
        config_version: "2026.06.26-001".to_string(),
        timestamp: Utc::now(),
        health: EdgeHealth::Healthy,
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
        protocols: vec![ProtocolRuntimeMetrics {
            connection_id: "modbus-line-a".to_string(),
            protocol: "Modbus TCP".to_string(),
            connected: true,
            latency_ms: 12,
            timeout_count: 0,
            error_count: 0,
            reconnect_count: 0,
        }],
        local_store: LocalStoreMetrics {
            backend: "jsonl".to_string(),
            buffered_records: 0,
            oldest_buffer_age_seconds: 0,
            disk_usage_percent: 35.0,
        },
        algorithms: Vec::new(),
        cloud_sync: CloudSyncMetrics {
            connected: true,
            last_sync_seconds_ago: 8,
            pending_uploads: 0,
            desired_version: "2026.06.26-001".to_string(),
            reported_version: "2026.06.26-001".to_string(),
        },
    });

    store
}

async fn ensure_default_mqtt_uplinks(
    sqlite_store: &SqliteCloudStore,
    store: &mut CloudControlStore,
) -> Result<()> {
    let edge_ids = store
        .edge_nodes()
        .map(|edge| edge.edge_id.clone())
        .collect::<Vec<_>>();

    for edge_id in edge_ids {
        if store.mqtt_uplink(&edge_id).is_some() {
            continue;
        }

        let uplink = default_mqtt_uplink(&edge_id);
        if let Some(mut package) = store.latest_config_package_for_edge(&edge_id).cloned() {
            if package.mqtt_uplinks.is_empty() {
                package.mqtt_uplinks.push(uplink.clone());
                store.upsert_config_package(package.clone());
                sqlite_store.upsert_config_package(package).await?;
            }
        }

        store.upsert_mqtt_uplink(edge_id.clone(), uplink.clone());
        sqlite_store.upsert_mqtt_uplink(&edge_id, uplink).await?;
    }

    Ok(())
}

async fn ensure_default_data_configs(
    sqlite_store: &SqliteCloudStore,
    store: &mut CloudControlStore,
) -> Result<()> {
    let edge_ids = store
        .edge_nodes()
        .map(|edge| edge.edge_id.clone())
        .collect::<Vec<_>>();

    for edge_id in edge_ids {
        let Some(mut package) = store.latest_config_package_for_edge(&edge_id).cloned() else {
            continue;
        };

        if !package.data_configs.is_empty() {
            continue;
        }

        let Some(data_config) = default_data_config_from_package(&package) else {
            continue;
        };

        package.data_configs.push(data_config);
        store.upsert_config_package(package.clone());
        sqlite_store.upsert_config_package(package).await?;
    }

    Ok(())
}

fn default_data_config_from_package(package: &EdgeConfigPackage) -> Option<DataConfig> {
    let task = package
        .collection_tasks
        .iter()
        .find(|task| task.enabled && !task.point_ids.is_empty());
    let device_id = task.map(|task| task.device_id.clone()).or_else(|| {
        package
            .point_mappings
            .first()
            .map(|point| point.device_id.clone())
    })?;
    let interval_ms = task.map(|task| task.interval_ms).unwrap_or(1000);
    let point_ids = task.map(|task| task.point_ids.clone()).unwrap_or_else(|| {
        package
            .point_mappings
            .iter()
            .filter(|point| point.device_id == device_id)
            .map(|point| point.point_id.clone())
            .collect()
    });

    let points = point_ids
        .iter()
        .filter_map(|point_id| {
            package
                .point_mappings
                .iter()
                .find(|point| point.point_id == *point_id && point.device_id == device_id)
        })
        .collect::<Vec<_>>();
    let first_point = points.first()?;
    let sink = package.mqtt_uplinks.first()?;

    let mut data_config = DataConfig::new(
        "default_telemetry",
        "默认遥测上报",
        device_id,
        first_point.protocol_connection_id.clone(),
        DataConfigCollection::new(interval_ms),
        DataConfigPublish::new(
            sink.sink_id.clone(),
            "factory/{edge_id}/{device_id}/telemetry",
            DataConfigPayload::object(),
        )
        .with_qos(sink.qos),
    );

    for point in points {
        data_config = data_config.with_point(
            DataConfigPoint::new(
                point.point_id.clone(),
                point.semantic_id.clone(),
                point.address.clone(),
                point.value_type,
                default_json_field(&point.point_id),
            )
            .with_unit(point.unit.clone().unwrap_or_default()),
        );
        if data_config.points.last().is_some_and(|point| {
            point
                .unit
                .as_ref()
                .is_some_and(|unit| unit.trim().is_empty())
        }) {
            if let Some(point) = data_config.points.last_mut() {
                point.unit = None;
            }
        }
    }

    if data_config.points.is_empty() {
        return None;
    }

    Some(data_config)
}

fn default_json_field(point_id: &str) -> String {
    point_id
        .chars()
        .map(|character| match character {
            '.' | '-' | ' ' => '_',
            _ => character,
        })
        .collect()
}

fn default_mqtt_uplink(edge_id: &str) -> MqttUplinkConfig {
    MqttUplinkConfig::velamq(
        "velamq-main",
        "mqtts://velamq.local:8883",
        format!("{edge_id}-runtime-dev"),
    )
}

async fn hydrate_from_sqlite(
    sqlite_store: &SqliteCloudStore,
    store: &mut CloudControlStore,
) -> Result<()> {
    for node in sqlite_store.edge_nodes().await? {
        store.register_edge(node);
    }
    for model in sqlite_store.device_models().await? {
        store.upsert_device_model(model);
    }
    for package in sqlite_store.config_packages().await? {
        for model in &package.device_models {
            if store.device_model(&model.device_type).is_none() {
                store.upsert_device_model(model.clone());
            }
        }
        store.upsert_config_package(package);
    }
    for release in sqlite_store.releases().await? {
        store.insert_release(release);
    }
    for snapshot in sqlite_store.runtime_metrics_snapshots().await? {
        store.upsert_runtime_metrics(snapshot);
    }
    for event in sqlite_store.runtime_events().await? {
        store.push_runtime_event(event);
    }
    for (edge_id, uplink) in sqlite_store.mqtt_uplinks().await? {
        store.upsert_mqtt_uplink(edge_id, uplink);
    }
    for (edge_id, report) in sqlite_store.discovery_report_entries().await? {
        store.insert_discovery_report(edge_id, report);
    }
    Ok(())
}

async fn persist_store_snapshot(
    sqlite_store: &SqliteCloudStore,
    store: &CloudControlStore,
) -> Result<()> {
    for node in store.edge_nodes().cloned().collect::<Vec<_>>() {
        sqlite_store.upsert_edge_node(node).await?;
    }
    for package in store.config_packages().cloned().collect::<Vec<_>>() {
        sqlite_store.upsert_config_package(package).await?;
    }
    for model in store.device_models().cloned().collect::<Vec<_>>() {
        sqlite_store.upsert_device_model(model).await?;
    }
    for release in store.releases().cloned().collect::<Vec<_>>() {
        sqlite_store.insert_release(release).await?;
    }
    for snapshot in store
        .runtime_metrics_snapshots()
        .cloned()
        .collect::<Vec<_>>()
    {
        sqlite_store.upsert_runtime_metrics(snapshot).await?;
    }
    for event in store.runtime_events().to_vec() {
        sqlite_store.push_runtime_event(event).await?;
    }
    for (edge_id, uplink) in store.mqtt_uplinks() {
        sqlite_store
            .upsert_mqtt_uplink(edge_id, uplink.clone())
            .await?;
    }
    for (edge_id, reports) in store.discovery_report_entries() {
        for report in reports {
            sqlite_store
                .insert_discovery_report(edge_id, report.clone())
                .await?;
        }
    }
    Ok(())
}

async fn ensure_sqlite_parent(database_url: &str) -> Result<()> {
    let Some(path) = database_url.strip_prefix("sqlite://") else {
        return Ok(());
    };
    if path == ":memory:" {
        return Ok(());
    }

    let Some(parent) = std::path::Path::new(path).parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() {
        return Ok(());
    }

    tokio::fs::create_dir_all(parent)
        .await
        .with_context(|| format!("create sqlite parent directory: {}", parent.display()))?;
    Ok(())
}
