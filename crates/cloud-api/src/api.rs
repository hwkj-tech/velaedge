use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use cloud_control::{AuditAction, EdgeNode, ReleaseService, ReleaseStatus};
use edge_core::{
    AlgorithmDsl, AlgorithmInputBinding, AlgorithmKind, AlgorithmOutput, AlgorithmReportMode,
    AlgorithmReportPolicy, AlgorithmRuntime, AlgorithmSpec, AlgorithmStep, AlgorithmTrigger,
    CollectionTask, DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPayloadMode,
    DataConfigPoint, DataConfigPublish, DeviceSpec, DiscoveredPoint, DiscoveryReport,
    EdgeConfigPackage, EdgeHealth, EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot, MqttUplinkConfig,
    NumberRange, PointAddress, PointMappingSuggestion, ProtocolConnection, ProtocolType,
    TelemetryPoint, TelemetryPointMapping, TelemetryType,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};

use crate::AppState;

#[derive(Serialize)]
pub struct SummaryResponse {
    pub edge_count: usize,
    pub pending_release_count: usize,
}

pub fn app(state: AppState) -> Router {
    let api = Router::new()
        .route("/api/summary", get(summary))
        .route("/api/edge-nodes", get(edge_nodes).post(create_edge_node))
        .route(
            "/api/device-models",
            get(device_models).post(create_device_model),
        )
        .route("/api/device-models/{device_type}", put(save_device_model))
        .route("/api/protocol-connections", get(protocol_connections))
        .route(
            "/api/edges/{edge_id}/protocol-connections",
            get(edge_protocol_connections).post(create_edge_protocol_connection),
        )
        .route(
            "/api/edges/{edge_id}/protocol-connections/{connection_id}",
            put(save_edge_protocol_connection),
        )
        .route("/api/collection-tasks", get(collection_tasks))
        .route(
            "/api/edges/{edge_id}/data-configs",
            get(edge_data_configs).post(create_edge_data_config),
        )
        .route(
            "/api/edges/{edge_id}/data-configs/{config_id}",
            put(save_edge_data_config).delete(delete_edge_data_config),
        )
        .route(
            "/api/edges/{edge_id}/collection-tasks",
            get(edge_collection_tasks).post(create_edge_collection_task),
        )
        .route(
            "/api/edges/{edge_id}/collection-tasks/{task_id}",
            put(save_edge_collection_task),
        )
        .route("/api/algorithms", get(algorithms))
        .route(
            "/api/edges/{edge_id}/algorithms",
            get(edge_algorithms).post(create_edge_algorithm),
        )
        .route(
            "/api/edges/{edge_id}/algorithms/{algorithm_id}",
            put(save_edge_algorithm),
        )
        .route("/api/audit-records", get(audit_records))
        .route("/api/runtime-status", get(runtime_status))
        .route(
            "/api/edges/{edge_id}/desired-config",
            get(edge_desired_config),
        )
        .route(
            "/api/edges/{edge_id}/runtime-metrics",
            get(runtime_status).post(report_runtime_metrics),
        )
        .route(
            "/api/edges/{edge_id}/runtime-events",
            get(runtime_status).post(report_runtime_event),
        )
        .route(
            "/api/edges/{edge_id}/mqtt-uplink",
            get(edge_mqtt_uplink).put(save_edge_mqtt_uplink),
        )
        .route(
            "/api/edges/{edge_id}/discovery/run",
            post(run_edge_discovery),
        )
        .route(
            "/api/edges/{edge_id}/discovery/suggestions",
            get(edge_discovery_suggestions),
        )
        .route(
            "/api/edges/{edge_id}/reported-config",
            get(releases).post(edge_reported_config),
        )
        .route(
            "/api/edges/{edge_id}/credentials/rotate",
            post(rotate_edge_credentials),
        )
        .route(
            "/api/edges/{edge_id}/maintenance-mode",
            post(enable_edge_maintenance_mode),
        )
        .route(
            "/api/edges/{edge_id}/config/validate",
            post(validate_edge_config),
        )
        .route("/api/point-mappings", get(point_mappings))
        .route("/api/point-mappings/{point_id}", put(save_point_mapping))
        .route(
            "/api/edges/{edge_id}/point-mappings",
            get(edge_point_mappings).post(create_edge_point_mapping),
        )
        .route(
            "/api/edges/{edge_id}/point-mappings/{point_id}",
            put(save_edge_point_mapping),
        )
        .route("/api/releases", get(releases).post(create_release))
        .route(
            "/api/releases/publish",
            get(releases).post(publish_latest_release),
        )
        .route(
            "/api/edges/{edge_id}/releases/publish",
            post(publish_latest_release_for_edge),
        )
        .route("/api/edges/{edge_id}/releases/diff", post(release_diff))
        .route("/api/agent/safety-check", post(agent_safety_check))
        .route("/api/agent/suggestions", post(agent_suggestions))
        .with_state(state);

    let console_dir = console_dist_dir();
    let static_files = ServeDir::new(&console_dir)
        .not_found_service(ServeFile::new(console_dir.join("index.html")));

    api.fallback_service(static_files)
}

fn console_dist_dir() -> PathBuf {
    std::env::var("EDGEOPS_CONSOLE_DIST")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web/console/dist")
        })
}

async fn summary(State(state): State<AppState>) -> Json<SummaryResponse> {
    let store = state.store.lock().expect("store mutex poisoned");

    Json(SummaryResponse {
        edge_count: store.edge_nodes().count(),
        pending_release_count: store
            .releases()
            .filter(|release| release.status == ReleaseStatus::Pending)
            .count(),
    })
}

async fn point_mappings(State(state): State<AppState>) -> Json<Vec<PointMappingResponse>> {
    let store = state.store.lock().expect("store mutex poisoned");
    let mut points = Vec::new();

    for edge in store.edge_nodes() {
        let Some(package) = store.latest_config_package_for_edge(&edge.edge_id) else {
            continue;
        };
        for mapping in &package.point_mappings {
            points.push(point_mapping_response(package, mapping));
        }
    }

    Json(points)
}

async fn edge_point_mappings(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<Vec<PointMappingResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    let package = store
        .latest_config_package_for_edge(&edge_id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
    let mut points = package
        .point_mappings
        .iter()
        .map(|mapping| point_mapping_response(package, mapping))
        .collect::<Vec<_>>();
    points.sort_by(|left, right| left.point_id.cmp(&right.point_id));

    Ok(Json(points))
}

async fn create_edge_point_mapping(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    request: Option<Json<CreatePointMappingRequest>>,
) -> Result<(StatusCode, Json<PointMappingResponse>), (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let mapping = match request {
            Some(Json(request)) => build_point_mapping_from_create_request(&package, request)?,
            None => {
                let point_id = next_point_id(&package);
                let device_id = package
                    .devices
                    .first()
                    .map(|device| device.device_id.clone())
                    .unwrap_or_else(|| "device-draft-1".to_string());
                let connection_id = package
                    .protocol_connections
                    .first()
                    .map(|connection| connection.connection_id.clone())
                    .unwrap_or_else(|| "simulated-main".to_string());
                TelemetryPointMapping::new(
                    point_id.clone(),
                    device_id,
                    format!("pump.{point_id}"),
                    connection_id,
                    PointAddress::simulated(point_id.clone()),
                    TelemetryType::Float,
                )
                .with_unit("-")
            }
        };

        package.version = next_version(&package.version);
        package.point_mappings.push(mapping);
        let response = point_mapping_response(
            &package,
            package
                .point_mappings
                .last()
                .expect("new point mapping exists"),
        );
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id);
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok((StatusCode::CREATED, Json(response)))
}

async fn edge_nodes(
    Query(query): Query<EdgeNodesQuery>,
    State(state): State<AppState>,
) -> axum::response::Response {
    let store = state.store.lock().expect("store mutex poisoned");
    let mut rows = store
        .edge_nodes()
        .map(|edge| {
            let runtime = store.runtime_metrics(&edge.edge_id);
            edge_node_response(edge, runtime)
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| left.edge_id.cmp(&right.edge_id));

    if query.page.is_none() && query.page_size.is_none() {
        return Json(rows).into_response();
    }

    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
    let total = rows.len();
    let total_pages = total.div_ceil(page_size).max(1);
    let start = (page.saturating_sub(1) * page_size).min(total);
    let items = rows
        .into_iter()
        .skip(start)
        .take(page_size)
        .collect::<Vec<_>>();

    Json(EdgeNodesPageResponse {
        items,
        page,
        page_size,
        total,
        total_pages,
    })
    .into_response()
}

async fn edge_mqtt_uplink(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<MqttUplinkResponse>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    let uplink = store
        .mqtt_uplink(&edge_id)
        .cloned()
        .or_else(|| {
            store
                .latest_config_package_for_edge(&edge_id)
                .and_then(|package| package.mqtt_uplinks.first().cloned())
        })
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing mqtt uplink config"))?;

    Ok(Json(mqtt_uplink_response(uplink)))
}

async fn save_edge_mqtt_uplink(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    Json(request): Json<SaveMqttUplinkRequest>,
) -> Result<Json<MqttUplinkResponse>, (StatusCode, Json<ErrorResponse>)> {
    let uplink = MqttUplinkConfig {
        sink_id: request.sink_id,
        broker: request.broker,
        client_id: request.client_id,
        topic_template: request.topic_template,
        qos: request.qos,
        batch_size: request.batch_size,
        flush_interval_ms: request.flush_interval_ms,
    };

    let package_to_persist = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        package.version = next_version(&package.version);
        package.mqtt_uplinks = vec![uplink.clone()];
        store.upsert_config_package(package.clone());
        store.upsert_mqtt_uplink(edge_id.clone(), uplink.clone());
        package
    };

    state
        .persist_config_package(package_to_persist)
        .await
        .map_err(persistence_error)?;
    state
        .persist_mqtt_uplink(&edge_id, uplink.clone())
        .await
        .map_err(persistence_error)?;

    Ok(Json(mqtt_uplink_response(uplink)))
}

