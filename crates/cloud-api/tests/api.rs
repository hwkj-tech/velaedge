use axum::{
    body::{to_bytes, Body},
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

#[tokio::test]
async fn point_mappings_endpoint_returns_seeded_config_points() {
    let response = app(AppState::default())
        .oneshot(
            Request::get("/api/point-mappings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload[0]["pointId"], "pressure");
    assert_eq!(payload[0]["edgeId"], "edge-dev");
    assert_eq!(payload[0]["address"], "holding_register:40001");
}

#[tokio::test]
async fn edge_point_mappings_endpoint_returns_selected_edge_points() {
    let response = app(AppState::default())
        .oneshot(
            Request::get("/api/edges/edge-dev/point-mappings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload[0]["edgeId"], "edge-dev");
    assert_eq!(payload[0]["pointId"], "pressure");
    assert_eq!(payload[0]["address"], "holding_register:40001");
}

#[tokio::test]
async fn edge_point_mapping_save_updates_selected_edge_draft() {
    let router = app(AppState::default());

    let response = router
        .clone()
        .oneshot(
            Request::put("/api/edges/edge-dev/point-mappings/running")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "addressKind": "coil",
                        "addressValue": "00009",
                        "intervalMs": 1500,
                        "unit": "-"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(saved["edgeId"], "edge-dev");
    assert_eq!(saved["pointId"], "running");
    assert_eq!(saved["address"], "coil:00009");
    assert_eq!(saved["interval"], "1500ms");

    let response = router
        .oneshot(
            Request::get("/api/edges/edge-dev/point-mappings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let points: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let running = points
        .as_array()
        .unwrap()
        .iter()
        .find(|point| point["pointId"] == "running")
        .unwrap();
    assert_eq!(running["address"], "coil:00009");
    assert_eq!(running["interval"], "1500ms");
}

#[tokio::test]
async fn releases_endpoint_returns_seeded_apply_results() {
    let response = app(AppState::default())
        .oneshot(Request::get("/api/releases").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload["draftVersion"], "2026.06.26-001");
    assert_eq!(payload["applyResults"][0]["edgeId"], "edge-dev");
}

#[tokio::test]
async fn management_endpoints_return_seeded_control_plane_data() {
    let router = app(AppState::default());

    let edge_response = router
        .clone()
        .oneshot(Request::get("/api/edge-nodes").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(edge_response.status(), StatusCode::OK);
    let body = to_bytes(edge_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let edges: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(edges[0]["edgeId"], "edge-dev");
    assert_eq!(edges[0]["status"], "健康");

    let models_response = router
        .clone()
        .oneshot(
            Request::get("/api/device-models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(models_response.status(), StatusCode::OK);
    let body = to_bytes(models_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let models: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(models[0]["deviceType"], "pump");
    assert_eq!(models[0]["telemetry"][0]["telemetryId"], "pressure");

    let protocol_response = router
        .clone()
        .oneshot(
            Request::get("/api/protocol-connections")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(protocol_response.status(), StatusCode::OK);
    let body = to_bytes(protocol_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let connections: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(connections[0]["connectionId"], "modbus-line-a");
    assert_eq!(connections[0]["protocol"], "Modbus TCP");

    let tasks_response = router
        .clone()
        .oneshot(
            Request::get("/api/collection-tasks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tasks_response.status(), StatusCode::OK);
    let body = to_bytes(tasks_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let tasks: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(tasks[0]["taskId"], "pump-main");
    assert_eq!(tasks[0]["pointList"], "pressure, running");

    let algorithms_response = router
        .clone()
        .oneshot(Request::get("/api/algorithms").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(algorithms_response.status(), StatusCode::OK);
    let body = to_bytes(algorithms_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let algorithms: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(algorithms[0]["algorithmId"], "pump-anomaly-v1");
    assert_eq!(algorithms[0]["execution"], "边端本地执行");

    let audit_response = router
        .oneshot(
            Request::get("/api/audit-records")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(audit_response.status(), StatusCode::OK);
    let body = to_bytes(audit_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let audit_records: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(audit_records[0]["actor"], "system");
    assert!(audit_records[0]["action"].is_string());
}

#[tokio::test]
async fn runtime_status_endpoint_returns_seeded_edge_metrics() {
    let response = app(AppState::default())
        .oneshot(
            Request::get("/api/runtime-status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload["healthyEdgeCount"], 1);
    assert_eq!(payload["edges"][0]["edge_id"], "edge-dev");
    assert_eq!(payload["edges"][0]["collection"]["average_latency_ms"], 24);
}

#[tokio::test]
async fn runtime_metrics_endpoint_accepts_edge_snapshot_and_returns_status() {
    let router = app(AppState::default());
    let snapshot = json!({
        "edge_id": "edge-dev",
        "runtime_id": "runtime-dev",
        "config_version": "2026.06.26-002",
        "timestamp": "2026-06-26T10:00:00Z",
        "health": "Degraded",
        "system": {
            "cpu_percent": 72.5,
            "memory_percent": 68.0,
            "disk_percent": 64.0,
            "process_uptime_seconds": 7200
        },
        "collection": {
            "active_task_count": 2,
            "success_rate": 0.982,
            "average_latency_ms": 41,
            "bad_point_count": 1
        },
        "protocols": [{
            "connection_id": "modbus-line-a",
            "protocol": "ModbusTcp",
            "connected": true,
            "latency_ms": 18,
            "timeout_count": 3,
            "error_count": 1,
            "reconnect_count": 0
        }],
        "local_store": {
            "backend": "jsonl",
            "buffered_records": 12,
            "oldest_buffer_age_seconds": 9,
            "disk_usage_percent": 36.0
        },
        "algorithms": [],
        "cloud_sync": {
            "connected": true,
            "last_sync_seconds_ago": 5,
            "pending_uploads": 2,
            "desired_version": "2026.06.26-002",
            "reported_version": "2026.06.26-002"
        }
    });

    let response = router
        .oneshot(
            Request::post("/api/edges/edge-dev/runtime-metrics")
                .header("content-type", "application/json")
                .body(Body::from(snapshot.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload["healthyEdgeCount"], 0);
    assert_eq!(payload["degradedEdgeCount"], 1);
    assert_eq!(payload["averageCollectionLatencyMs"], 41);
    assert_eq!(payload["edges"][0]["health"], "Degraded");
    assert_eq!(payload["edges"][0]["system"]["cpu_percent"], 72.5);
}

#[tokio::test]
async fn runtime_events_endpoint_accepts_edge_event_and_returns_status() {
    let router = app(AppState::default());
    let event = json!({
        "edge_id": "edge-dev",
        "severity": "Warning",
        "category": "Protocol",
        "code": "modbus.timeout",
        "message": "Modbus TCP read timeout",
        "timestamp": "2026-06-26T10:01:00Z",
        "context": {
            "connection_id": "modbus-line-a"
        }
    });

    let response = router
        .oneshot(
            Request::post("/api/edges/edge-dev/runtime-events")
                .header("content-type", "application/json")
                .body(Body::from(event.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload["events"][0]["code"], "modbus.timeout");
    assert_eq!(
        payload["events"][0]["context"]["connection_id"],
        "modbus-line-a"
    );
}

#[tokio::test]
async fn point_mapping_update_saves_new_draft_version() {
    let router = app(AppState::default());
    let payload = json!({
        "addressKind": "holding_register",
        "addressValue": "40002",
        "intervalMs": 2000,
        "unit": "MPa"
    });

    let response = router
        .clone()
        .oneshot(
            Request::put("/api/point-mappings/pressure")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let response = router
        .clone()
        .oneshot(
            Request::get("/api/point-mappings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload[0]["address"], "holding_register:40002");
    assert_eq!(payload[0]["interval"], "2000ms");

    let response = router
        .oneshot(Request::get("/api/releases").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload["draftVersion"], "2026.06.26-002");
}

#[tokio::test]
async fn sqlite_app_state_persists_point_mapping_update_across_reopen() {
    let tempdir = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite://{}", tempdir.path().join("cloud.db").display());

    let router = app(AppState::with_sqlite(&database_url).await.unwrap());
    let update = json!({
        "addressKind": "holding_register",
        "addressValue": "40033",
        "intervalMs": 3000,
        "unit": "MPa"
    });

    let update_response = router
        .oneshot(
            Request::put("/api/point-mappings/pressure")
                .header("content-type", "application/json")
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update_response.status(), StatusCode::OK);

    let reopened = app(AppState::with_sqlite(&database_url).await.unwrap());
    let response = reopened
        .oneshot(
            Request::get("/api/point-mappings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload[0]["address"], "holding_register:40033");
    assert_eq!(payload[0]["interval"], "3000ms");
}

#[tokio::test]
async fn publish_endpoint_releases_latest_draft_as_pending_runtime_deploy() {
    let router = app(AppState::default());
    let update = json!({
        "addressKind": "holding_register",
        "addressValue": "40002",
        "intervalMs": 2000,
        "unit": "MPa"
    });

    let update_response = router
        .clone()
        .oneshot(
            Request::put("/api/point-mappings/pressure")
                .header("content-type", "application/json")
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update_response.status(), StatusCode::OK);

    let publish_response = router
        .oneshot(
            Request::post("/api/releases/publish")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(publish_response.status(), StatusCode::OK);

    let body = to_bytes(publish_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload["draftVersion"], "2026.06.26-002");
    assert_eq!(payload["applyResults"][0]["result"], "等待下发");
    assert_eq!(payload["applyResults"][0]["reportedVersion"], "-");
}

#[tokio::test]
async fn edge_scoped_publish_endpoint_releases_selected_edge_draft() {
    let router = app(AppState::default());

    let publish_response = router
        .oneshot(
            Request::post("/api/edges/edge-dev/releases/publish")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(publish_response.status(), StatusCode::OK);

    let body = to_bytes(publish_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload["applyResults"][0]["edgeId"], "edge-dev");
    assert_eq!(payload["applyResults"][0]["result"], "等待下发");
}

#[tokio::test]
async fn edge_desired_config_endpoint_returns_latest_package() {
    let router = app(AppState::default());
    let update = json!({
        "addressKind": "holding_register",
        "addressValue": "40002",
        "intervalMs": 2000,
        "unit": "MPa"
    });

    let update_response = router
        .clone()
        .oneshot(
            Request::put("/api/point-mappings/pressure")
                .header("content-type", "application/json")
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update_response.status(), StatusCode::OK);

    let response = router
        .oneshot(
            Request::get("/api/edges/edge-dev/desired-config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload["edgeId"], "edge-dev");
    assert_eq!(payload["desiredVersion"], "2026.06.26-002");
    assert_eq!(payload["package"]["version"], "2026.06.26-002");
    assert_eq!(
        payload["package"]["point_mappings"][0]["address"]["value"],
        "40002"
    );
}

#[tokio::test]
async fn edge_reported_config_endpoint_marks_release_applied() {
    let router = app(AppState::default());
    let package = json!({
        "edge_id": "edge-dev",
        "version": "2026.06.26-010",
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

    let release_response = router
        .clone()
        .oneshot(
            Request::post("/api/releases")
                .header("content-type", "application/json")
                .body(Body::from(package.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(release_response.status(), StatusCode::CREATED);

    let report = json!({
        "reportedVersion": "2026.06.26-010"
    });
    let response = router
        .oneshot(
            Request::post("/api/edges/edge-dev/reported-config")
                .header("content-type", "application/json")
                .body(Body::from(report.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload["applyResults"][0]["edgeId"], "edge-dev");
    assert_eq!(
        payload["applyResults"][0]["desiredVersion"],
        "2026.06.26-010"
    );
    assert_eq!(
        payload["applyResults"][0]["reportedVersion"],
        "2026.06.26-010"
    );
    assert_eq!(payload["applyResults"][0]["result"], "已应用");
}
