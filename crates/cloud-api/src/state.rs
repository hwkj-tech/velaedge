use std::{
    env,
    sync::{Arc, Mutex},
};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use cloud_control::{
    AgentConversation, AgentProposal, AuditRecord, CloudControlStore, EdgeAccessCredential,
    EdgeNode, KnowledgeDocument, PointSet, PointSetPoint, Product, ProductVersion,
    ProductVersionStatus, Project, ReleaseRecord, ReleaseService, SqliteCloudStore,
};
use edge_core::{
    AlgorithmDsl, AlgorithmInputBinding, AlgorithmKind, AlgorithmOutput, AlgorithmReportMode,
    AlgorithmReportPolicy, AlgorithmSpec, AlgorithmStep, AlgorithmTrigger, CloudSyncMetrics,
    CollectionRuntimeMetrics, CollectionTask, CommandRisk, CommandSpec, DataConfig,
    DataConfigCollection, DataConfigPayload, DataConfigPoint, DataConfigPublish, DeviceInstance,
    DeviceSpec, EdgeConfigPackage, EdgeHealth, EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot,
    EventSeverity, EventSpec, LocalStoreMetrics, MqttUplinkConfig, NumberRange, PointAddress,
    ProtocolConnection, ProtocolRuntimeMetrics, ProtocolType, SystemRuntimeMetrics, TelemetryPoint,
    TelemetryPointMapping, TelemetryType,
};

use crate::{
    agent_service::AgentService, auth::ApiAuthConfig, gateway::EdgeGatewayCommandRegistry,
};

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Mutex<CloudControlStore>>,
    pub sqlite_store: Option<SqliteCloudStore>,
    pub gateway_commands: EdgeGatewayCommandRegistry,
    pub agent_service: AgentService,
    pub api_auth: ApiAuthConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootstrapMode {
    Demo,
    Empty,
}

impl BootstrapMode {
    pub fn from_env() -> Result<Self> {
        Self::resolve(
            env::var("EDGEOPS_BOOTSTRAP_MODE").ok().as_deref(),
            env::var("EDGEOPS_API_AUTH_MODE").ok().as_deref(),
        )
    }

    fn resolve(bootstrap_mode: Option<&str>, api_auth_mode: Option<&str>) -> Result<Self> {
        match bootstrap_mode
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(value) if value.eq_ignore_ascii_case("demo") => Ok(Self::Demo),
            Some(value) if value.eq_ignore_ascii_case("empty") => Ok(Self::Empty),
            Some(value) => bail!("EDGEOPS_BOOTSTRAP_MODE must be 'demo' or 'empty', got '{value}'"),
            None if api_auth_mode
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("required")) =>
            {
                Ok(Self::Empty)
            }
            None => Ok(Self::Demo),
        }
    }
}

impl AppState {
    pub async fn with_sqlite(database_url: &str) -> Result<Self> {
        Self::with_sqlite_bootstrap(database_url, BootstrapMode::from_env()?).await
    }

    pub async fn with_sqlite_bootstrap(
        database_url: &str,
        bootstrap_mode: BootstrapMode,
    ) -> Result<Self> {
        ensure_sqlite_parent(database_url).await?;
        let sqlite_store = SqliteCloudStore::connect(database_url).await?;
        let mut store = CloudControlStore::default();
        hydrate_from_sqlite(&sqlite_store, &mut store).await?;

        if bootstrap_mode == BootstrapMode::Demo {
            if store.edge_nodes().next().is_none() {
                store = demo_store();
                persist_store_snapshot(&sqlite_store, &store).await?;
            } else {
                ensure_default_mqtt_uplinks(&sqlite_store, &mut store).await?;
                ensure_default_data_configs(&sqlite_store, &mut store).await?;
                ensure_default_catalog(&sqlite_store, &mut store).await?;
                ensure_default_edge_product_bindings(&sqlite_store, &mut store).await?;
            }
        }

        Ok(Self {
            store: Arc::new(Mutex::new(store)),
            sqlite_store: Some(sqlite_store),
            gateway_commands: EdgeGatewayCommandRegistry::default(),
            agent_service: AgentService::from_env(),
            api_auth: ApiAuthConfig::from_env()?,
        })
    }