async fn run_edge_discovery(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    Json(request): Json<RunDiscoveryRequest>,
) -> Result<(StatusCode, Json<DiscoveryReportResponse>), (StatusCode, Json<ErrorResponse>)> {
    let report = simulated_discovery_report(&edge_id, &request.connection_id);
    {
        let mut store = state.store.lock().expect("store mutex poisoned");
        if store.latest_config_package_for_edge(&edge_id).is_none() {
            return Err(error(StatusCode::NOT_FOUND, "missing edge config package"));
        }
        store.insert_discovery_report(edge_id.clone(), report.clone());
        store.push_audit(AuditAction::UpdateConfig, format!("{edge_id}:discovery"));
    }

    state
        .persist_discovery_report(&edge_id, report.clone())
        .await
        .map_err(persistence_error)?;

    Ok((StatusCode::CREATED, Json(discovery_report_response(report))))
}

async fn edge_discovery_suggestions(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Json<Vec<PointMappingSuggestionResponse>> {
    let store = state.store.lock().expect("store mutex poisoned");
    Json(
        store
            .discovery_suggestions(&edge_id)
            .into_iter()
            .map(point_mapping_suggestion_response)
            .collect(),
    )
}

fn simulated_discovery_report(edge_id: &str, connection_id: &str) -> DiscoveryReport {
    let job_id = format!(
        "discovery-{edge_id}-{}",
        if edge_id == "edge-dev" { 1 } else { 0 }
    );
    DiscoveryReport::new(job_id, connection_id)
        .with_point(
            DiscoveredPoint::new(
                connection_id,
                PointAddress::modbus_holding_register(40001),
                TelemetryType::Float,
            )
            .with_sample_values(vec!["220.1".to_string(), "220.3".to_string()])
            .with_confidence(0.72),
        )
        .with_suggestion(
            PointMappingSuggestion::new(
                "pump_flow_rate",
                "pump-1",
                "pump.flow_rate",
                connection_id,
                PointAddress::modbus_holding_register(40001),
                TelemetryType::Float,
            )
            .with_unit("m3/h")
            .with_confidence(0.82)
            .with_evidence("数值范围和波动特征符合泵流量"),
        )
}

async fn create_edge_node(
    State(state): State<AppState>,
    Json(request): Json<CreateEdgeNodeRequest>,
) -> Result<(StatusCode, Json<EdgeNodeResponse>), (StatusCode, Json<ErrorResponse>)> {
    let (node, package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let edge_id = next_edge_id(&store.edge_nodes().cloned().collect::<Vec<_>>());
        let node = EdgeNode::new(
            edge_id.clone(),
            request
                .display_name
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "新边端待确认".to_string()),
        )
        .at_site(
            request
                .site
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "待分配".to_string()),
        )
        .with_capability("registration:draft");
        let package = EdgeConfigPackage::new(edge_id, "registration-draft-001");

        store.register_edge(node.clone());
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, node.edge_id.clone());

        let response = edge_node_response(&node, None);
        (node, package, response)
    };

    state
        .persist_edge_node(node)
        .await
        .map_err(persistence_error)?;
    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok((StatusCode::CREATED, Json(response)))
}

async fn rotate_edge_credentials(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<EdgeNodeActionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (node, credential_version) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let Some(existing) = store
            .edge_nodes()
            .find(|edge| edge.edge_id == edge_id)
            .cloned()
        else {
            return Err(error(StatusCode::NOT_FOUND, "missing edge node"));
        };
        let mut node = existing;
        let credential_version = next_credential_version(&node);
        node.capabilities
            .retain(|capability| !capability.starts_with("credential:"));
        node.capabilities
            .push(format!("credential:{credential_version}"));
        store.register_edge(node.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id.clone());
        (node, credential_version)
    };

    state
        .persist_edge_node(node)
        .await
        .map_err(persistence_error)?;

    Ok(Json(EdgeNodeActionResponse {
        action: "rotate_credentials".to_string(),
        credential_version: Some(credential_version),
        edge_id,
        message: "凭证已轮换".to_string(),
        status: None,
    }))
}

async fn enable_edge_maintenance_mode(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<EdgeNodeActionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let node = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let Some(existing) = store
            .edge_nodes()
            .find(|edge| edge.edge_id == edge_id)
            .cloned()
        else {
            return Err(error(StatusCode::NOT_FOUND, "missing edge node"));
        };
        let mut node = existing;
        if !node
            .capabilities
            .iter()
            .any(|capability| capability == "mode:maintenance")
        {
            node.capabilities.push("mode:maintenance".to_string());
        }
        store.register_edge(node.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id.clone());
        node
    };

    state
        .persist_edge_node(node)
        .await
        .map_err(persistence_error)?;

    Ok(Json(EdgeNodeActionResponse {
        action: "enable_maintenance".to_string(),
        credential_version: None,
        edge_id,
        message: "维护模式已启用".to_string(),
        status: Some("维护中".to_string()),
    }))
}

async fn device_models(State(state): State<AppState>) -> Json<Vec<DeviceModelResponse>> {
    let store = state.store.lock().expect("store mutex poisoned");
    let mut latest_by_type = BTreeMap::<String, (String, DeviceSpec)>::new();

    for package in store.config_packages() {
        for model in &package.device_models {
            let should_replace = latest_by_type
                .get(&model.device_type)
                .map(|(version, _)| package.version > *version)
                .unwrap_or(true);
            if should_replace {
                latest_by_type.insert(
                    model.device_type.clone(),
                    (package.version.clone(), model.clone()),
                );
            }
        }
    }
    let mut rows = latest_by_type
        .into_values()
        .map(|(_, model)| device_model_response(&model))
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| left.device_type.cmp(&right.device_type));

    Json(rows)
}

async fn create_device_model(
    State(state): State<AppState>,
    request: Option<Json<CreateDeviceModelRequest>>,
) -> Result<(StatusCode, Json<DeviceModelResponse>), (StatusCode, Json<ErrorResponse>)> {
    let (package, model, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let edge_id = default_config_edge_id(&store)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge node"))?;
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let model = match request {
            Some(Json(request)) => build_device_model_from_request(request)?,
            None => DeviceSpec::new(next_device_model_type(&package), "v1")
                .with_telemetry(vec![TelemetryPoint::new("status", TelemetryType::Boolean)
                    .with_description("设备状态")]),
        };

        package.version = next_version(&package.version);
        package.device_models.push(model.clone());
        store.upsert_device_model(model.clone());
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id);

        let response = device_model_response(&model);
        (package, model, response)
    };

    state
        .persist_device_model(model)
        .await
        .map_err(persistence_error)?;
    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok((StatusCode::CREATED, Json(response)))
}

async fn save_device_model(
    State(state): State<AppState>,
    Path(device_type): Path<String>,
    Json(request): Json<SaveDeviceModelRequest>,
) -> Result<Json<DeviceModelResponse>, (StatusCode, Json<ErrorResponse>)> {
    let model = build_device_model(device_type.clone(), request.version, request.telemetry)?;
    let (packages, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        if store.device_model(&device_type).is_none()
            && !store.config_packages().any(|package| {
                package
                    .device_models
                    .iter()
                    .any(|candidate| candidate.device_type == device_type)
            })
        {
            return Err(error(StatusCode::NOT_FOUND, "missing device model"));
        }

        store.upsert_device_model(model.clone());
        let edge_ids = store
            .edge_nodes()
            .map(|edge| edge.edge_id.clone())
            .collect::<Vec<_>>();
        let mut packages = Vec::new();
        for edge_id in edge_ids {
            let Some(mut package) = store.latest_config_package_for_edge(&edge_id).cloned() else {
                continue;
            };
            let Some(model_index) = package
                .device_models
                .iter()
                .position(|candidate| candidate.device_type == device_type)
            else {
                continue;
            };
            package.version = next_version(&package.version);
            package.device_models[model_index] = model.clone();
            store.upsert_config_package(package.clone());
            store.push_audit(AuditAction::UpdateConfig, edge_id);
            packages.push(package);
        }
        let response = device_model_response(&model);
        (packages, response)
    };

    state
        .persist_device_model(model)
        .await
        .map_err(persistence_error)?;
    for package in packages {
        state
            .persist_config_package(package)
            .await
            .map_err(persistence_error)?;
    }

    Ok(Json(response))
}

async fn protocol_connections(
    State(state): State<AppState>,
) -> Json<Vec<ProtocolConnectionResponse>> {
    let store = state.store.lock().expect("store mutex poisoned");
    let mut rows = Vec::new();

    for package in store.config_packages() {
        let runtime = store.runtime_metrics(&package.edge_id);
        for connection in &package.protocol_connections {
            let runtime_connection = runtime.and_then(|snapshot| {
                snapshot
                    .protocols
                    .iter()
                    .find(|protocol| protocol.connection_id == connection.connection_id)
            });
            rows.push(protocol_connection_response(
                package,
                connection,
                runtime_connection.map(|metrics| metrics.connected),
            ));
        }
    }
    rows.sort_by(|left, right| left.connection_id.cmp(&right.connection_id));

    Json(rows)
}

