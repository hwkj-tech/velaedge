use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use cloud_control::{ReleaseService, ReleaseStatus};
use edge_core::{EdgeConfigPackage, PointAddress, ProtocolType, TelemetryType};
use serde::Serialize;
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
        .route("/api/point-mappings", get(point_mappings))
        .route("/api/releases", get(releases).post(create_release))
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

    for package in store.config_packages() {
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

        for mapping in &package.point_mappings {
            points.push(PointMappingResponse {
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
            });
        }
    }

    Json(points)
}

async fn releases(State(state): State<AppState>) -> Json<ReleaseListResponse> {
    let store = state.store.lock().expect("store mutex poisoned");
    let mut releases = store.releases().cloned().collect::<Vec<_>>();
    releases.sort_by(|left, right| left.desired_version.cmp(&right.desired_version));
    releases.reverse();

    let draft_version = releases
        .first()
        .map(|release| release.desired_version.clone())
        .unwrap_or_else(|| "-".to_string());

    Json(ReleaseListResponse {
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
    })
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
pub struct ErrorResponse {
    pub message: String,
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