    pub async fn persist_config_package(&self, package: EdgeConfigPackage) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.upsert_config_package(package).await?;
        }
        Ok(())
    }

    pub fn with_agent_service(mut self, agent_service: AgentService) -> Self {
        self.agent_service = agent_service;
        self
    }

    pub fn with_api_auth(mut self, api_auth: ApiAuthConfig) -> Self {
        self.api_auth = api_auth;
        self
    }

    pub async fn persist_edge_node(&self, node: EdgeNode) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.upsert_edge_node(node).await?;
        }
        Ok(())
    }

    pub async fn persist_edge_credential(&self, credential: EdgeAccessCredential) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.replace_edge_credential(credential).await?;
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

    pub async fn persist_project(&self, project: Project) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.upsert_project(project).await?;
        }
        Ok(())
    }

    pub async fn delete_project(&self, project_id: &str) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.delete_project(project_id).await?;
        }
        Ok(())
    }

    pub async fn persist_point_set(&self, point_set: PointSet) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.upsert_point_set(point_set).await?;
        }
        Ok(())
    }

    pub async fn delete_point_set(&self, point_set_id: &str) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.delete_point_set(point_set_id).await?;
        }
        Ok(())
    }

    pub async fn persist_product(&self, product: Product) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.upsert_product(product).await?;
        }
        Ok(())
    }

    pub async fn delete_product(&self, product_id: &str) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.delete_product(product_id).await?;
        }
        Ok(())
    }

    pub async fn persist_product_version(&self, version: ProductVersion) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.upsert_product_version(version).await?;
        }
        Ok(())
    }

    pub async fn persist_product_version_transition(
        &self,
        product: Product,
        versions: Vec<ProductVersion>,
        edge_nodes: Vec<EdgeNode>,
        packages: Vec<EdgeConfigPackage>,
        releases: Vec<cloud_control::ReleaseRecord>,
    ) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store
                .transition_product_version(product, versions, edge_nodes, packages, releases)
                .await?;
        }
        Ok(())
    }

    pub async fn delete_product_version(&self, product_id: &str, version: &str) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.delete_product_version(product_id, version).await?;
        }
        Ok(())
    }

    pub async fn persist_agent_proposal_transition(
        &self,
        proposal: AgentProposal,
        audit: AuditRecord,
    ) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store
                .upsert_agent_proposal_with_audit(proposal, audit)
                .await?;
        }
        Ok(())
    }

    pub async fn persist_knowledge_document_transition(
        &self,
        document: KnowledgeDocument,
        audit: AuditRecord,
    ) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store
                .upsert_knowledge_document_with_audit(document, audit)
                .await?;
        }
        Ok(())
    }

    pub async fn delete_knowledge_document_transition(
        &self,
        document_id: uuid::Uuid,
        audit: AuditRecord,
    ) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store
                .delete_knowledge_document_with_audit(document_id, audit)
                .await?;
        }
        Ok(())
    }

    pub async fn persist_agent_conversation(&self, conversation: AgentConversation) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store.upsert_agent_conversation(conversation).await?;
        }
        Ok(())
    }

    pub async fn persist_agent_conversation_transition(
        &self,
        conversation: AgentConversation,
        audit: AuditRecord,
    ) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store
                .upsert_agent_conversation_with_audit(conversation, audit)
                .await?;
        }
        Ok(())
    }

    pub async fn delete_agent_conversation_transition(
        &self,
        conversation_id: uuid::Uuid,
        audit: AuditRecord,
    ) -> Result<()> {
        if let Some(store) = &self.sqlite_store {
            store
                .delete_agent_conversation_with_audit(conversation_id, audit)
                .await?;
        }
        Ok(())
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            store: Arc::new(Mutex::new(demo_store())),
            sqlite_store: None,
            gateway_commands: EdgeGatewayCommandRegistry::default(),
            agent_service: AgentService::from_env(),
            api_auth: ApiAuthConfig::disabled(),
        }
    }
}

fn demo_store() -> CloudControlStore {
    let mut store = CloudControlStore::default();

    let mut demo_edge = EdgeNode::new("edge-dev", "研发实验室边端")
        .at_site("研发/实验室")
        .with_capability("protocol:modbus-rtu")
        .with_capability("transport:serial")
        .with_capability("uplink:mqtt")
        .with_capability("local-store:rocksdb")
        .with_capability("project:demo-plant")
        .with_capability("product:pump-collection-uplink")
        .bind_product("demo-plant", "pump-collection-uplink", "v1.4.3");
    demo_edge.reported_product_version = Some("2026.06.26-001".to_string());
    store.register_edge(demo_edge);

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
    package.algorithms.push(AlgorithmSpec::dsl(
        "pump-anomaly-v1",
        "1.0.0",
        AlgorithmKind::ChangeReport,
        AlgorithmDsl {
            inputs: vec![
                AlgorithmInputBinding::new("pressure", "pressure"),
                AlgorithmInputBinding::new("running", "running"),
            ],
            trigger: AlgorithmTrigger::on_any_input(),
            steps: vec![AlgorithmStep::change_filter("pressure", 0.05)],
            outputs: vec![AlgorithmOutput::virtual_point(
                "pressure",
                "pump.anomaly_score",
            )],
            report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnChange, "velamq-main"),
        },
    ));
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

    seed_default_catalog(&mut store);

    store
}