async fn edge_protocol_connections(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<Vec<ProtocolConnectionResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    let package = store
        .latest_config_package_for_edge(&edge_id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
    let runtime = store.runtime_metrics(&edge_id);
    let mut connections = package
        .protocol_connections
        .iter()
        .map(|connection| {
            let connected = runtime.and_then(|snapshot| {
                snapshot
                    .protocols
                    .iter()
                    .find(|protocol| protocol.connection_id == connection.connection_id)
                    .map(|metrics| metrics.connected)
            });
            protocol_connection_response(package, connection, connected)
        })
        .collect::<Vec<_>>();
    connections.sort_by(|left, right| left.connection_id.cmp(&right.connection_id));

    Ok(Json(connections))
}

async fn create_edge_protocol_connection(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    Json(request): Json<CreateProtocolConnectionRequest>,
) -> Result<(StatusCode, Json<ProtocolConnectionResponse>), (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let connection_id = next_connection_id(&package);
        let connection = ProtocolConnection {
            connection_id,
            protocol: request.protocol_type.unwrap_or(ProtocolType::ModbusTcp),
            endpoint: request
                .endpoint
                .filter(|endpoint| !endpoint.trim().is_empty()),
            serial: None,
        };

        package.version = next_version(&package.version);
        package.protocol_connections.push(connection);
        let response = protocol_connection_response(
            &package,
            package
                .protocol_connections
                .last()
                .expect("new protocol connection exists"),
            None,
        );
        store.upsert_config_package(package.clone());
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok((StatusCode::CREATED, Json(response)))
}

async fn collection_tasks(State(state): State<AppState>) -> Json<Vec<CollectionTaskResponse>> {
    let store = state.store.lock().expect("store mutex poisoned");
    let mut rows = Vec::new();

    for package in store.config_packages() {
        for task in &package.collection_tasks {
            rows.push(collection_task_response(package, task));
        }
    }
    rows.sort_by(|left, right| left.task_id.cmp(&right.task_id));

    Json(rows)
}

async fn edge_data_configs(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<Vec<DataConfigResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    let package = store
        .latest_config_package_for_edge(&edge_id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
    let mut configs = package
        .data_configs
        .iter()
        .map(|data_config| data_config_response(package, data_config))
        .collect::<Vec<_>>();
    configs.sort_by(|left, right| left.config_id.cmp(&right.config_id));

    Ok(Json(configs))
}

async fn create_edge_data_config(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    Json(request): Json<SaveDataConfigRequest>,
) -> Result<(StatusCode, Json<DataConfigResponse>), (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let data_config = build_data_config_from_request(&package, None, request)?;
        if package
            .data_configs
            .iter()
            .any(|candidate| candidate.config_id == data_config.config_id)
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!("data config `{}` already exists", data_config.config_id),
            ));
        }

        package.version = next_version(&package.version);
        package.data_configs.push(data_config);
        let response = data_config_response(
            &package,
            package
                .data_configs
                .last()
                .expect("new data config exists"),
        );
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id);
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok((StatusCode::CREATED, Json(response)))
}

async fn save_edge_data_config(
    State(state): State<AppState>,
    Path((edge_id, config_id)): Path<(String, String)>,
    Json(request): Json<SaveDataConfigRequest>,
) -> Result<Json<DataConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let config_index = package
            .data_configs
            .iter()
            .position(|candidate| candidate.config_id == config_id)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing data config"))?;

        let data_config = build_data_config_from_request(&package, Some(&config_id), request)?;
        if data_config.config_id != config_id
            && package
                .data_configs
                .iter()
                .any(|candidate| candidate.config_id == data_config.config_id)
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!("data config `{}` already exists", data_config.config_id),
            ));
        }

        package.version = next_version(&package.version);
        package.data_configs[config_index] = data_config;
        let response = data_config_response(&package, &package.data_configs[config_index]);
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id);
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok(Json(response))
}

async fn delete_edge_data_config(
    State(state): State<AppState>,
    Path((edge_id, config_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let package = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let before = package.data_configs.len();
        package
            .data_configs
            .retain(|candidate| candidate.config_id != config_id);
        if package.data_configs.len() == before {
            return Err(error(StatusCode::NOT_FOUND, "missing data config"));
        }
        package.version = next_version(&package.version);
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id);
        package
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn edge_collection_tasks(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<Vec<CollectionTaskResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    let package = store
        .latest_config_package_for_edge(&edge_id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
    let mut tasks = package
        .collection_tasks
        .iter()
        .map(|task| collection_task_response(package, task))
        .collect::<Vec<_>>();
    tasks.sort_by(|left, right| left.task_id.cmp(&right.task_id));

    Ok(Json(tasks))
}

async fn create_edge_collection_task(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    request: Option<Json<CreateCollectionTaskRequest>>,
) -> Result<(StatusCode, Json<CollectionTaskResponse>), (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let task = match request {
            Some(Json(request)) => build_collection_task_from_create_request(&package, request)?,
            None => {
                let device_id = package
                    .devices
                    .first()
                    .map(|device| device.device_id.clone())
                    .unwrap_or_else(|| "device-draft-1".to_string());
                let point_ids = package
                    .point_mappings
                    .iter()
                    .map(|mapping| mapping.point_id.clone())
                    .collect::<Vec<_>>();
                if point_ids.is_empty() {
                    return Err(error(
                        StatusCode::BAD_REQUEST,
                        "collection task requires at least one point mapping",
                    ));
                }
                CollectionTask::interval(next_task_id(&package), device_id, point_ids, 1000)
            }
        };

        package.version = next_version(&package.version);
        package.collection_tasks.push(task);
        let response = collection_task_response(
            &package,
            package
                .collection_tasks
                .last()
                .expect("new collection task exists"),
        );
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id);
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok((StatusCode::CREATED, Json(response)))
}

async fn algorithms(State(state): State<AppState>) -> Json<Vec<AlgorithmResponse>> {
    let store = state.store.lock().expect("store mutex poisoned");
    let mut rows = Vec::new();

    for package in store.config_packages() {
        for algorithm in &package.algorithms {
            rows.push(algorithm_response(package, algorithm));
        }
    }
    rows.sort_by(|left, right| left.algorithm_id.cmp(&right.algorithm_id));

    Json(rows)
}

async fn edge_algorithms(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<Vec<AlgorithmResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    let package = store
        .latest_config_package_for_edge(&edge_id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
    let mut algorithms = package
        .algorithms
        .iter()
        .map(|algorithm| algorithm_response(package, algorithm))
        .collect::<Vec<_>>();
    algorithms.sort_by(|left, right| left.algorithm_id.cmp(&right.algorithm_id));

    Ok(Json(algorithms))
}

async fn create_edge_algorithm(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    request: Option<Json<CreateAlgorithmRequest>>,
) -> Result<(StatusCode, Json<AlgorithmResponse>), (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let algorithm = match request {
            Some(Json(request)) => build_algorithm_from_create_request(&package, request)?,
            None => {
                let input = package
                    .point_mappings
                    .first()
                    .map(|mapping| mapping.point_id.clone())
                    .ok_or_else(|| {
                        error(StatusCode::BAD_REQUEST, "algorithm requires an input point")
                    })?;
                let algorithm_id = next_algorithm_id(&package);
                AlgorithmSpec {
                    id: algorithm_id.clone(),
                    version: "0.1.0".to_string(),
                    kind: AlgorithmKind::ChangeReport,
                    dsl: AlgorithmDsl::default(),
                    runtime: AlgorithmRuntime::Rule,
                    inputs: vec![input],
                    outputs: vec![format!("{algorithm_id}.output")],
                }
            }
        };

        package.version = next_version(&package.version);
        package.algorithms.push(algorithm);
        let response = algorithm_response(
            &package,
            package.algorithms.last().expect("new algorithm exists"),
        );
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id);
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok((StatusCode::CREATED, Json(response)))
}

async fn audit_records(State(state): State<AppState>) -> Json<Vec<AuditRecordResponse>> {
    let store = state.store.lock().expect("store mutex poisoned");
    let mut rows = store
        .audit_records()
        .iter()
        .map(|record| AuditRecordResponse {
            created_at: record.created_at.to_rfc3339(),
            time: record.created_at.format("%H:%M:%S").to_string(),
            actor: record.actor.clone(),
            action: format_audit_action(record.action),
            target: record.target.clone(),
            result: "成功".to_string(),
        })
        .collect::<Vec<_>>();
    rows.reverse();

    Json(rows)
}

async fn releases(State(state): State<AppState>) -> Json<ReleaseListResponse> {
    let store = state.store.lock().expect("store mutex poisoned");
    Json(release_list_response(&store))
}

async fn runtime_status(State(state): State<AppState>) -> Json<RuntimeStatusResponse> {
    let store = state.store.lock().expect("store mutex poisoned");
    Json(runtime_status_response(&store))
}

async fn edge_desired_config(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<EdgeDesiredConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    let package = store
        .latest_config_package_for_edge(&edge_id)
        .cloned()
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;

    Ok(Json(EdgeDesiredConfigResponse {
        edge_id,
        desired_version: package.version.clone(),
        package,
    }))
}

async fn edge_reported_config(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    Json(request): Json<EdgeReportedConfigRequest>,
) -> Result<Json<ReleaseListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let reported_version = request.reported_version;
    let (release_id, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let release_id = store
            .releases()
            .filter(|release| release.edge_id == edge_id)
            .max_by(|left, right| left.desired_version.cmp(&right.desired_version))
            .map(|release| release.release_id)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing release for edge"))?;

        ReleaseService::mark_reported(&mut store, release_id, reported_version.clone())
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing release for edge"))?;

        (release_id, release_list_response(&store))
    };

    state
        .persist_release_report(release_id, reported_version)
        .await
        .map_err(persistence_error)?;

    Ok(Json(response))
}

async fn report_runtime_metrics(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    Json(snapshot): Json<EdgeRuntimeMetricsSnapshot>,
) -> Result<Json<RuntimeStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    if snapshot.edge_id != edge_id {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "runtime metrics edge_id does not match request path",
        ));
    }

    let response = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        store.upsert_runtime_metrics(snapshot.clone());
        runtime_status_response(&store)
    };

    state
        .persist_runtime_metrics(snapshot)
        .await
        .map_err(persistence_error)?;

    Ok(Json(response))
}

