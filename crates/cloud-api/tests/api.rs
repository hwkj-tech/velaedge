use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use cloud_api::{app, AppState};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn summary_endpoint_returns_initial_counts() {
    let response = app(AppState::default())
        .oneshot(Request::get("/api/summary").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn release_endpoint_accepts_valid_edge_config_package() {
    let payload = json!({
        "edge_id": "edge-dev",
        "version": "2026.06.26-001",
        "device_models": [],
        "devices": [{"device_id": "pump-1", "device_type": "pump"}],
        "protocol_connections": [{"connection_id": "sim-main", "protocol": "Simulated", "endpoint": null}],
        "point_mappings": [{
            "point_id": "pressure",
            "device_id": "pump-1",
            "semantic_id": "pressure",
            "protocol_connection_id": "sim-main",
            "address": {"kind": "simulated", "value": "pressure"},
            "value_type": "Float",
            "unit": "MPa",
            "range": null,
            "interval_ms": 1000
        }],
        "collection_tasks": [{
            "task_id": "pump-main",
            "device_id": "pump-1",
            "point_ids": ["pressure"],
            "interval_ms": 1000,
            "enabled": true
        }],
        "algorithms": []
    });

    let response = app(AppState::default())
        .oneshot(
            Request::post("/api/releases")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}
