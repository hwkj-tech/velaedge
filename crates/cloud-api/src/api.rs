use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use cloud_control::{ReleaseService, ReleaseStatus};
use edge_core::EdgeConfigPackage;
use serde::Serialize;
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
        .route("/api/releases", post(create_release))
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

async fn summary(State(_state): State<AppState>) -> Json<SummaryResponse> {
    Json(SummaryResponse {
        edge_count: 0,
        pending_release_count: 0,
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