async fn report_runtime_event(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    Json(event): Json<EdgeRuntimeEvent>,
) -> Result<Json<RuntimeStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    if event.edge_id != edge_id {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "runtime event edge_id does not match request path",
        ));
    }

    let response = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        store.push_runtime_event(event.clone());
        runtime_status_response(&store)
    };

    state
        .persist_runtime_event(event)
        .await
        .map_err(persistence_error)?;

    Ok(Json(response))
}

async fn validate_edge_config(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<ManagementActionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    let package = store
        .latest_config_package_for_edge(&edge_id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;

    Ok(Json(config_validation_response(package)))
}

async fn release_diff(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<ManagementActionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    let package = store
        .latest_config_package_for_edge(&edge_id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
    let latest_release = store
        .releases()
        .filter(|release| release.edge_id == edge_id)
        .max_by(|left, right| left.desired_version.cmp(&right.desired_version));
    let baseline = latest_release
        .map(|release| release.desired_version.clone())
        .unwrap_or_else(|| "-".to_string());

    Ok(Json(ManagementActionResponse {
        action: "release_diff".to_string(),
        details: vec![
            format!("基线版本 {baseline}"),
            format!("草稿版本 {}", package.version),
            format!("点位 {} 个", package.point_mappings.len()),
            format!("算法 {} 个", package.algorithms.len()),
        ],
        message: "配置差异摘要已生成".to_string(),
        status: "已生成".to_string(),
    }))
}

async fn agent_safety_check(State(state): State<AppState>) -> Json<AgentActionResponse> {
    let store = state.store.lock().expect("store mutex poisoned");
    let edge_count = store.edge_nodes().count();
    let pending_count = store
        .releases()
        .filter(|release| release.status == ReleaseStatus::Pending)
        .count();

    Json(AgentActionResponse {
        action: "agent_safety_check".to_string(),
        details: vec![
            format!("受管边端 {edge_count} 个"),
            format!("待发布版本 {pending_count} 个"),
            "高风险命令仍需人工确认".to_string(),
        ],
        message: "安全策略检查已完成".to_string(),
        status: "已通过".to_string(),
        suggestions: agent_suggestion_list(&store),
    })
}

async fn agent_suggestions(State(state): State<AppState>) -> Json<AgentActionResponse> {
    let store = state.store.lock().expect("store mutex poisoned");
    let suggestions = agent_suggestion_list(&store);

    Json(AgentActionResponse {
        action: "agent_generate_suggestions".to_string(),
        details: vec![format!("建议 {} 条", suggestions.len())],
        message: "Agent 建议已生成".to_string(),
        status: "待确认".to_string(),
        suggestions,
    })
}

async fn save_point_mapping(
    State(state): State<AppState>,
    Path(point_id): Path<String>,
    Json(request): Json<SavePointMappingRequest>,
) -> Result<Json<PointMappingResponse>, (StatusCode, Json<ErrorResponse>)> {
    save_point_mapping_for_edge_id(state, "edge-dev", point_id, request).await
}

async fn save_edge_point_mapping(
    State(state): State<AppState>,
    Path((edge_id, point_id)): Path<(String, String)>,
    Json(request): Json<SavePointMappingRequest>,
) -> Result<Json<PointMappingResponse>, (StatusCode, Json<ErrorResponse>)> {
    save_point_mapping_for_edge_id(state, &edge_id, point_id, request).await
}

async fn save_point_mapping_for_edge_id(
    state: AppState,
    edge_id: &str,
    point_id: String,
    request: SavePointMappingRequest,
) -> Result<Json<PointMappingResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        package.version = next_version(&package.version);

        let mapping_index = package
            .point_mappings
            .iter()
            .position(|mapping| mapping.point_id == point_id)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing point mapping"))?;

        {
            let mapping = &mut package.point_mappings[mapping_index];
            mapping.address = PointAddress {
                kind: request.address_kind,
                value: request.address_value,
            };
            mapping.interval_ms = request.interval_ms;
            mapping.unit = Some(request.unit);
        }

        let response = point_mapping_response(&package, &package.point_mappings[mapping_index]);
        store.upsert_config_package(package.clone());
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok(Json(response))
}

async fn save_edge_collection_task(
    State(state): State<AppState>,
    Path((edge_id, task_id)): Path<(String, String)>,
    Json(request): Json<SaveCollectionTaskRequest>,
) -> Result<Json<CollectionTaskResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;

        if !package
            .devices
            .iter()
            .any(|device| device.device_id == request.device_id)
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "collection task device missing",
            ));
        }
        if request.point_ids.is_empty() {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "collection task must include at least one point",
            ));
        }
        if let Some(missing_point_id) = request.point_ids.iter().find(|point_id| {
            !package
                .point_mappings
                .iter()
                .any(|mapping| mapping.point_id == **point_id)
        }) {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!("collection task point `{missing_point_id}` missing"),
            ));
        }

        let task_index = package
            .collection_tasks
            .iter()
            .position(|task| task.task_id == task_id)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing collection task"))?;

        package.version = next_version(&package.version);
        {
            let task = &mut package.collection_tasks[task_index];
            task.device_id = request.device_id;
            task.point_ids = request.point_ids;
            task.interval_ms = request.interval_ms.max(100);
            task.enabled = request.enabled;
        }

        let response = collection_task_response(&package, &package.collection_tasks[task_index]);
        store.upsert_config_package(package.clone());
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok(Json(response))
}

async fn save_edge_protocol_connection(
    State(state): State<AppState>,
    Path((edge_id, connection_id)): Path<(String, String)>,
    Json(request): Json<SaveProtocolConnectionRequest>,
) -> Result<Json<ProtocolConnectionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let connection_index = package
            .protocol_connections
            .iter()
            .position(|connection| connection.connection_id == connection_id)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing protocol connection"))?;

        package.version = next_version(&package.version);
        {
            let connection = &mut package.protocol_connections[connection_index];
            connection.protocol = request.protocol_type;
            connection.endpoint = request
                .endpoint
                .filter(|endpoint| !endpoint.trim().is_empty());
        }

        let response = protocol_connection_response(
            &package,
            &package.protocol_connections[connection_index],
            None,
        );
        store.upsert_config_package(package.clone());
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok(Json(response))
}

async fn save_edge_algorithm(
    State(state): State<AppState>,
    Path((edge_id, algorithm_id)): Path<(String, String)>,
    Json(request): Json<SaveAlgorithmRequest>,
) -> Result<Json<AlgorithmResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;

        validate_algorithm_dsl(&package, &request.dsl)?;

        let algorithm_index = package
            .algorithms
            .iter()
            .position(|algorithm| algorithm.id == algorithm_id)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing algorithm"))?;

        package.version = next_version(&package.version);
        {
            let algorithm = &mut package.algorithms[algorithm_index];
            algorithm.version = request.version;
            algorithm.kind = request.algorithm_kind;
            algorithm.dsl = request.dsl;
            algorithm.runtime = AlgorithmRuntime::Rule;
            algorithm.inputs = algorithm.inputs();
            algorithm.outputs = algorithm.outputs();
        }

        let response = algorithm_response(&package, &package.algorithms[algorithm_index]);
        store.upsert_config_package(package.clone());
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok(Json(response))
}

async fn publish_latest_release(
    State(state): State<AppState>,
) -> Result<Json<ReleaseListResponse>, (StatusCode, Json<ErrorResponse>)> {
    publish_latest_release_for_edge_id(state, "edge-dev").await
}

async fn publish_latest_release_for_edge(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<ReleaseListResponse>, (StatusCode, Json<ErrorResponse>)> {
    publish_latest_release_for_edge_id(state, &edge_id).await
}

async fn publish_latest_release_for_edge_id(
    state: AppState,
    edge_id: &str,
) -> Result<Json<ReleaseListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (release, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let package = store
            .latest_config_package_for_edge(edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let release = ReleaseService::create_release(&mut store, package).map_err(|errors| {
            error(
                StatusCode::BAD_REQUEST,
                errors
                    .into_iter()
                    .map(|error| error.message)
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })?;
        (release, release_list_response(&store))
    };

    state
        .persist_release(release)
        .await
        .map_err(persistence_error)?;

    Ok(Json(response))
}

fn release_list_response(store: &cloud_control::CloudControlStore) -> ReleaseListResponse {
    let mut releases = store.releases().cloned().collect::<Vec<_>>();
    releases.sort_by(|left, right| {
        right
            .desired_version
            .cmp(&left.desired_version)
            .then_with(|| release_status_rank(left.status).cmp(&release_status_rank(right.status)))
            .then_with(|| left.release_id.cmp(&right.release_id))
    });

    let draft_version = store
        .edge_nodes()
        .filter_map(|edge| store.latest_config_package_for_edge(&edge.edge_id))
        .map(|package| package.version.clone())
        .max()
        .or_else(|| {
            releases
                .first()
                .map(|release| release.desired_version.clone())
        })
        .unwrap_or_else(|| "-".to_string());

    ReleaseListResponse {
        draft_version,
        validation_status: "已通过".to_string(),
        change_summary: "云端配置包已生成".to_string(),
        rollout_policy: "单边端发布".to_string(),
        apply_results: releases
            .into_iter()
            .map(|release| ApplyResultResponse {
                edge_id: release.edge_id,
                desired_version: release.desired_version,
                reported_version: release.reported_version.unwrap_or_else(|| "-".to_string()),
                result: match release.status {
                    ReleaseStatus::Pending => "等待下发",
                    ReleaseStatus::Applied => "已应用",
                    ReleaseStatus::Failed => "应用失败",
                }
                .to_string(),
                heartbeat: "18 秒前".to_string(),
            })
            .collect(),
    }
}

fn release_status_rank(status: ReleaseStatus) -> u8 {
    match status {
        ReleaseStatus::Pending => 0,
        ReleaseStatus::Failed => 1,
        ReleaseStatus::Applied => 2,
    }
}

fn runtime_status_response(store: &cloud_control::CloudControlStore) -> RuntimeStatusResponse {
    let mut edges = store
        .runtime_metrics_snapshots()
        .cloned()
        .collect::<Vec<_>>();
    edges.sort_by(|left, right| left.edge_id.cmp(&right.edge_id));

    let average_collection_latency_ms = if edges.is_empty() {
        0
    } else {
        edges
            .iter()
            .map(|edge| edge.collection.average_latency_ms)
            .sum::<u64>()
            / edges.len() as u64
    };

    RuntimeStatusResponse {
        healthy_edge_count: edges
            .iter()
            .filter(|edge| edge.health == EdgeHealth::Healthy)
            .count(),
        degraded_edge_count: edges
            .iter()
            .filter(|edge| edge.health == EdgeHealth::Degraded)
            .count(),
        critical_edge_count: edges
            .iter()
            .filter(|edge| edge.health == EdgeHealth::Critical)
            .count(),
        average_collection_latency_ms,
        edges,
        events: store.runtime_events().to_vec(),
    }
}

async fn create_release(
    State(state): State<AppState>,
    Json(package): Json<EdgeConfigPackage>,
) -> Result<(StatusCode, Json<ReleaseResponse>), (StatusCode, Json<ErrorResponse>)> {
    let package_for_persist = package.clone();
    let release = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        ReleaseService::create_release(&mut store, package).map_err(|errors| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    message: errors
                        .into_iter()
                        .map(|error| error.message)
                        .collect::<Vec<_>>()
                        .join("; "),
                }),
            )
        })?
    };

    state
        .persist_config_package(package_for_persist)
        .await
        .map_err(persistence_error)?;
    state
        .persist_release(release.clone())
        .await
        .map_err(persistence_error)?;

    Ok((
        StatusCode::CREATED,
        Json(ReleaseResponse {
            release_id: release.release_id.to_string(),
            edge_id: release.edge_id,
            desired_version: release.desired_version,
            status: release.status,
        }),
    ))
}