async fn ensure_default_catalog(
    sqlite_store: &SqliteCloudStore,
    store: &mut CloudControlStore,
) -> Result<()> {
    if store.projects().next().is_some() {
        return Ok(());
    }
    seed_default_catalog(store);
    for project in store.projects().cloned().collect::<Vec<_>>() {
        sqlite_store.upsert_project(project).await?;
    }
    for point_set in store.point_sets().cloned().collect::<Vec<_>>() {
        sqlite_store.upsert_point_set(point_set).await?;
    }
    for product in store.products().cloned().collect::<Vec<_>>() {
        sqlite_store.upsert_product(product).await?;
    }
    for version in store.product_versions().cloned().collect::<Vec<_>>() {
        sqlite_store.upsert_product_version(version).await?;
    }
    Ok(())
}

async fn ensure_default_edge_product_bindings(
    sqlite_store: &SqliteCloudStore,
    store: &mut CloudControlStore,
) -> Result<()> {
    let nodes = store.edge_nodes().cloned().collect::<Vec<_>>();
    for mut node in nodes {
        if node.product_id.is_some() && node.desired_product_version.is_some() {
            continue;
        }

        let product_id = node
            .capabilities
            .iter()
            .find_map(|capability| capability.strip_prefix("product:"))
            .map(str::to_string)
            .or_else(|| (node.edge_id == "edge-dev").then(|| "pump-collection-uplink".to_string()));
        let Some(product_id) = product_id else {
            continue;
        };
        let Some(product) = store.product(&product_id).cloned() else {
            continue;
        };
        let Some(desired_version) = product.latest_version.clone() else {
            continue;
        };

        node.project_id = Some(product.project_id);
        node.product_id = Some(product_id);
        node.desired_product_version = Some(desired_version);
        store.register_edge(node.clone());
        sqlite_store.upsert_edge_node(node).await?;
    }
    Ok(())
}

