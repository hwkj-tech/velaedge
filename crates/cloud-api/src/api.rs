use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use cloud_control::{ReleaseService, ReleaseStatus};
use edge_core::EdgeConfigPackage;
use serde::Serialize;

use crate::AppState;

#[derive(Serialize)]
pub struct SummaryResponse {
    pub edge_count: usize,
    pub pending_release_count: usize,
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/api/summary", get(summary))
        .route("/api/releases", post(create_release))
        .with_state(state)
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