#[derive(Serialize)]
pub struct ReleaseResponse {
    pub release_id: String,
    pub edge_id: String,
    pub desired_version: String,
    pub status: ReleaseStatus,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeNodeResponse {
    pub edge_id: String,
    pub display_name: String,
    pub site: String,
    pub runtime_id: String,
    pub status: String,
    pub resources: String,
    pub heartbeat: String,
    pub capabilities: Vec<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeNodesQuery {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeNodesPageResponse {
    pub items: Vec<EdgeNodeResponse>,
    pub page: usize,
    pub page_size: usize,
    pub total: usize,
    pub total_pages: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateEdgeNodeRequest {
    pub display_name: Option<String>,
    pub site: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeNodeActionResponse {
    pub action: String,
    pub credential_version: Option<String>,
    pub edge_id: String,
    pub message: String,
    pub status: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceModelResponse {
    pub device_type: String,
    pub version: String,
    pub telemetry: Vec<TelemetryModelResponse>,
    pub command_count: usize,
    pub event_count: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDeviceModelRequest {
    pub device_type: String,
    pub version: String,
    pub telemetry: Vec<CreateTelemetryModelRequest>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDeviceModelRequest {
    pub version: String,
    pub telemetry: Vec<CreateTelemetryModelRequest>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTelemetryModelRequest {
    pub telemetry_id: String,
    pub value_type: String,
    pub unit: Option<String>,
    pub range: Option<String>,
    pub description: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagementActionResponse {
    pub action: String,
    pub details: Vec<String>,
    pub message: String,
    pub status: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSuggestionResponse {
    pub title: String,
    pub detail: String,
    pub state: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentActionResponse {
    pub action: String,
    pub details: Vec<String>,
    pub message: String,
    pub status: String,
    pub suggestions: Vec<AgentSuggestionResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryModelResponse {
    pub telemetry_id: String,
    pub name: String,
    pub value_type: String,
    pub unit: String,
    pub range: String,
    pub description: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolConnectionResponse {
    pub edge_id: String,
    pub connection_id: String,
    pub protocol_type: ProtocolType,
    pub protocol: String,
    pub endpoint: String,
    pub status: String,
    pub policy: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionTaskResponse {
    pub edge_id: String,
    pub task_id: String,
    pub device_id: String,
    pub point_ids: Vec<String>,
    pub point_list: String,
    pub interval_ms: u64,
    pub interval: String,
    pub enabled: bool,
    pub status: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataConfigResponse {
    pub edge_id: String,
    pub config_id: String,
    pub name: String,
    pub enabled: bool,
    pub device_id: String,
    pub protocol_connection_id: String,
    pub collection: DataConfigCollectionDto,
    pub points: Vec<DataConfigPointDto>,
    pub publish: DataConfigPublishDto,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataConfigCollectionDto {
    pub period_ms: u64,
    pub timeout_ms: u64,
    pub retry_count: u32,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataConfigPointDto {
    pub point_id: String,
    pub semantic_id: String,
    pub address_kind: String,
    pub address_value: String,
    pub value_type: String,
    pub unit: Option<String>,
    pub json_field: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataConfigPublishDto {
    pub sink_id: String,
    pub topic_template: String,
    pub qos: u8,
    pub payload: DataConfigPayloadDto,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataConfigPayloadDto {
    pub mode: String,
    pub timestamp_field: String,
    pub include_quality: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlgorithmResponse {
    pub edge_id: String,
    pub algorithm_id: String,
    pub version: String,
    pub algorithm_kind: AlgorithmKind,
    pub dsl: AlgorithmDsl,
    pub runtime: AlgorithmRuntime,
    pub kind: String,
    pub input_ids: Vec<String>,
    pub output_ids: Vec<String>,
    pub inputs: String,
    pub outputs: String,
    pub execution: String,
    pub validation: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRecordResponse {
    pub created_at: String,
    pub time: String,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub result: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatusResponse {
    pub healthy_edge_count: usize,
    pub degraded_edge_count: usize,
    pub critical_edge_count: usize,
    pub average_collection_latency_ms: u64,
    pub edges: Vec<EdgeRuntimeMetricsSnapshot>,
    pub events: Vec<EdgeRuntimeEvent>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MqttUplinkResponse {
    pub sink_id: String,
    pub broker: String,
    pub client_id: String,
    pub topic_template: String,
    pub qos: u8,
    pub batch_size: u32,
    pub flush_interval_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveMqttUplinkRequest {
    pub sink_id: String,
    pub broker: String,
    pub client_id: String,
    pub topic_template: String,
    pub qos: u8,
    pub batch_size: u32,
    pub flush_interval_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDiscoveryRequest {
    pub connection_id: String,
    pub address_range: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryReportResponse {
    pub job_id: String,
    pub protocol_connection_id: String,
    pub discovered_points: Vec<DiscoveredPointResponse>,
    pub suggestions: Vec<PointMappingSuggestionResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredPointResponse {
    pub protocol_connection_id: String,
    pub address: String,
    pub value_type: String,
    pub sample_values: Vec<String>,
    pub confidence: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PointMappingSuggestionResponse {
    pub point_id: String,
    pub device_id: String,
    pub semantic_id: String,
    pub protocol_connection_id: String,
    pub address: String,
    pub value_type: String,
    pub unit: String,
    pub confidence: f64,
    pub evidence: String,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeDesiredConfigResponse {
    pub edge_id: String,
    pub desired_version: String,
    pub package: EdgeConfigPackage,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeReportedConfigRequest {
    pub reported_version: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePointMappingRequest {
    pub address_kind: String,
    pub address_value: String,
    pub interval_ms: u64,
    pub unit: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePointMappingRequest {
    pub point_id: Option<String>,
    pub device_id: Option<String>,
    pub semantic_id: Option<String>,
    pub connection_id: Option<String>,
    pub address_kind: Option<String>,
    pub address_value: Option<String>,
    pub value_type: Option<String>,
    pub unit: Option<String>,
    pub interval_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveCollectionTaskRequest {
    pub device_id: String,
    pub point_ids: Vec<String>,
    pub interval_ms: u64,
    pub enabled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCollectionTaskRequest {
    pub task_id: Option<String>,
    pub device_id: String,
    pub point_ids: Vec<String>,
    pub interval_ms: u64,
    pub enabled: Option<bool>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDataConfigRequest {
    pub config_id: String,
    pub name: String,
    pub enabled: bool,
    pub device_id: String,
    pub protocol_connection_id: String,
    pub collection: DataConfigCollectionDto,
    pub points: Vec<DataConfigPointDto>,
    pub publish: DataConfigPublishDto,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProtocolConnectionRequest {
    pub protocol_type: ProtocolType,
    pub endpoint: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProtocolConnectionRequest {
    pub protocol_type: Option<ProtocolType>,
    pub endpoint: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAlgorithmRequest {
    pub version: String,
    pub algorithm_kind: AlgorithmKind,
    pub dsl: AlgorithmDsl,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAlgorithmRequest {
    pub algorithm_id: Option<String>,
    pub version: Option<String>,
    pub algorithm_kind: Option<AlgorithmKind>,
    pub dsl: Option<AlgorithmDsl>,
    pub runtime: Option<AlgorithmRuntime>,
    #[serde(default)]
    pub input_ids: Vec<String>,
    #[serde(default)]
    pub output_ids: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PointMappingResponse {
    pub edge_id: String,
    pub point_id: String,
    pub point_name: String,
    pub device_id: String,
    pub device_model: String,
    pub semantic_telemetry: String,
    pub protocol: String,
    pub connection: String,
    pub address: String,
    pub value_type: String,
    pub read_write: String,
    pub unit: String,
    pub scale: String,
    pub interval: String,
    pub range: String,
    pub quality_rule: String,
    pub status: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseListResponse {
    pub draft_version: String,
    pub validation_status: String,
    pub change_summary: String,
    pub rollout_policy: String,
    pub apply_results: Vec<ApplyResultResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResultResponse {
    pub edge_id: String,
    pub desired_version: String,
    pub reported_version: String,
    pub result: String,
    pub heartbeat: String,
}

fn device_model_response(model: &DeviceSpec) -> DeviceModelResponse {
    DeviceModelResponse {
        device_type: model.device_type.clone(),
        version: model.version.clone(),
        telemetry: model
            .telemetry
            .iter()
            .map(|telemetry| TelemetryModelResponse {
                telemetry_id: telemetry.id.clone(),
                name: telemetry.id.clone(),
                value_type: format_telemetry_type(telemetry.value_type),
                unit: telemetry.unit.clone().unwrap_or_else(|| "-".to_string()),
                range: telemetry
                    .range
                    .as_ref()
                    .map(|range| format!("{}-{}", range.min, range.max))
                    .unwrap_or_else(|| "-".to_string()),
                description: telemetry
                    .description
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            })
            .collect(),
        command_count: model.commands.len(),
        event_count: model.events.len(),
    }
}

fn build_device_model_from_request(
    request: CreateDeviceModelRequest,
) -> Result<DeviceSpec, (StatusCode, Json<ErrorResponse>)> {
    build_device_model(request.device_type, request.version, request.telemetry)
}

fn build_device_model(
    device_type: String,
    version: String,
    telemetry_request: Vec<CreateTelemetryModelRequest>,
) -> Result<DeviceSpec, (StatusCode, Json<ErrorResponse>)> {
    let device_type = non_empty_field(device_type, "deviceType")?;
    let version = non_empty_field(version, "version")?;
    if telemetry_request.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "device model requires at least one telemetry point",
        ));
    }

    let mut telemetry = Vec::with_capacity(telemetry_request.len());
    for point in telemetry_request {
        let telemetry_id = non_empty_field(point.telemetry_id, "telemetryId")?;
        let value_type = parse_telemetry_type(&point.value_type)?;
        let mut telemetry_point = TelemetryPoint::new(telemetry_id, value_type);
        if let Some(unit) = non_empty_optional(point.unit) {
            telemetry_point = telemetry_point.with_unit(unit);
        }
        if let Some(range) = non_empty_optional(point.range) {
            telemetry_point = telemetry_point.with_range(parse_number_range(&range)?);
        }
        if let Some(description) = non_empty_optional(point.description) {
            telemetry_point = telemetry_point.with_description(description);
        }
        telemetry.push(telemetry_point);
    }

    Ok(DeviceSpec::new(device_type, version).with_telemetry(telemetry))
}

fn build_point_mapping_from_create_request(
    package: &EdgeConfigPackage,
    request: CreatePointMappingRequest,
) -> Result<TelemetryPointMapping, (StatusCode, Json<ErrorResponse>)> {
    let point_id = non_empty_optional(request.point_id).unwrap_or_else(|| next_point_id(package));
    if package
        .point_mappings
        .iter()
        .any(|mapping| mapping.point_id == point_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("point mapping `{point_id}` already exists"),
        ));
    }

    let device_id = non_empty_optional(request.device_id)
        .or_else(|| {
            package
                .devices
                .first()
                .map(|device| device.device_id.clone())
        })
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "point mapping requires a device"))?;
    if !package
        .devices
        .iter()
        .any(|device| device.device_id == device_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("point mapping device `{device_id}` missing"),
        ));
    }

    let connection_id = non_empty_optional(request.connection_id)
        .or_else(|| {
            package
                .protocol_connections
                .first()
                .map(|connection| connection.connection_id.clone())
        })
        .ok_or_else(|| {
            error(
                StatusCode::BAD_REQUEST,
                "point mapping requires a protocol connection",
            )
        })?;
    if !package
        .protocol_connections
        .iter()
        .any(|connection| connection.connection_id == connection_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("point mapping connection `{connection_id}` missing"),
        ));
    }

    let semantic_id = non_empty_optional(request.semantic_id).unwrap_or_else(|| point_id.clone());
    let address = PointAddress {
        kind: non_empty_optional(request.address_kind).unwrap_or_else(|| "simulated".to_string()),
        value: non_empty_optional(request.address_value).unwrap_or_else(|| point_id.clone()),
    };
    let value_type = request
        .value_type
        .as_deref()
        .map(parse_telemetry_type)
        .transpose()?
        .unwrap_or(TelemetryType::Float);

    Ok(TelemetryPointMapping::new(
        point_id,
        device_id,
        semantic_id,
        connection_id,
        address,
        value_type,
    )
    .with_unit(non_empty_optional(request.unit).unwrap_or_else(|| "-".to_string()))
    .with_interval_ms(request.interval_ms.unwrap_or(1000).max(100)))
}

fn build_collection_task_from_create_request(
    package: &EdgeConfigPackage,
    request: CreateCollectionTaskRequest,
) -> Result<CollectionTask, (StatusCode, Json<ErrorResponse>)> {
    if !package
        .devices
        .iter()
        .any(|device| device.device_id == request.device_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "collection task device missing",
        ));
    }
    if request.point_ids.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "collection task must include at least one point",
        ));
    }
    if let Some(missing_point_id) = request.point_ids.iter().find(|point_id| {
        !package
            .point_mappings
            .iter()
            .any(|mapping| mapping.point_id == **point_id)
    }) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("collection task point `{missing_point_id}` missing"),
        ));
    }

    let task_id = non_empty_optional(request.task_id).unwrap_or_else(|| next_task_id(package));
    if package
        .collection_tasks
        .iter()
        .any(|task| task.task_id == task_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("collection task `{task_id}` already exists"),
        ));
    }

    let mut task = CollectionTask::interval(
        task_id,
        request.device_id,
        request.point_ids,
        request.interval_ms.max(100),
    );
    task.enabled = request.enabled.unwrap_or(true);
    Ok(task)
}

fn build_data_config_from_request(
    package: &EdgeConfigPackage,
    existing_config_id: Option<&str>,
    request: SaveDataConfigRequest,
) -> Result<DataConfig, (StatusCode, Json<ErrorResponse>)> {
    let config_id = non_empty_field(request.config_id, "configId")?;
    if let Some(existing_config_id) = existing_config_id {
        if config_id != existing_config_id
            && package
                .data_configs
                .iter()
                .any(|candidate| candidate.config_id == config_id)
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!("data config `{config_id}` already exists"),
            ));
        }
    }

    let device_id = non_empty_field(request.device_id, "deviceId")?;
    if !package
        .devices
        .iter()
        .any(|device| device.device_id == device_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("data config device `{device_id}` missing"),
        ));
    }

    let protocol_connection_id =
        non_empty_field(request.protocol_connection_id, "protocolConnectionId")?;
    if !package
        .protocol_connections
        .iter()
        .any(|connection| connection.connection_id == protocol_connection_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("data config connection `{protocol_connection_id}` missing"),
        ));
    }

    if !package
        .mqtt_uplinks
        .iter()
        .any(|uplink| uplink.sink_id == request.publish.sink_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("data config mqtt sink `{}` missing", request.publish.sink_id),
        ));
    }
    if request.points.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "data config must include at least one point",
        ));
    }

    let collection = DataConfigCollection {
        period_ms: request.collection.period_ms.max(100),
        timeout_ms: request.collection.timeout_ms.max(1),
        retry_count: request.collection.retry_count,
    };
    let publish = DataConfigPublish {
        sink_id: non_empty_field(request.publish.sink_id, "sinkId")?,
        topic_template: non_empty_field(request.publish.topic_template, "topicTemplate")?,
        qos: request.publish.qos.min(2),
        payload: DataConfigPayload {
            mode: parse_data_config_payload_mode(&request.publish.payload.mode)?,
            timestamp_field: non_empty_field(
                request.publish.payload.timestamp_field,
                "timestampField",
            )?,
            include_quality: request.publish.payload.include_quality,
        },
    };
    let mut data_config = DataConfig::new(
        config_id,
        non_empty_field(request.name, "name")?,
        device_id,
        protocol_connection_id,
        collection,
        publish,
    );
    data_config.enabled = request.enabled;

    for point in request.points {
        let mut data_point = DataConfigPoint::new(
            non_empty_field(point.point_id, "pointId")?,
            non_empty_field(point.semantic_id, "semanticId")?,
            PointAddress {
                kind: non_empty_field(point.address_kind, "addressKind")?,
                value: non_empty_field(point.address_value, "addressValue")?,
            },
            parse_telemetry_type(&point.value_type)?,
            non_empty_field(point.json_field, "jsonField")?,
        );
        if let Some(unit) = non_empty_optional(point.unit) {
            data_point = data_point.with_unit(unit);
        }
        data_config = data_config.with_point(data_point);
    }

    Ok(data_config)
}

fn build_algorithm_from_create_request(
    package: &EdgeConfigPackage,
    request: CreateAlgorithmRequest,
) -> Result<AlgorithmSpec, (StatusCode, Json<ErrorResponse>)> {
    let algorithm_kind = request
        .algorithm_kind
        .unwrap_or(AlgorithmKind::ChangeReport);
    let dsl = request.dsl.unwrap_or_else(|| {
        legacy_algorithm_dsl(
            request.input_ids.clone(),
            request.output_ids.clone(),
            AlgorithmReportMode::OnChange,
        )
    });
    validate_algorithm_dsl(package, &dsl)?;

    let algorithm_id =
        non_empty_optional(request.algorithm_id).unwrap_or_else(|| next_algorithm_id(package));
    if package
        .algorithms
        .iter()
        .any(|algorithm| algorithm.id == algorithm_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("algorithm `{algorithm_id}` already exists"),
        ));
    }

    let version = non_empty_optional(request.version).unwrap_or_else(|| "0.1.0".to_string());
    let mut algorithm = AlgorithmSpec::dsl(algorithm_id, version, algorithm_kind, dsl);
    algorithm.runtime = request.runtime.unwrap_or(AlgorithmRuntime::Rule);
    Ok(algorithm)
}

fn legacy_algorithm_dsl(
    input_ids: Vec<String>,
    output_ids: Vec<String>,
    mode: AlgorithmReportMode,
) -> AlgorithmDsl {
    let first_input = input_ids
        .first()
        .cloned()
        .unwrap_or_else(|| "input".to_string());
    let first_output = output_ids
        .first()
        .cloned()
        .unwrap_or_else(|| format!("{first_input}.reported"));
    AlgorithmDsl {
        inputs: input_ids
            .into_iter()
            .enumerate()
            .map(|(index, point_id)| AlgorithmInputBinding::new(format!("p{index}"), point_id))
            .collect(),
        trigger: AlgorithmTrigger::on_sample(),
        steps: vec![AlgorithmStep::change_filter("p0", 0.0)],
        outputs: vec![AlgorithmOutput::virtual_point("p0", first_output)],
        report: AlgorithmReportPolicy::new(mode, "velamq-main"),
    }
}

fn validate_algorithm_dsl(
    package: &EdgeConfigPackage,
    dsl: &AlgorithmDsl,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if dsl.inputs.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "algorithm DSL must include at least one input point",
        ));
    }
    if dsl.outputs.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "algorithm DSL must include at least one output point",
        ));
    }
    if let Some(input) = dsl.inputs.iter().find(|input| {
        !package
            .point_mappings
            .iter()
            .any(|mapping| mapping.point_id == input.point_id)
    }) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("algorithm input point `{}` missing", input.point_id),
        ));
    }
    let aliases = dsl
        .inputs
        .iter()
        .map(|input| input.alias.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for step in &dsl.steps {
        let source = match step {
            AlgorithmStep::ChangeFilter { source, .. }
            | AlgorithmStep::WindowAggregate { source, .. }
            | AlgorithmStep::ThresholdRule { source, .. } => Some(source.as_str()),
            AlgorithmStep::Expression { .. } => None,
        };
        if let Some(source) = source {
            if !aliases.contains(source) {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    format!("algorithm step references missing input alias `{source}`"),
                ));
            }
        }
    }
    Ok(())
}