fn seed_default_catalog(store: &mut CloudControlStore) {
    let mut demo_project = Project::new("demo-plant", "demo-plant");
    demo_project.owner = "platform-team".to_string();
    demo_project.description =
        "研发实验室与 demo 产线共用项目，承载串口采集、边缘计算和 velaMQ 上报。".to_string();
    store.upsert_project(demo_project);

    let mut energy_project = Project::new("energy-demo", "energy-demo");
    energy_project.owner = "energy-team".to_string();
    energy_project.description = "能源计量场景的产品与边端隔离空间。".to_string();
    store.upsert_project(energy_project);

    let mut pump_points = PointSet::new(
        "pump-standard-points",
        "demo-plant",
        "泵站标准点位",
        ProtocolType::ModbusRtu,
    );
    pump_points.description = "泵出口压力和运行状态点位集合。".to_string();
    pump_points.points = vec![
        PointSetPoint {
            point_id: "pump_pressure".to_string(),
            semantic_id: "pump.pressure".to_string(),
            address: PointAddress::modbus_holding_register(40011),
            value_type: TelemetryType::Float,
            unit: Some("MPa".to_string()),
            interval_ms: 1000,
        },
        PointSetPoint {
            point_id: "pump_running".to_string(),
            semantic_id: "pump.running".to_string(),
            address: PointAddress {
                kind: "coil".to_string(),
                value: "00001".to_string(),
            },
            value_type: TelemetryType::Boolean,
            unit: None,
            interval_ms: 1000,
        },
    ];
    store.upsert_point_set(pump_points);

    let mut meter_points = PointSet::new(
        "meter-standard-points",
        "demo-plant",
        "电表标准点位",
        ProtocolType::ModbusRtu,
    );
    meter_points.points = vec![
        PointSetPoint {
            point_id: "meter_voltage_a".to_string(),
            semantic_id: "electric.voltage_a".to_string(),
            address: PointAddress::modbus_holding_register(40001),
            value_type: TelemetryType::Float,
            unit: Some("V".to_string()),
            interval_ms: 1000,
        },
        PointSetPoint {
            point_id: "meter_current_a".to_string(),
            semantic_id: "electric.current_a".to_string(),
            address: PointAddress::modbus_holding_register(40003),
            value_type: TelemetryType::Float,
            unit: Some("A".to_string()),
            interval_ms: 1000,
        },
    ];
    store.upsert_point_set(meter_points);

    let mut energy_points = PointSet::new(
        "energy-standard-points",
        "demo-plant",
        "能耗标准点位",
        ProtocolType::ModbusRtu,
    );
    energy_points.points = vec![PointSetPoint {
        point_id: "energy_power".to_string(),
        semantic_id: "energy.power".to_string(),
        address: PointAddress::modbus_holding_register(40101),
        value_type: TelemetryType::Float,
        unit: Some("kW".to_string()),
        interval_ms: 5000,
    }];
    store.upsert_point_set(energy_points);

    let catalog = [
        (
            "pump-collection-uplink",
            "泵站状态模板",
            "pump-station",
            "v1.4.3",
            "pump-standard-points",
        ),
        (
            "modbus-rtu-meter-basic",
            "Modbus 电表标准模板",
            "meter",
            "v1.2.0",
            "meter-standard-points",
        ),
        (
            "energy-window-report",
            "能耗聚合模板",
            "energy",
            "v1.1.0",
            "energy-standard-points",
        ),
    ];
    for (product_id, name, product_type, version, point_set_id) in catalog {
        let mut product = Product::new(product_id, "demo-plant", name, product_type);
        product.latest_version = Some(version.to_string());
        product.description = format!("{name}的版本化边端配置");
        store.upsert_product(product);

        let mut product_version = ProductVersion::draft(product_id, version);
        product_version.status = ProductVersionStatus::Published;
        product_version.point_set_ids = vec![point_set_id.to_string()];
        if product_id == "pump-collection-uplink" {
            if let Some(package) = store.latest_config_package_for_edge("edge-dev").cloned() {
                product_version.device_models = package.device_models;
                product_version.devices = package.devices;
                product_version.protocol_connections = package.protocol_connections;
                product_version.collection_tasks = package.collection_tasks;
                product_version.algorithms = package.algorithms;
                product_version.data_configs = package.data_configs;
                product_version.mqtt_uplinks = package.mqtt_uplinks;
            }
        }
        store.upsert_product_version(product_version);
    }
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

#[cfg(test)]
mod bootstrap_tests {
    use super::BootstrapMode;

    #[test]
    fn explicit_bootstrap_mode_wins_over_auth_default() {
        assert_eq!(
            BootstrapMode::resolve(Some("demo"), Some("required")).unwrap(),
            BootstrapMode::Demo
        );
        assert_eq!(
            BootstrapMode::resolve(Some("empty"), Some("disabled")).unwrap(),
            BootstrapMode::Empty
        );
    }

    #[test]
    fn required_auth_defaults_to_empty_and_local_defaults_to_demo() {
        assert_eq!(
            BootstrapMode::resolve(None, Some("required")).unwrap(),
            BootstrapMode::Empty
        );
        assert_eq!(
            BootstrapMode::resolve(None, Some("disabled")).unwrap(),
            BootstrapMode::Demo
        );
        assert_eq!(
            BootstrapMode::resolve(None, None).unwrap(),
            BootstrapMode::Demo
        );
    }

    #[test]
    fn invalid_bootstrap_mode_is_rejected() {
        let error = BootstrapMode::resolve(Some("seed"), None).unwrap_err();
        assert!(error.to_string().contains("EDGEOPS_BOOTSTRAP_MODE"));
    }
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
    for credential in sqlite_store.edge_credentials().await? {
        store.upsert_edge_credential(credential);
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
    for project in sqlite_store.projects().await? {
        store.upsert_project(project);
    }
    for point_set in sqlite_store.point_sets().await? {
        store.upsert_point_set(point_set);
    }
    for product in sqlite_store.products().await? {
        store.upsert_product(product);
    }
    for version in sqlite_store.product_versions().await? {
        store.upsert_product_version(version);
    }
    for proposal in sqlite_store.agent_proposals().await? {
        store.upsert_agent_proposal(proposal);
    }
    for document in sqlite_store.knowledge_documents().await? {
        store.upsert_knowledge_document(document);
    }
    for conversation in sqlite_store.agent_conversations().await? {
        store.upsert_agent_conversation(conversation);
    }
    for audit in sqlite_store.audit_records().await? {
        store.push_audit_record(audit);
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
    for credential in store.edge_credentials().cloned().collect::<Vec<_>>() {
        sqlite_store.upsert_edge_credential(credential).await?;
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
    for project in store.projects().cloned().collect::<Vec<_>>() {
        sqlite_store.upsert_project(project).await?;
    }
    for point_set in store.point_sets().cloned().collect::<Vec<_>>() {
        sqlite_store.upsert_point_set(point_set).await?;
    }
    for product in store.products().cloned().collect::<Vec<_>>() {
        sqlite_store.upsert_product(product).await?;
    }
    for version in store.product_versions().cloned().collect::<Vec<_>>() {
        sqlite_store.upsert_product_version(version).await?;
    }
    for proposal in store.agent_proposals().cloned().collect::<Vec<_>>() {
        sqlite_store.upsert_agent_proposal(proposal).await?;
    }
    for audit in store.audit_records().iter().cloned() {
        sqlite_store.push_audit_record(audit).await?;
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
