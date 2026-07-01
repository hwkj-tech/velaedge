use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use cloud_api::{app, AppState};
use cloud_control::{EdgeNode, SqliteCloudStore};
use edge_core::{
    CollectionTask, DeviceInstance, EdgeConfigPackage, MqttUplinkConfig, PointAddress,
    ProtocolConnection, TelemetryPointMapping, TelemetryType,
};
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
async fn data_config_endpoints_create_and_list_edge_configs() {
    let router = app(AppState::default());

    let response = router
        .clone()
        .oneshot(
            Request::post("/api/edges/edge-dev/data-configs")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "configId": "pump_status_extra",
                        "name": "泵运行状态上报",
                        "enabled": true,
                        "deviceId": "pump-1",
                        "protocolConnectionId": "modbus-line-a",
                        "collection": {"periodMs": 1000, "timeoutMs": 800, "retryCount": 2},
                        "points": [{
                            "pointId": "pressure",
                            "semanticId": "pump.pressure",
                            "addressKind": "holding_register",
                            "addressValue": "40001",
                            "valueType": "float32",
                            "unit": "MPa",
                            "jsonField": "pressure"
                        }],
                        "publish": {
                            "sinkId": "velamq-main",
                            "topicTemplate": "factory/{edge_id}/{device_id}/status",
                            "qos": 1,
                            "payload": {"mode": "object", "timestampField": "ts", "includeQuality": true}
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = router
        .oneshot(
            Request::get("/api/edges/edge-dev/data-configs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let configs: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(configs
        .as_array()
        .unwrap()
        .iter()
        .any(|config| config["configId"] == "pump_status_extra"));
}

#[tokio::test]
async fn data_config_endpoint_rejects_unknown_points_and_duplicate_json_fields() {
    let router = app(AppState::default());

    let unknown_point_response = router
        .clone()
        .oneshot(
            Request::post("/api/edges/edge-dev/data-configs")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "configId": "bad_unknown_point",
                        "name": "非法点位上报",
                        "enabled": true,
                        "deviceId": "pump-1",
                        "protocolConnectionId": "modbus-line-a",
                        "collection": {"periodMs": 1000, "timeoutMs": 800, "retryCount": 2},
                        "points": [{
                            "pointId": "not_configured",
                            "semanticId": "pump.not_configured",
                            "addressKind": "holding_register",
                            "addressValue": "49999",
                            "valueType": "float32",
                            "unit": "-",
                            "jsonField": "not_configured"
                        }],
                        "publish": {
                            "sinkId": "velamq-main",
                            "topicTemplate": "factory/{edge_id}/{device_id}/bad",
                            "qos": 1,
                            "payload": {"mode": "object", "timestampField": "ts", "includeQuality": true}
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unknown_point_response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(unknown_point_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        payload["message"],
        "data config point `not_configured` missing"
    );

    let duplicate_json_response = router
        .oneshot(
            Request::post("/api/edges/edge-dev/data-configs")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "configId": "bad_duplicate_json",
                        "name": "重复 JSON 字段上报",
                        "enabled": true,
                        "deviceId": "pump-1",
                        "protocolConnectionId": "modbus-line-a",
                        "collection": {"periodMs": 1000, "timeoutMs": 800, "retryCount": 2},
                        "points": [
                          {
                            "pointId": "pressure",
                            "semanticId": "pump.pressure",
                            "addressKind": "holding_register",
                            "addressValue": "40001",
                            "valueType": "float32",
                            "unit": "MPa",
                            "jsonField": "value"
                          },
                          {
                            "pointId": "running",
                            "semanticId": "pump.running",
                            "addressKind": "coil",
                            "addressValue": "00001",
                            "valueType": "bool",
                            "unit": "-",
                            "jsonField": "value"
                          }
                        ],
                        "publish": {
                            "sinkId": "velamq-main",
                            "topicTemplate": "factory/{edge_id}/{device_id}/bad",
                            "qos": 1,
                            "payload": {"mode": "object", "timestampField": "ts", "includeQuality": true}
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(duplicate_json_response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(duplicate_json_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        payload["message"],
        "data config json field `value` duplicated"
    );
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
    assert_eq!(connections[0]["protocolType"], "ModbusTcp");

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
    assert_eq!(tasks[0]["pointIds"][0], "pressure");
    assert_eq!(tasks[0]["intervalMs"], 1000);

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
    assert_eq!(algorithms[0]["runtime"], "Onnx");
    assert_eq!(algorithms[0]["inputIds"], json!(["pressure", "running"]));

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
async fn edge_node_create_registers_draft_edge_and_empty_config() {
    let router = app(AppState::default());

    let response = router
        .clone()
        .oneshot(
            Request::post("/api/edge-nodes")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "displayName": "一号产线边端",
                        "site": "制造/一号线"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(created["edgeId"], "edge-draft-2");
    assert_eq!(created["displayName"], "一号产线边端");
    assert_eq!(created["site"], "制造/一号线");

    let response = router
        .clone()
        .oneshot(Request::get("/api/edge-nodes").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let edges: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(edges
        .as_array()
        .unwrap()
        .iter()
        .any(|edge| edge["edgeId"] == "edge-draft-2"));

    let response = router
        .oneshot(
            Request::get("/api/edges/edge-draft-2/protocol-connections")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn removed_edge_node_actions_are_not_callable() {
    let router = app(AppState::default());

    let response = router
        .clone()
        .oneshot(
            Request::post("/api/edges/edge-dev/credentials/rotate")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    let response = router
        .oneshot(
            Request::post("/api/edges/edge-dev/maintenance-mode")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn device_model_create_adds_model_draft_to_console_list() {
    let router = app(AppState::default());

    let response = router
        .clone()
        .oneshot(
            Request::post("/api/device-models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let model: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(model["deviceType"], "device-model-draft-2");
    assert_eq!(model["telemetry"][0]["telemetryId"], "status");

    let response = router
        .oneshot(
            Request::get("/api/device-models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let models: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(models
        .as_array()
        .unwrap()
        .iter()
        .any(|model| model["deviceType"] == "device-model-draft-2"));
}

#[tokio::test]
async fn device_model_create_accepts_user_defined_model_fields() {
    let router = app(AppState::default());

    let response = router
        .clone()
        .oneshot(
            Request::post("/api/device-models")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "deviceType": "meter",
                        "version": "v2",
                        "telemetry": [
                            {
                                "telemetryId": "voltage_a",
                                "valueType": "float32",
                                "unit": "V",
                                "range": "0-500",
                                "description": "A 相电压"
                            }
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let model: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(model["deviceType"], "meter");
    assert_eq!(model["version"], "v2");
    assert_eq!(model["telemetry"][0]["telemetryId"], "voltage_a");
    assert_eq!(model["telemetry"][0]["valueType"], "float32");
    assert_eq!(model["telemetry"][0]["unit"], "V");
    assert_eq!(model["telemetry"][0]["range"], "0-500");
    assert_eq!(model["telemetry"][0]["description"], "A 相电压");

    let response = router
        .oneshot(
            Request::get("/api/device-models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let models: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(models
        .as_array()
        .unwrap()
        .iter()
        .any(|model| model["deviceType"] == "meter"));
}

#[tokio::test]
async fn device_model_save_updates_model_and_latest_edge_config() {
    let router = app(AppState::default());

    let response = router
        .clone()
        .oneshot(
            Request::put("/api/device-models/pump")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "version": "v2",
                        "telemetry": [
                            {
                                "telemetryId": "temperature",
                                "valueType": "float32",
                                "unit": "C",
                                "range": "0-120",
                                "description": "泵体温度"
                            }
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let model: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(model["deviceType"], "pump");
    assert_eq!(model["version"], "v2");
    assert_eq!(model["telemetry"][0]["telemetryId"], "temperature");
    assert_eq!(model["telemetry"][0]["unit"], "C");

    let response = router
        .clone()
        .oneshot(
            Request::get("/api/device-models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let models: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let pump = models
        .as_array()
        .unwrap()
        .iter()
        .find(|model| model["deviceType"] == "pump")
        .unwrap();
    assert_eq!(pump["version"], "v2");

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
    let config: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let model = config["package"]["device_models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|model| model["device_type"] == "pump")
        .unwrap();
    assert_eq!(model["version"], "v2");
    assert_eq!(model["telemetry"][0]["id"], "temperature");
}

#[tokio::test]
async fn edge_algorithms_endpoint_returns_selected_edge_algorithms() {
    let response = app(AppState::default())
        .oneshot(
            Request::get("/api/edges/edge-dev/algorithms")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload[0]["edgeId"], "edge-dev");
    assert_eq!(payload[0]["algorithmId"], "pump-anomaly-v1");
    assert_eq!(payload[0]["runtime"], "Onnx");
    assert_eq!(payload[0]["inputIds"], json!(["pressure", "running"]));
    assert_eq!(payload[0]["outputIds"], json!(["pump.anomaly_score"]));
}

#[tokio::test]
async fn draft_create_endpoints_add_config_resources_for_selected_edge() {
    let router = app(AppState::default());

    let point_response = router
        .clone()
        .oneshot(
            Request::post("/api/edges/edge-dev/point-mappings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(point_response.status(), StatusCode::CREATED);
    let body = to_bytes(point_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let point: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(point["pointId"], "point-draft-3");
    assert_eq!(point["address"], "simulated:point-draft-3");

    let task_response = router
        .clone()
        .oneshot(
            Request::post("/api/edges/edge-dev/collection-tasks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(task_response.status(), StatusCode::CREATED);
    let body = to_bytes(task_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let task: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(task["taskId"], "task-draft-2");
    assert_eq!(
        task["pointIds"],
        json!(["pressure", "running", "point-draft-3"])
    );

    let algorithm_response = router
        .clone()
        .oneshot(
            Request::post("/api/edges/edge-dev/algorithms")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(algorithm_response.status(), StatusCode::CREATED);
    let body = to_bytes(algorithm_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let algorithm: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(algorithm["algorithmId"], "algorithm-draft-2");
    assert_eq!(algorithm["runtime"], "Rule");

    let points_response = router
        .oneshot(
            Request::get("/api/edges/edge-dev/point-mappings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(points_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let points: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(points
        .as_array()
        .unwrap()
        .iter()
        .any(|point| point["pointId"] == "point-draft-3"));
}

#[tokio::test]
async fn draft_create_endpoints_accept_user_defined_config_resources() {
    let router = app(AppState::default());

    let point_response = router
        .clone()
        .oneshot(
            Request::post("/api/edges/edge-dev/point-mappings")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "pointId": "temperature",
                        "deviceId": "pump-1",
                        "semanticId": "pump.temperature",
                        "connectionId": "modbus-line-a",
                        "addressKind": "input_register",
                        "addressValue": "30001",
                        "valueType": "float32",
                        "unit": "C",
                        "intervalMs": 2000
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(point_response.status(), StatusCode::CREATED);
    let body = to_bytes(point_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let point: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(point["pointId"], "temperature");
    assert_eq!(point["deviceId"], "pump-1");
    assert_eq!(point["semanticTelemetry"], "pump.temperature");
    assert_eq!(point["connection"], "modbus-line-a");
    assert_eq!(point["address"], "input_register:30001");
    assert_eq!(point["valueType"], "float32");
    assert_eq!(point["unit"], "C");
    assert_eq!(point["interval"], "2000ms");

    let task_response = router
        .clone()
        .oneshot(
            Request::post("/api/edges/edge-dev/collection-tasks")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "taskId": "thermal-task",
                        "deviceId": "pump-1",
                        "pointIds": ["temperature"],
                        "intervalMs": 3000,
                        "enabled": false
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(task_response.status(), StatusCode::CREATED);
    let body = to_bytes(task_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let task: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(task["taskId"], "thermal-task");
    assert_eq!(task["deviceId"], "pump-1");
    assert_eq!(task["pointIds"], json!(["temperature"]));
    assert_eq!(task["intervalMs"], 3000);
    assert_eq!(task["enabled"], false);
    assert_eq!(task["status"], "暂停");

    let algorithm_response = router
        .clone()
        .oneshot(
            Request::post("/api/edges/edge-dev/algorithms")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "algorithmId": "thermal-rule",
                        "version": "1.0.0",
                        "runtime": "Rule",
                        "inputIds": ["temperature"],
                        "outputIds": ["thermal.alert"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(algorithm_response.status(), StatusCode::CREATED);
    let body = to_bytes(algorithm_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let algorithm: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(algorithm["algorithmId"], "thermal-rule");
    assert_eq!(algorithm["version"], "1.0.0");
    assert_eq!(algorithm["runtime"], "Rule");
    assert_eq!(algorithm["inputIds"], json!(["temperature"]));
    assert_eq!(algorithm["outputIds"], json!(["thermal.alert"]));

    let config_response = router
        .oneshot(
            Request::get("/api/edges/edge-dev/desired-config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(config_response.status(), StatusCode::OK);
    let body = to_bytes(config_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let config: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(config["package"]["point_mappings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|point| point["point_id"] == "temperature"));
    assert!(config["package"]["collection_tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| task["task_id"] == "thermal-task"));
    assert!(config["package"]["algorithms"]
        .as_array()
        .unwrap()
        .iter()
        .any(|algorithm| algorithm["id"] == "thermal-rule"));
}

#[tokio::test]
async fn management_action_endpoints_return_computed_results() {
    let router = app(AppState::default());

    let validation_response = router
        .clone()
        .oneshot(
            Request::post("/api/edges/edge-dev/config/validate")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(validation_response.status(), StatusCode::OK);
    let body = to_bytes(validation_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let validation: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(validation["action"], "validate_config");
    assert_eq!(validation["status"], "已通过");
    assert_eq!(validation["details"][0], "协议连接 1 个");

    let diff_response = router
        .clone()
        .oneshot(
            Request::post("/api/edges/edge-dev/releases/diff")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(diff_response.status(), StatusCode::OK);
    let body = to_bytes(diff_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let diff: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(diff["action"], "release_diff");
    assert_eq!(diff["message"], "配置差异摘要已生成");

    let safety_response = router
        .clone()
        .oneshot(
            Request::post("/api/agent/safety-check")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(safety_response.status(), StatusCode::OK);
    let body = to_bytes(safety_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let safety: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(safety["action"], "agent_safety_check");
    assert_eq!(safety["status"], "已通过");

    let suggestions_response = router
        .oneshot(
            Request::post("/api/agent/suggestions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(suggestions_response.status(), StatusCode::OK);
    let body = to_bytes(suggestions_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let suggestions: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(suggestions["action"], "agent_generate_suggestions");
    assert_eq!(suggestions["suggestions"][0]["title"], "点位补全");
}

#[tokio::test]
async fn edge_algorithm_save_updates_selected_edge_draft() {
    let router = app(AppState::default());

    let response = router
        .clone()
        .oneshot(
            Request::put("/api/edges/edge-dev/algorithms/pump-anomaly-v1")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "version": "1.1.0",
                        "algorithmKind": "WindowAggregate",
                        "dsl": {
                            "inputs": [{"alias": "p", "pointId": "pressure"}],
                            "trigger": {"type": "window", "everyMs": 60000},
                            "steps": [{
                                "type": "windowAggregate",
                                "source": "p",
                                "functions": [{"function": "avg", "output": "pressure_avg"}]
                            }],
                            "outputs": [{"name": "pressure_avg", "pointId": "pump.pressure.avg_1m"}],
                            "report": {"mode": "WindowResult", "sink": "velamq-main"}
                        }
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
    assert_eq!(saved["algorithmId"], "pump-anomaly-v1");
    assert_eq!(saved["version"], "1.1.0");
    assert_eq!(saved["algorithmKind"], "WindowAggregate");
    assert_eq!(saved["kind"], "窗口聚合");
    assert_eq!(saved["inputIds"], json!(["pressure"]));
    assert_eq!(saved["outputIds"], json!(["pump.pressure.avg_1m"]));
    assert_eq!(saved["dsl"]["trigger"]["type"], "window");

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
    let config: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(config["desiredVersion"], "2026.06.26-002");
    assert_eq!(config["package"]["algorithms"][0]["version"], "1.1.0");
    assert_eq!(
        config["package"]["algorithms"][0]["kind"],
        "WindowAggregate"
    );
    assert_eq!(
        config["package"]["algorithms"][0]["inputs"],
        json!(["pressure"])
    );
    assert_eq!(
        config["package"]["algorithms"][0]["outputs"],
        json!(["pump.pressure.avg_1m"])
    );
    assert_eq!(
        config["package"]["algorithms"][0]["dsl"]["outputs"][0]["pointId"],
        "pump.pressure.avg_1m"
    );
}

#[tokio::test]
async fn edge_protocol_connections_endpoint_returns_selected_edge_connections() {
    let response = app(AppState::default())
        .oneshot(
            Request::get("/api/edges/edge-dev/protocol-connections")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload[0]["edgeId"], "edge-dev");
    assert_eq!(payload[0]["connectionId"], "modbus-line-a");
    assert_eq!(payload[0]["protocolType"], "ModbusTcp");
    assert_eq!(payload[0]["endpoint"], "10.12.0.20:502");
}

#[tokio::test]
async fn edge_protocol_connection_save_updates_selected_edge_draft() {
    let router = app(AppState::default());

    let response = router
        .clone()
        .oneshot(
            Request::put("/api/edges/edge-dev/protocol-connections/modbus-line-a")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "protocolType": "OpcUa",
                        "endpoint": "opc.tcp://10.12.0.80:4840"
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
    assert_eq!(saved["connectionId"], "modbus-line-a");
    assert_eq!(saved["protocolType"], "OpcUa");
    assert_eq!(saved["protocol"], "OPC UA");
    assert_eq!(saved["endpoint"], "opc.tcp://10.12.0.80:4840");

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
    let config: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(config["desiredVersion"], "2026.06.26-002");
    assert_eq!(
        config["package"]["protocol_connections"][0]["protocol"],
        "OpcUa"
    );
    assert_eq!(
        config["package"]["protocol_connections"][0]["endpoint"],
        "opc.tcp://10.12.0.80:4840"
    );
}

#[tokio::test]
async fn edge_protocol_connection_create_adds_selected_edge_draft() {
    let router = app(AppState::default());

    let response = router
        .clone()
        .oneshot(
            Request::post("/api/edges/edge-dev/protocol-connections")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "protocolType": "ModbusRtu",
                        "endpoint": "/dev/ttyUSB0"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(created["edgeId"], "edge-dev");
    assert_eq!(created["connectionId"], "connection-draft-2");
    assert_eq!(created["protocolType"], "ModbusRtu");
    assert_eq!(created["protocol"], "Modbus RTU");
    assert_eq!(created["endpoint"], "/dev/ttyUSB0");

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
    let config: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(config["desiredVersion"], "2026.06.26-002");
    assert_eq!(
        config["package"]["protocol_connections"][1]["connection_id"],
        "connection-draft-2"
    );
    assert_eq!(
        config["package"]["protocol_connections"][1]["protocol"],
        "ModbusRtu"
    );
}

#[tokio::test]
async fn mqtt_uplink_endpoints_manage_velamq_northbound_config() {
    let router = app(AppState::default());

    let response = router
        .clone()
        .oneshot(
            Request::get("/api/edges/edge-dev/mqtt-uplink")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let initial: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(initial["broker"], "mqtts://velamq.local:8883");
    assert_eq!(initial["sinkId"], "velamq-main");

    let response = router
        .clone()
        .oneshot(
            Request::put("/api/edges/edge-dev/mqtt-uplink")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "sinkId": "velamq-prod",
                        "broker": "mqtts://velamq.prod:8883",
                        "clientId": "edge-dev-runtime-dev",
                        "topicTemplate": "velamq/{edge_id}/{device_id}/telemetry",
                        "qos": 1,
                        "batchSize": 200,
                        "flushIntervalMs": 500
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
    assert_eq!(saved["sinkId"], "velamq-prod");
    assert_eq!(saved["broker"], "mqtts://velamq.prod:8883");

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
    let config: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        config["package"]["mqtt_uplinks"][0]["broker"],
        "mqtts://velamq.prod:8883"
    );
}

#[tokio::test]
async fn sqlite_app_state_backfills_missing_mqtt_uplink_for_legacy_edges() {
    let tempdir = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite://{}", tempdir.path().join("cloud.db").display());
    let sqlite_store = SqliteCloudStore::connect(&database_url).await.unwrap();
    sqlite_store
        .upsert_edge_node(EdgeNode::new("edge-legacy", "历史边端"))
        .await
        .unwrap();
    sqlite_store
        .upsert_config_package(
            EdgeConfigPackage::new("edge-legacy", "2026.06.26-legacy")
                .with_protocol_connection(ProtocolConnection::simulated("sim-main")),
        )
        .await
        .unwrap();

    let state = AppState::with_sqlite(&database_url).await.unwrap();
    let router = app(state);

    let response = router
        .clone()
        .oneshot(
            Request::get("/api/edges/edge-legacy/mqtt-uplink")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let uplink: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(uplink["broker"], "mqtts://velamq.local:8883");
    assert_eq!(uplink["clientId"], "edge-legacy-runtime-dev");

    let response = router
        .oneshot(
            Request::get("/api/edges/edge-legacy/desired-config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let config: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        config["package"]["mqtt_uplinks"][0]["sink_id"],
        "velamq-main"
    );
}

#[tokio::test]
async fn sqlite_app_state_backfills_default_data_config_for_legacy_edges() {
    let tempdir = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite://{}", tempdir.path().join("cloud.db").display());
    let sqlite_store = SqliteCloudStore::connect(&database_url).await.unwrap();
    sqlite_store
        .upsert_edge_node(EdgeNode::new("edge-legacy", "历史边端"))
        .await
        .unwrap();
    sqlite_store
        .upsert_config_package(
            EdgeConfigPackage::new("edge-legacy", "2026.06.26-legacy")
                .with_device(DeviceInstance::new("pump-1", "pump"))
                .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
                .with_mqtt_uplink(MqttUplinkConfig::velamq(
                    "velamq-main",
                    "mqtts://velamq.local:8883",
                    "edge-legacy-runtime-dev",
                ))
                .with_point_mapping(
                    TelemetryPointMapping::new(
                        "pump.pressure",
                        "pump-1",
                        "pump.pressure",
                        "sim-main",
                        PointAddress::simulated("pressure"),
                        TelemetryType::Float,
                    )
                    .with_unit("MPa")
                    .with_interval_ms(1000),
                )
                .with_point_mapping(TelemetryPointMapping::new(
                    "pump-running",
                    "pump-1",
                    "pump.running",
                    "sim-main",
                    PointAddress::simulated("running"),
                    TelemetryType::Boolean,
                ))
                .with_collection_task(CollectionTask::interval(
                    "pump-main",
                    "pump-1",
                    vec!["pump.pressure".to_string(), "pump-running".to_string()],
                    2000,
                )),
        )
        .await
        .unwrap();

    let router = app(AppState::with_sqlite(&database_url).await.unwrap());

    let response = router
        .clone()
        .oneshot(
            Request::get("/api/edges/edge-legacy/data-configs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let data_configs: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(data_configs[0]["configId"], "default_telemetry");
    assert_eq!(data_configs[0]["collection"]["periodMs"], 2000);
    assert_eq!(data_configs[0]["points"][0]["jsonField"], "pump_pressure");
    assert_eq!(data_configs[0]["points"][1]["jsonField"], "pump_running");

    let response = router
        .oneshot(
            Request::get("/api/edges/edge-legacy/desired-config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let config: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        config["package"]["data_configs"][0]["config_id"],
        "default_telemetry"
    );
}

#[tokio::test]
async fn discovery_run_endpoint_returns_agent_mapping_suggestions() {
    let router = app(AppState::default());

    let response = router
        .clone()
        .oneshot(
            Request::post("/api/edges/edge-dev/discovery/run")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "connectionId": "meter-rs485-bus-1",
                        "addressRange": "holding_register:40001-40002"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let report: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(report["jobId"], "discovery-edge-dev-1");
    assert_eq!(report["suggestions"][0]["pointId"], "pump_flow_rate");
    assert_eq!(report["suggestions"][0]["confidence"], 0.82);

    let response = router
        .oneshot(
            Request::get("/api/edges/edge-dev/discovery/suggestions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let suggestions: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(suggestions[0]["semanticId"], "pump.flow_rate");
    assert_eq!(suggestions[0]["evidence"], "数值范围和波动特征符合泵流量");
}

#[tokio::test]
async fn edge_collection_tasks_endpoint_returns_selected_edge_tasks() {
    let response = app(AppState::default())
        .oneshot(
            Request::get("/api/edges/edge-dev/collection-tasks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload[0]["edgeId"], "edge-dev");
    assert_eq!(payload[0]["taskId"], "pump-main");
    assert_eq!(payload[0]["pointIds"], json!(["pressure", "running"]));
    assert_eq!(payload[0]["intervalMs"], 1000);
    assert_eq!(payload[0]["enabled"], true);
}

#[tokio::test]
async fn edge_collection_task_save_updates_selected_edge_draft() {
    let router = app(AppState::default());

    let response = router
        .clone()
        .oneshot(
            Request::put("/api/edges/edge-dev/collection-tasks/pump-main")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "deviceId": "pump-1",
                        "pointIds": ["pressure"],
                        "intervalMs": 2500,
                        "enabled": false
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
    assert_eq!(saved["taskId"], "pump-main");
    assert_eq!(saved["pointIds"], json!(["pressure"]));
    assert_eq!(saved["interval"], "2500ms");
    assert_eq!(saved["status"], "暂停");

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
    let config: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(config["desiredVersion"], "2026.06.26-002");
    assert_eq!(
        config["package"]["collection_tasks"][0]["point_ids"],
        json!(["pressure"])
    );
    assert_eq!(
        config["package"]["collection_tasks"][0]["interval_ms"],
        2500
    );
    assert_eq!(config["package"]["collection_tasks"][0]["enabled"], false);
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
async fn edge_nodes_endpoint_supports_pagination_metadata() {
    let router = app(AppState::default());

    let response = router
        .oneshot(
            Request::get("/api/edge-nodes?page=1&pageSize=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload["page"], 1);
    assert_eq!(payload["pageSize"], 1);
    assert_eq!(payload["total"], 1);
    assert_eq!(payload["items"][0]["edgeId"], "edge-dev");
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