fn non_empty_field(
    value: String,
    field: &str,
) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err(error(
            StatusCode::BAD_REQUEST,
            format!("{field} cannot be empty"),
        ))
    } else {
        Ok(value)
    }
}

fn non_empty_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != "-")
}

fn edge_node_response(
    edge: &EdgeNode,
    runtime: Option<&EdgeRuntimeMetricsSnapshot>,
) -> EdgeNodeResponse {
    EdgeNodeResponse {
        edge_id: edge.edge_id.clone(),
        display_name: edge_display_name(edge),
        site: edge.site.clone().unwrap_or_else(|| "-".to_string()),
        runtime_id: runtime
            .map(|snapshot| snapshot.runtime_id.clone())
            .unwrap_or_else(|| "-".to_string()),
        status: edge_status(edge, runtime),
        resources: runtime
            .map(|snapshot| {
                format!(
                    "{} / {} / {}",
                    format_percent(snapshot.system.cpu_percent),
                    format_percent(snapshot.system.memory_percent),
                    format_percent(snapshot.system.disk_percent)
                )
            })
            .unwrap_or_else(|| "-".to_string()),
        heartbeat: runtime
            .map(|snapshot| format!("{} 秒前", snapshot.cloud_sync.last_sync_seconds_ago))
            .unwrap_or_else(|| "-".to_string()),
        capabilities: edge.capabilities.clone(),
    }
}

