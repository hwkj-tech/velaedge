use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use cloud_control::{ReleaseService, ReleaseStatus};
use edge_core::{
    EdgeConfigPackage, EdgeHealth, EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot, PointAddress,
    ProtocolType, TelemetryPointMapping, TelemetryType,
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
        .route("/api/releases", get(releases).post(create_release))
        .route(
            "/api/releases/publish",
            get(releases).post(publish_latest_release),
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
    let mut store = state.store.lock().expect("store mutex poisoned");
    let release_id = store
        .releases()
        .filter(|release| release.edge_id == edge_id)
        .max_by(|left, right| left.desired_version.cmp(&right.desired_version))
        .map(|release| release.release_id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing release for edge"))?;

    ReleaseService::mark_reported(&mut store, release_id, request.reported_version)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing release for edge"))?;

    Ok(Json(release_list_response(&store)))
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

    let mut store = state.store.lock().expect("store mutex poisoned");
    store.upsert_runtime_metrics(snapshot);

    Ok(Json(runtime_status_response(&store)))
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

    let mut store = state.store.lock().expect("store mutex poisoned");
    store.push_runtime_event(event);

    Ok(Json(runtime_status_response(&store)))
}

async fn save_point_mapping(
    State(state): State<AppState>,
    Path(point_id): Path<String>,
    Json(request): Json<SavePointMappingRequest>,
) -> Result<Json<PointMappingResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mut store = state.store.lock().expect("store mutex poisoned");
    let mut package = store
        .latest_config_package_for_edge("edge-dev")
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
    store.upsert_config_package(package);

    Ok(Json(response))
}

async fn publish_latest_release(
    State(state): State<AppState>,
) -> Result<Json<ReleaseListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mut store = state.store.lock().expect("store mutex poisoned");
    let package = store
        .latest_config_package_for_edge("edge-dev")
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
    ReleaseService::mark_reported(&mut store, release.release_id, release.desired_version);

    Ok(Json(release_list_response(&store)))
}

fn release_list_response(store: &cloud_control::CloudControlStore) -> ReleaseListResponse {
    let mut releases = store.releases().cloned().collect::<Vec<_>>();
    releases.sort_by(|left, right| left.desired_version.cmp(&right.desired_version));
    releases.reverse();

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
    let mut store = state.store.lock().expect("store mutex poisoned");
    let release = ReleaseService::create_release(&mut store, package).map_err(|errors| {
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
    })?;

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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PointMappingResponse {
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

fn format_address(address: &PointAddress) -> String {
    format!("{}:{}", address.kind, address.value)
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
