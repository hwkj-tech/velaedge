use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use cloud_control::{AuditAction, ReleaseService, ReleaseStatus};
use edge_core::{
    AlgorithmRuntime, AlgorithmSpec, CollectionTask, EdgeConfigPackage, EdgeHealth,
    EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot, PointAddress, ProtocolConnection, ProtocolType,
    TelemetryPointMapping, TelemetryType,
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
        .route("/api/edge-nodes", get(edge_nodes))
        .route("/api/device-models", get(device_models))
        .route("/api/protocol-connections", get(protocol_connections))
        .route(
            "/api/edges/{edge_id}/protocol-connections",
            get(edge_protocol_connections),
        )
        .route(
            "/api/edges/{edge_id}/protocol-connections/{connection_id}",
            put(save_edge_protocol_connection),
        )
        .route("/api/collection-tasks", get(collection_tasks))
        .route(
            "/api/edges/{edge_id}/collection-tasks",
            get(edge_collection_tasks),
        )
        .route(
            "/api/edges/{edge_id}/collection-tasks/{task_id}",
            put(save_edge_collection_task),
        )
        .route("/api/algorithms", get(algorithms))
        .route("/api/edges/{edge_id}/algorithms", get(edge_algorithms))
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
            "/api/edges/{edge_id}/reported-config",
            get(releases).post(edge_reported_config),
        )
        .route("/api/point-mappings", get(point_mappings))
        .route("/api/point-mappings/{point_id}", put(save_point_mapping))
        .route(
            "/api/edges/{edge_id}/point-mappings",
            get(edge_point_mappings),
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

async fn edge_nodes(State(state): State<AppState>) -> Json<Vec<EdgeNodeResponse>> {
    let store = state.store.lock().expect("store mutex poisoned");
    let mut rows = store
        .edge_nodes()
        .map(|edge| {
            let runtime = store.runtime_metrics(&edge.edge_id);
            EdgeNodeResponse {
                edge_id: edge.edge_id.clone(),
                display_name: edge.display_name.clone(),
                site: edge.site.clone().unwrap_or_else(|| "-".to_string()),
                runtime_id: runtime
                    .map(|snapshot| snapshot.runtime_id.clone())
                    .unwrap_or_else(|| "-".to_string()),
                status: runtime
                    .map(|snapshot| format_health(snapshot.health))
                    .unwrap_or_else(|| "未上报".to_string()),
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
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| left.edge_id.cmp(&right.edge_id));

    Json(rows)
}

async fn device_models(State(state): State<AppState>) -> Json<Vec<DeviceModelResponse>> {
    let store = state.store.lock().expect("store mutex poisoned");
    let mut rows = Vec::new();

    for package in store.config_packages() {
        for model in &package.device_models {
            rows.push(DeviceModelResponse {
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
            });
        }
    }
    rows.sort_by(|left, right| left.device_type.cmp(&right.device_type));

    Json(rows)
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

        if request.input_ids.is_empty() {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "algorithm must include at least one input",
            ));
        }
        if let Some(missing_point_id) = request.input_ids.iter().find(|point_id| {
            !package
                .point_mappings
                .iter()
                .any(|mapping| mapping.point_id == **point_id)
        }) {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!("algorithm input point `{missing_point_id}` missing"),
            ));
        }

        let algorithm_index = package
            .algorithms
            .iter()
            .position(|algorithm| algorithm.id == algorithm_id)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing algorithm"))?;

        package.version = next_version(&package.version);
        {
            let algorithm = &mut package.algorithms[algorithm_index];
            algorithm.version = request.version;
            algorithm.runtime = request.runtime;
            algorithm.inputs = request.input_ids;
            algorithm.outputs = request.output_ids;
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceModelResponse {
    pub device_type: String,
    pub version: String,
    pub telemetry: Vec<TelemetryModelResponse>,
    pub command_count: usize,
    pub event_count: usize,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlgorithmResponse {
    pub edge_id: String,
    pub algorithm_id: String,
    pub version: String,
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
pub struct SaveCollectionTaskRequest {
    pub device_id: String,
    pub point_ids: Vec<String>,
    pub interval_ms: u64,
    pub enabled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProtocolConnectionRequest {
    pub protocol_type: ProtocolType,
    pub endpoint: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAlgorithmRequest {
    pub version: String,
    pub runtime: AlgorithmRuntime,
    pub input_ids: Vec<String>,
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
        runtime: algorithm.runtime,
        kind: format_algorithm_runtime(algorithm.runtime),
        input_ids: algorithm.inputs.clone(),
        output_ids: algorithm.outputs.clone(),
        inputs: algorithm.inputs.join(", "),
        outputs: algorithm.outputs.join(", "),
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
        ProtocolType::OpcUa => "OPC UA",
        ProtocolType::Mqtt => "MQTT",
        ProtocolType::SiemensS7 => "Siemens S7",
    }
    .to_string()
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

fn format_algorithm_runtime(runtime: AlgorithmRuntime) -> String {
    match runtime {
        AlgorithmRuntime::Rule => "规则算法",
        AlgorithmRuntime::Wasm => "WASM 算法",
        AlgorithmRuntime::Onnx => "异常检测",
        AlgorithmRuntime::Python => "Python 算法",
    }
    .to_string()
}

fn format_audit_action(action: AuditAction) -> String {
    match action {
        AuditAction::CreateRelease => "create_release",
        AuditAction::ApplyRelease => "apply_release",
    }
    .to_string()
}