fn edge_display_name(edge: &EdgeNode) -> String {
    match edge.display_name.as_str() {
        "" | "新边端注册草稿" => "新边端待确认".to_string(),
        value => value.to_string(),
    }
}

fn edge_status(edge: &EdgeNode, runtime: Option<&EdgeRuntimeMetricsSnapshot>) -> String {
    if edge
        .capabilities
        .iter()
        .any(|capability| capability == "mode:maintenance")
    {
        return "维护中".to_string();
    }

    runtime
        .map(|snapshot| format_health(snapshot.health))
        .unwrap_or_else(|| "未上报".to_string())
}

fn next_edge_id(edges: &[EdgeNode]) -> String {
    let mut next = edges.len() + 1;
    loop {
        let candidate = format!("edge-draft-{next}");
        if edges.iter().all(|edge| edge.edge_id != candidate) {
            return candidate;
        }
        next += 1;
    }
}

fn next_credential_version(edge: &EdgeNode) -> String {
    let next = edge
        .capabilities
        .iter()
        .filter_map(|capability| capability.strip_prefix("credential:credential-v"))
        .filter_map(|version| version.parse::<u32>().ok())
        .max()
        .unwrap_or(1)
        + 1;
    format!("credential-v{next}")
}

fn default_config_edge_id(store: &cloud_control::CloudControlStore) -> Option<String> {
    store.edge_nodes().next().map(|edge| edge.edge_id.clone())
}

fn config_validation_response(package: &EdgeConfigPackage) -> ManagementActionResponse {
    ManagementActionResponse {
        action: "validate_config".to_string(),
        details: vec![
            format!("协议连接 {} 个", package.protocol_connections.len()),
            format!("点位 {} 个", package.point_mappings.len()),
            format!("采集任务 {} 个", package.collection_tasks.len()),
            format!("算法 {} 个", package.algorithms.len()),
        ],
        message: "配置校验已完成".to_string(),
        status: "已通过".to_string(),
    }
}

fn agent_suggestion_list(store: &cloud_control::CloudControlStore) -> Vec<AgentSuggestionResponse> {
    let total_points = store
        .edge_nodes()
        .filter_map(|edge| store.latest_config_package_for_edge(&edge.edge_id))
        .map(|package| package.point_mappings.len())
        .sum::<usize>();
    let pending_count = store
        .releases()
        .filter(|release| release.status == ReleaseStatus::Pending)
        .count();

    vec![
        AgentSuggestionResponse {
            title: "点位补全".to_string(),
            detail: format!("当前已配置 {total_points} 个点位，可按设备模型继续生成缺失映射"),
            state: "生成草稿".to_string(),
        },
        AgentSuggestionResponse {
            title: "发布风险".to_string(),
            detail: format!("当前有 {pending_count} 个待发布版本，建议先单边端灰度"),
            state: "需确认".to_string(),
        },
        AgentSuggestionResponse {
            title: "故障解释".to_string(),
            detail: "协议超时、点位质量和 runtime 事件可共同用于定位采集中断".to_string(),
            state: "可查看".to_string(),
        },
    ]
}

fn point_mapping_response(
    package: &EdgeConfigPackage,
    mapping: &TelemetryPointMapping,
) -> PointMappingResponse {
    let connections = package
        .protocol_connections
        .iter()
        .map(|connection| (connection.connection_id.as_str(), connection.protocol))
        .collect::<BTreeMap<_, _>>();
    let devices = package
        .devices
        .iter()
        .map(|device| (device.device_id.as_str(), device.device_type.as_str()))
        .collect::<BTreeMap<_, _>>();

    PointMappingResponse {
        edge_id: package.edge_id.clone(),
        point_id: mapping.point_id.clone(),
        point_name: mapping.point_id.clone(),
        device_id: mapping.device_id.clone(),
        device_model: devices
            .get(mapping.device_id.as_str())
            .copied()
            .unwrap_or("unknown")
            .to_string(),
        semantic_telemetry: mapping.semantic_id.clone(),
        protocol: connections
            .get(mapping.protocol_connection_id.as_str())
            .copied()
            .map(format_protocol)
            .unwrap_or_else(|| "Unknown".to_string()),
        connection: mapping.protocol_connection_id.clone(),
        address: format_address(&mapping.address),
        value_type: format_telemetry_type(mapping.value_type),
        read_write: "read".to_string(),
        unit: mapping.unit.clone().unwrap_or_else(|| "-".to_string()),
        scale: "1".to_string(),
        interval: format!("{}ms", mapping.interval_ms),
        range: mapping
            .range
            .as_ref()
            .map(|range| format!("{}-{}", range.min, range.max))
            .unwrap_or_else(|| "-".to_string()),
        quality_rule: "timeout->bad".to_string(),
        status: "启用".to_string(),
    }
}

fn collection_task_response(
    package: &EdgeConfigPackage,
    task: &CollectionTask,
) -> CollectionTaskResponse {
    CollectionTaskResponse {
        edge_id: package.edge_id.clone(),
        task_id: task.task_id.clone(),
        device_id: task.device_id.clone(),
        point_ids: task.point_ids.clone(),
        point_list: task.point_ids.join(", "),
        interval_ms: task.interval_ms,
        interval: format!("{}ms", task.interval_ms),
        enabled: task.enabled,
        status: if task.enabled { "启用" } else { "暂停" }.to_string(),
    }
}

fn data_config_response(
    package: &EdgeConfigPackage,
    data_config: &DataConfig,
) -> DataConfigResponse {
    DataConfigResponse {
        edge_id: package.edge_id.clone(),
        config_id: data_config.config_id.clone(),
        name: data_config.name.clone(),
        enabled: data_config.enabled,
        device_id: data_config.device_id.clone(),
        protocol_connection_id: data_config.protocol_connection_id.clone(),
        collection: DataConfigCollectionDto {
            period_ms: data_config.collection.period_ms,
            timeout_ms: data_config.collection.timeout_ms,
            retry_count: data_config.collection.retry_count,
        },
        points: data_config
            .points
            .iter()
            .map(|point| DataConfigPointDto {
                point_id: point.point_id.clone(),
                semantic_id: point.semantic_id.clone(),
                address_kind: point.address.kind.clone(),
                address_value: point.address.value.clone(),
                value_type: format_telemetry_type(point.value_type),
                unit: point.unit.clone(),
                json_field: point.json_field.clone(),
            })
            .collect(),
        publish: DataConfigPublishDto {
            sink_id: data_config.publish.sink_id.clone(),
            topic_template: data_config.publish.topic_template.clone(),
            qos: data_config.publish.qos,
            payload: DataConfigPayloadDto {
                mode: format_data_config_payload_mode(data_config.publish.payload.mode),
                timestamp_field: data_config.publish.payload.timestamp_field.clone(),
                include_quality: data_config.publish.payload.include_quality,
            },
        },
    }
}

fn protocol_connection_response(
    package: &EdgeConfigPackage,
    connection: &ProtocolConnection,
    connected: Option<bool>,
) -> ProtocolConnectionResponse {
    ProtocolConnectionResponse {
        edge_id: package.edge_id.clone(),
        connection_id: connection.connection_id.clone(),
        protocol_type: connection.protocol,
        protocol: format_protocol(connection.protocol),
        endpoint: connection
            .endpoint
            .clone()
            .unwrap_or_else(|| "runtime://simulated".to_string()),
        status: connected
            .map(|connected| {
                if connected {
                    "启用".to_string()
                } else {
                    "异常".to_string()
                }
            })
            .unwrap_or_else(|| "启用".to_string()),
        policy: "1000ms timeout / 3 retry".to_string(),
    }
}

fn algorithm_response(package: &EdgeConfigPackage, algorithm: &AlgorithmSpec) -> AlgorithmResponse {
    AlgorithmResponse {
        edge_id: package.edge_id.clone(),
        algorithm_id: algorithm.id.clone(),
        version: algorithm.version.clone(),
        algorithm_kind: algorithm.kind,
        dsl: algorithm.dsl.clone(),
        runtime: algorithm.runtime,
        kind: format_algorithm_kind(algorithm.kind),
        input_ids: algorithm.inputs(),
        output_ids: algorithm.outputs(),
        inputs: algorithm.inputs().join(", "),
        outputs: algorithm.outputs().join(", "),
        execution: "边端本地执行".to_string(),
        validation: "已通过".to_string(),
    }
}

fn next_version(version: &str) -> String {
    let Some((prefix, suffix)) = version.rsplit_once('-') else {
        return format!("{version}-001");
    };
    let next = suffix.parse::<u64>().unwrap_or(0) + 1;
    format!("{prefix}-{next:03}")
}

fn next_connection_id(package: &EdgeConfigPackage) -> String {
    let mut next = package.protocol_connections.len() + 1;
    loop {
        let candidate = format!("connection-draft-{next}");
        if package
            .protocol_connections
            .iter()
            .all(|connection| connection.connection_id != candidate)
        {
            return candidate;
        }
        next += 1;
    }
}

fn next_point_id(package: &EdgeConfigPackage) -> String {
    let mut next = package.point_mappings.len() + 1;
    loop {
        let candidate = format!("point-draft-{next}");
        if package
            .point_mappings
            .iter()
            .all(|mapping| mapping.point_id != candidate)
        {
            return candidate;
        }
        next += 1;
    }
}

fn next_task_id(package: &EdgeConfigPackage) -> String {
    let mut next = package.collection_tasks.len() + 1;
    loop {
        let candidate = format!("task-draft-{next}");
        if package
            .collection_tasks
            .iter()
            .all(|task| task.task_id != candidate)
        {
            return candidate;
        }
        next += 1;
    }
}

fn next_algorithm_id(package: &EdgeConfigPackage) -> String {
    let mut next = package.algorithms.len() + 1;
    loop {
        let candidate = format!("algorithm-draft-{next}");
        if package
            .algorithms
            .iter()
            .all(|algorithm| algorithm.id != candidate)
        {
            return candidate;
        }
        next += 1;
    }
}

fn next_device_model_type(package: &EdgeConfigPackage) -> String {
    let mut next = package.device_models.len() + 1;
    loop {
        let candidate = format!("device-model-draft-{next}");
        if package
            .device_models
            .iter()
            .all(|model| model.device_type != candidate)
        {
            return candidate;
        }
        next += 1;
    }
}

fn error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (
        status,
        Json(ErrorResponse {
            message: message.into(),
        }),
    )
}

fn persistence_error(cause: anyhow::Error) -> (StatusCode, Json<ErrorResponse>) {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("persist cloud state failed: {cause}"),
    )
}

fn format_address(address: &PointAddress) -> String {
    format!("{}:{}", address.kind, address.value)
}

fn format_health(health: EdgeHealth) -> String {
    match health {
        EdgeHealth::Healthy => "健康",
        EdgeHealth::Degraded => "降级",
        EdgeHealth::Critical => "严重",
        EdgeHealth::Offline => "离线",
    }
    .to_string()
}

fn format_percent(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}%")
    } else {
        format!("{value:.1}%")
    }
}

fn format_protocol(protocol: ProtocolType) -> String {
    match protocol {
        ProtocolType::Simulated => "Simulated",
        ProtocolType::ModbusTcp => "Modbus TCP",
        ProtocolType::ModbusRtu => "Modbus RTU",
        ProtocolType::Dlt645 => "DL/T645",
        ProtocolType::Iec101 => "IEC-101",
        ProtocolType::CustomSerial => "自定义串口",
        ProtocolType::OpcUa => "OPC UA",
        ProtocolType::SiemensS7 => "Siemens S7",
    }
    .to_string()
}

fn mqtt_uplink_response(uplink: MqttUplinkConfig) -> MqttUplinkResponse {
    MqttUplinkResponse {
        sink_id: uplink.sink_id,
        broker: uplink.broker,
        client_id: uplink.client_id,
        topic_template: uplink.topic_template,
        qos: uplink.qos,
        batch_size: uplink.batch_size,
        flush_interval_ms: uplink.flush_interval_ms,
    }
}

fn discovery_report_response(report: DiscoveryReport) -> DiscoveryReportResponse {
    DiscoveryReportResponse {
        job_id: report.job_id,
        protocol_connection_id: report.protocol_connection_id,
        discovered_points: report
            .discovered_points
            .into_iter()
            .map(discovered_point_response)
            .collect(),
        suggestions: report
            .suggestions
            .into_iter()
            .map(point_mapping_suggestion_response)
            .collect(),
    }
}

fn discovered_point_response(point: DiscoveredPoint) -> DiscoveredPointResponse {
    DiscoveredPointResponse {
        protocol_connection_id: point.protocol_connection_id,
        address: format_address(&point.address),
        value_type: format_telemetry_type(point.value_type),
        sample_values: point.sample_values,
        confidence: point.confidence,
    }
}

fn point_mapping_suggestion_response(
    suggestion: PointMappingSuggestion,
) -> PointMappingSuggestionResponse {
    PointMappingSuggestionResponse {
        point_id: suggestion.point_id,
        device_id: suggestion.device_id,
        semantic_id: suggestion.semantic_id,
        protocol_connection_id: suggestion.protocol_connection_id,
        address: format_address(&suggestion.address),
        value_type: format_telemetry_type(suggestion.value_type),
        unit: suggestion.unit.unwrap_or_else(|| "-".to_string()),
        confidence: suggestion.confidence,
        evidence: suggestion.evidence,
    }
}

fn format_telemetry_type(value_type: TelemetryType) -> String {
    match value_type {
        TelemetryType::Boolean => "bool",
        TelemetryType::Integer => "int64",
        TelemetryType::Float => "float32",
        TelemetryType::Text => "string",
    }
    .to_string()
}

fn parse_telemetry_type(
    value_type: &str,
) -> Result<TelemetryType, (StatusCode, Json<ErrorResponse>)> {
    match value_type.trim() {
        "bool" | "boolean" => Ok(TelemetryType::Boolean),
        "int64" | "int" | "integer" => Ok(TelemetryType::Integer),
        "float32" | "float" | "double" => Ok(TelemetryType::Float),
        "string" | "text" => Ok(TelemetryType::Text),
        value => Err(error(
            StatusCode::BAD_REQUEST,
            format!("unsupported telemetry valueType: {value}"),
        )),
    }
}

fn format_data_config_payload_mode(mode: DataConfigPayloadMode) -> String {
    match mode {
        DataConfigPayloadMode::Object => "object",
        DataConfigPayloadMode::Array => "array",
    }
    .to_string()
}

fn parse_data_config_payload_mode(
    mode: &str,
) -> Result<DataConfigPayloadMode, (StatusCode, Json<ErrorResponse>)> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "object" => Ok(DataConfigPayloadMode::Object),
        "array" => Ok(DataConfigPayloadMode::Array),
        value => Err(error(
            StatusCode::BAD_REQUEST,
            format!("unsupported data config payload mode: {value}"),
        )),
    }
}

fn parse_number_range(range: &str) -> Result<NumberRange, (StatusCode, Json<ErrorResponse>)> {
    let Some((min, max)) = range.split_once('-') else {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "range must use min-max format",
        ));
    };
    let min = min
        .trim()
        .parse::<f64>()
        .map_err(|_| error(StatusCode::BAD_REQUEST, "range min must be a number"))?;
    let max = max
        .trim()
        .parse::<f64>()
        .map_err(|_| error(StatusCode::BAD_REQUEST, "range max must be a number"))?;
    Ok(NumberRange::new(min, max))
}

fn format_algorithm_kind(kind: AlgorithmKind) -> String {
    match kind {
        AlgorithmKind::ChangeReport => "变化上报",
        AlgorithmKind::WindowAggregate => "窗口聚合",
        AlgorithmKind::ExpressionAggregate => "表达式聚合",
        AlgorithmKind::ThresholdRule => "阈值告警",
        AlgorithmKind::DurationRule => "持续条件",
        AlgorithmKind::Deadband => "死区过滤",
        AlgorithmKind::Debounce => "去抖动",
        AlgorithmKind::Statistics => "统计计算",
    }
    .to_string()
}

fn format_audit_action(action: AuditAction) -> String {
    match action {
        AuditAction::CreateRelease => "create_release",
        AuditAction::ApplyRelease => "apply_release",
        AuditAction::UpdateConfig => "update_config",
    }
    .to_string()
}
