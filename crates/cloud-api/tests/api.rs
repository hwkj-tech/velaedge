use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use cloud_api::{app, AgentService, ApiAuthConfig, ApiRole, AppState};
use cloud_control::{EdgeNode, SqliteCloudStore};
use edge_core::{
    CollectionTask, DeviceInstance, EdgeConfigPackage, MqttUplinkConfig, PointAddress,
    ProtocolConnection, SerialConnectionSettings, TelemetryPointMapping, TelemetryType,
};
use serde_json::json;
use tower::ServiceExt;

const VIEWER_TOKEN: &str = "viewer-token-for-api-rbac-tests";
const OPERATOR_TOKEN: &str = "operator-token-for-api-rbac-tests";
const SECOND_OPERATOR_TOKEN: &str = "second-operator-token-for-rbac-tests";
const ADMIN_TOKEN: &str = "administrator-token-for-rbac-tests";

fn rbac_router() -> axum::Router {
    let auth = ApiAuthConfig::required(vec![
        (
            "read-only".to_string(),
            ApiRole::Viewer,
            VIEWER_TOKEN.to_string(),
        ),
        (
            "config-operator".to_string(),
            ApiRole::Operator,
            OPERATOR_TOKEN.to_string(),
        ),
        (
            "backup-operator".to_string(),
            ApiRole::Operator,
            SECOND_OPERATOR_TOKEN.to_string(),
        ),
        (
            "platform-admin".to_string(),
            ApiRole::Admin,
            ADMIN_TOKEN.to_string(),
        ),
    ])
    .unwrap();
    app(AppState::default().with_api_auth(auth))
}

#[tokio::test]
async fn health_probes_are_public_and_report_dependency_state() {
    let router = rbac_router();

    let live = router
        .clone()
        .oneshot(Request::get("/health/live").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(live.status(), StatusCode::OK);
    let body = to_bytes(live.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["checks"]["process"], "ok");

    let ready = router
        .clone()
        .oneshot(Request::get("/health/ready").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);
    let body = to_bytes(ready.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["status"], "ready");
    assert_eq!(payload["checks"]["memory"], "ok");
    assert_eq!(payload["checks"]["sqlite"], "not_configured");

    let protected = router
        .oneshot(Request::get("/api/summary").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(protected.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn authenticated_agent_uses_server_principal_for_ownership_and_audit() {
    let router = rbac_router();
    let chat = router
        .clone()
        .oneshot(
            Request::post("/api/agent/chat")
                .header("authorization", bearer(OPERATOR_TOKEN))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "message": "检查当前边端配置风险",
                        "operatorId": "spoofed-operator"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(chat.status(), StatusCode::OK);

    let own_conversations = router
        .clone()
        .oneshot(
            Request::get("/api/agent/conversations?operatorId=spoofed-operator")
                .header("authorization", bearer(OPERATOR_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(own_conversations.status(), StatusCode::OK);
    let body = to_bytes(own_conversations.into_body(), usize::MAX)
        .await
        .unwrap();
    let conversations: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(conversations.as_array().unwrap().len(), 1);
    assert_eq!(conversations[0]["operatorId"], "config-operator");

    let isolated_conversations = router
        .clone()
        .oneshot(
            Request::get("/api/agent/conversations?operatorId=config-operator")
                .header("authorization", bearer(SECOND_OPERATOR_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(isolated_conversations.into_body(), usize::MAX)
        .await
        .unwrap();
    let conversations: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(conversations.as_array().unwrap().is_empty());

    let knowledge = router
        .clone()
        .oneshot(
            Request::post("/api/agent/knowledge")
                .header("authorization", bearer(OPERATOR_TOKEN))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "title": "现场规程",
                        "content": "压力越限时先检查传感器质量。",
                        "tags": ["pressure"],
                        "enabled": true,
                        "actor": "spoofed-author"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(knowledge.status(), StatusCode::CREATED);
    let body = to_bytes(knowledge.into_body(), usize::MAX).await.unwrap();
    let knowledge: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(knowledge["createdBy"], "config-operator");

    let proposal = router
        .clone()
        .oneshot(
            Request::post("/api/agent/proposals")
                .header("authorization", bearer(OPERATOR_TOKEN))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "agentId": "edgeops-agent",
                        "kind": "config_suggestion",
                        "title": "调整采集周期",
                        "summary": "将压力点采集周期调整为 2 秒",
                        "risk": "low",
                        "createdBy": "spoofed-creator"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(proposal.status(), StatusCode::CREATED);
    let body = to_bytes(proposal.into_body(), usize::MAX).await.unwrap();
    let proposal: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(proposal["createdBy"], "config-operator");
    let proposal_id = proposal["proposalId"].as_str().unwrap();

    let reviewed = router
        .oneshot(
            Request::post(format!("/api/agent/proposals/{proposal_id}/approve"))
                .header("authorization", bearer(OPERATOR_TOKEN))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "reviewer": "spoofed-reviewer",
                        "note": "进入人工发布流程"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reviewed.status(), StatusCode::OK);
    let body = to_bytes(reviewed.into_body(), usize::MAX).await.unwrap();
    let reviewed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(reviewed["reviewedBy"], "config-operator");
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

#[tokio::test]
async fn management_api_enforces_authentication_and_role_hierarchy() {
    let router = rbac_router();

    let anonymous = router
        .clone()
        .oneshot(Request::get("/api/summary").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        anonymous.headers()["www-authenticate"],
        "Bearer realm=\"edgeops\""
    );

    let viewer_read = router
        .clone()
        .oneshot(
            Request::get("/api/auth/me")
                .header("authorization", bearer(VIEWER_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(viewer_read.status(), StatusCode::OK);
    let body = to_bytes(viewer_read.into_body(), usize::MAX).await.unwrap();
    let status: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(status["subject"], "read-only");
    assert_eq!(status["role"], "viewer");
    assert_eq!(status["authenticationEnabled"], true);

    let project = json!({
        "projectId": "rbac-acceptance",
        "name": "RBAC acceptance",
        "environment": "test",
        "owner": "platform-team",
        "description": "role hierarchy acceptance fixture"
    });
    let viewer_write = router
        .clone()
        .oneshot(
            Request::post("/api/projects")
                .header("authorization", bearer(VIEWER_TOKEN))
                .header("content-type", "application/json")
                .body(Body::from(project.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(viewer_write.status(), StatusCode::FORBIDDEN);

    let operator_write = router
        .clone()
        .oneshot(
            Request::post("/api/projects")
                .header("authorization", bearer(OPERATOR_TOKEN))
                .header("content-type", "application/json")
                .body(Body::from(project.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(operator_write.status(), StatusCode::CREATED);

    let operator_delete = router
        .clone()
        .oneshot(
            Request::delete("/api/projects/rbac-acceptance")
                .header("authorization", bearer(OPERATOR_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(operator_delete.status(), StatusCode::FORBIDDEN);

    let operator_token_rotation = router
        .clone()
        .oneshot(
            Request::post("/api/edge-nodes/edge-dev/access-token")
                .header("authorization", bearer(OPERATOR_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(operator_token_rotation.status(), StatusCode::FORBIDDEN);

    let admin_delete = router
        .oneshot(
            Request::delete("/api/projects/rbac-acceptance")
                .header("authorization", bearer(ADMIN_TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin_delete.status(), StatusCode::NO_CONTENT);
}

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
                        "visualGraph": {
                            "nodes": [
                                {
                                    "nodeId": "point-pressure",
                                    "kind": "point",
                                    "label": "pressure",
                                    "refId": "pressure",
                                    "x": 72,
                                    "y": 80
                                },
                                {
                                    "nodeId": "mqtt-pressure",
                                    "kind": "mqtt",
                                    "label": "压力主题",
                                    "refId": "factory/{edge_id}/{device_id}/pressure",
                                    "x": 680,
                                    "y": 80
                                },
                                {
                                    "nodeId": "mqtt-status",
                                    "kind": "mqtt",
                                    "label": "状态主题",
                                    "refId": "factory/{edge_id}/{device_id}/status",
                                    "x": 680,
                                    "y": 180
                                }
                            ],
                            "edges": [
                                {
                                    "edgeId": "point-pressure:value-to-mqtt-pressure:payload",
                                    "from": "point-pressure",
                                    "fromPort": "value",
                                    "to": "mqtt-pressure",
                                    "toPort": "payload"
                                },
                                {
                                    "edgeId": "point-pressure:value-to-mqtt-status:payload",
                                    "from": "point-pressure",
                                    "fromPort": "value",
                                    "to": "mqtt-status",
                                    "toPort": "payload"
                                }
                            ]
                        },
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
        .clone()
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
    let saved = configs
        .as_array()
        .unwrap()
        .iter()
        .find(|config| config["configId"] == "pump_status_extra")
        .unwrap();
    assert_eq!(saved["visualGraph"]["nodes"].as_array().unwrap().len(), 3);
    assert_eq!(
        saved["visualGraph"]["nodes"][1]["refId"],
        "factory/{edge_id}/{device_id}/pressure"
    );
    assert_eq!(
        saved["visualGraph"]["nodes"][2]["refId"],
        "factory/{edge_id}/{device_id}/status"
    );

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
    let desired: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let deployed = desired["package"]["data_configs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|config| config["config_id"] == "pump_status_extra")
        .unwrap();
    assert_eq!(
        deployed["visual_graph"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|node| node["kind"] == "Mqtt")
            .count(),
        2
    );
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
        .clone()
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

    let missing_algorithm_input_response = router
        .clone()
        .oneshot(
            Request::post("/api/edges/edge-dev/data-configs")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "configId": "bad_algorithm_input",
                        "name": "算法输入缺失上报",
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
                        "algorithmIds": ["pump-anomaly-v1"],
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
    assert_eq!(
        missing_algorithm_input_response.status(),
        StatusCode::BAD_REQUEST
    );
    let body = to_bytes(missing_algorithm_input_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        payload["message"],
        "data config algorithm `pump-anomaly-v1` input point `running` is not included in data config points"
    );

    let algorithm_response = router
        .clone()
        .oneshot(
            Request::post("/api/edges/edge-dev/algorithms")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "algorithmId": "pressure-field-collision",
                        "version": "1.0.0",
                        "algorithmKind": "ChangeReport",
                        "runtime": "Rule",
                        "dsl": {
                            "inputs": [{"alias": "p", "pointId": "pressure"}],
                            "trigger": {"type": "onSample"},
                            "steps": [{"type": "changeFilter", "source": "p", "threshold": 0.1}],
                            "outputs": [{"name": "pressure", "pointId": "pressure.reported"}],
                            "report": {"mode": "OnChange", "sink": "velamq-main"}
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(algorithm_response.status(), StatusCode::CREATED);

    let algorithm_output_collision_response = router
        .oneshot(
            Request::post("/api/edges/edge-dev/data-configs")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "configId": "bad_algorithm_output_field",
                        "name": "算法输出字段冲突上报",
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
                        "algorithmIds": ["pressure-field-collision"],
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
    assert_eq!(
        algorithm_output_collision_response.status(),
        StatusCode::BAD_REQUEST
    );
    let body = to_bytes(algorithm_output_collision_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        payload["message"],
        "data config algorithm `pressure-field-collision` output json field `pressure` conflicts with another payload field"
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
                        "productId": "pump-collection-uplink",
                        "projectId": "demo-plant",
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
    assert!(created["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|capability| capability == "product:pump-collection-uplink"));
    assert!(created["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|capability| capability == "project:demo-plant"));

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
async fn device_model_delete_removes_unused_models_and_protects_referenced_models() {
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

    let response = router
        .clone()
        .oneshot(
            Request::delete("/api/device-models/device-model-draft-2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = router
        .clone()
        .oneshot(
            Request::get("/api/device-models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let models: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(!models
        .as_array()
        .unwrap()
        .iter()
        .any(|model| model["deviceType"] == "device-model-draft-2"));

    let response = router
        .oneshot(
            Request::delete("/api/device-models/pump")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn edge_node_delete_removes_draft_edges_and_protects_active_edges() {
    let router = app(AppState::default());

    let response = router
        .clone()
        .oneshot(
            Request::post("/api/edge-nodes")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "displayName": "待移除边端",
                        "productId": "pump-collection-uplink",
                        "projectId": "demo-plant",
                        "site": "待分配"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = router
        .clone()
        .oneshot(
            Request::delete("/api/edge-nodes/edge-draft-2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = router
        .clone()
        .oneshot(Request::get("/api/edge-nodes").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let edges: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(!edges
        .as_array()
        .unwrap()
        .iter()
        .any(|edge| edge["edgeId"] == "edge-draft-2"));

    let response = router
        .oneshot(
            Request::delete("/api/edge-nodes/edge-dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
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
async fn algorithm_dsl_accepts_an_upstream_algorithm_output_as_input() {
    let router = app(AppState::default());
    let upstream = json!({
        "algorithmId": "pressure-filter",
        "version": "1.0.0",
        "algorithmKind": "ChangeReport",
        "dsl": {
            "inputs": [{ "alias": "p0", "pointId": "pressure" }],
            "trigger": { "type": "onSample" },
            "steps": [{ "type": "changeFilter", "source": "p0", "threshold": 0.1 }],
            "outputs": [{ "name": "value", "pointId": "pressure.filtered" }],
            "report": { "mode": "OnChange", "sink": "velamq-main" }
        }
    });
    let response = router
        .clone()
        .oneshot(
            Request::post("/api/edges/edge-dev/algorithms")
                .header("content-type", "application/json")
                .body(Body::from(upstream.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let downstream = json!({
        "algorithmId": "pressure-expression",
        "version": "1.0.0",
        "algorithmKind": "ExpressionAggregate",
        "dsl": {
            "inputs": [{ "alias": "p0", "pointId": "pressure.filtered" }],
            "trigger": { "type": "onAnyInput" },
            "steps": [{ "type": "expression", "output": "value", "expr": "p0" }],
            "outputs": [{ "name": "value", "pointId": "pressure.normalized" }],
            "report": { "mode": "OnOutput", "sink": "velamq-main" }
        }
    });
    let response = router
        .oneshot(
            Request::post("/api/edges/edge-dev/algorithms")
                .header("content-type", "application/json")
                .body(Body::from(downstream.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
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
async fn agent_proposal_review_is_persisted_governance_not_release_execution() {
    let state = AppState::default();
    let release_count_before = state
        .store
        .lock()
        .expect("store mutex poisoned")
        .releases()
        .count();
    let router = app(state.clone());

    let created = router
        .clone()
        .oneshot(
            Request::post("/api/agent/proposals")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "agentId": "fleet-agent",
                        "kind": "config_suggestion",
                        "projectId": "demo-plant",
                        "edgeId": "edge-dev",
                        "title": "补全压力点位",
                        "summary": "建议增加 pump.pressure 映射",
                        "payload": {"pointId": "pressure"},
                        "risk": "medium",
                        "createdBy": "operator-a"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let body = to_bytes(created.into_body(), usize::MAX).await.unwrap();
    let proposal: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(proposal["status"], "pending_review");
    let proposal_id = proposal["proposalId"].as_str().unwrap();

    let approved = router
        .clone()
        .oneshot(
            Request::post(format!("/api/agent/proposals/{proposal_id}/approve"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "reviewer": "reviewer-a",
                        "note": "允许进入人工配置流程"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approved.status(), StatusCode::OK);
    let body = to_bytes(approved.into_body(), usize::MAX).await.unwrap();
    let approved: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(approved["status"], "approved");
    assert_eq!(approved["reviewedBy"], "reviewer-a");

    let duplicate_review = router
        .clone()
        .oneshot(
            Request::post(format!("/api/agent/proposals/{proposal_id}/reject"))
                .header("content-type", "application/json")
                .body(Body::from(json!({"reviewer": "reviewer-b"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(duplicate_review.status(), StatusCode::CONFLICT);

    let listed = router
        .oneshot(
            Request::get("/api/agent/proposals")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let body = to_bytes(listed.into_body(), usize::MAX).await.unwrap();
    let proposals: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(proposals.as_array().unwrap().len(), 1);

    let store = state.store.lock().expect("store mutex poisoned");
    assert_eq!(store.releases().count(), release_count_before);
    assert!(store
        .audit_records()
        .iter()
        .any(|record| record.actor == "reviewer-a"));
}

#[tokio::test]
async fn agent_chat_uses_scoped_backend_context_without_execution_side_effects() {
    let state = AppState::default().with_agent_service(AgentService::new(None));
    let release_count_before = state
        .store
        .lock()
        .expect("store mutex poisoned")
        .releases()
        .count();
    let router = app(state.clone());

    let provider = router
        .clone()
        .oneshot(
            Request::get("/api/agent/provider")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(provider.into_body(), usize::MAX).await.unwrap();
    let provider: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(provider["configured"], false);
    assert_eq!(provider["mode"], "deterministic");

    let response = router
        .clone()
        .oneshot(
            Request::post("/api/agent/chat")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "message": "分析 edge-dev 当前发布风险",
                        "projectId": "demo-plant",
                        "edgeId": "edge-dev"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(response["mode"], "deterministic");
    assert!(response["message"].as_str().unwrap().contains("1 个边端"));
    assert!(response["message"]
        .as_str()
        .unwrap()
        .contains("不会自动发布配置"));
    assert_eq!(
        state
            .store
            .lock()
            .expect("store mutex poisoned")
            .releases()
            .count(),
        release_count_before
    );

    let invalid_scope = router
        .oneshot(
            Request::post("/api/agent/chat")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"message": "分析", "edgeId": "missing-edge"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid_scope.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn governed_agent_knowledge_is_scoped_cited_redacted_and_persistent() {
    let tempdir = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite://{}", tempdir.path().join("cloud.db").display());
    let state = AppState::with_sqlite(&database_url)
        .await
        .unwrap()
        .with_agent_service(AgentService::new(None));
    let router = app(state);

    let create_document = |project_id: &str, title: &str, content: &str, enabled: bool| {
        json!({
            "projectId": project_id,
            "title": title,
            "sourceUri": format!("kb://{project_id}/{title}"),
            "content": content,
            "tags": ["运维", "Modbus"],
            "enabled": enabled,
            "actor": "knowledge-admin"
        })
    };

    let demo = router
        .clone()
        .oneshot(
            Request::post("/api/agent/knowledge")
                .header("content-type", "application/json")
                .body(Body::from(
                    create_document(
                        "demo-plant",
                        "泵站压力超时处理",
                        "泵站压力采集超时时，先检查串口参数与从站地址。\npassword=must-not-leak",
                        true,
                    )
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(demo.status(), StatusCode::CREATED);
    let body = to_bytes(demo.into_body(), usize::MAX).await.unwrap();
    let demo: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let demo_id = demo["documentId"].as_str().unwrap().to_string();

    let energy = router
        .clone()
        .oneshot(
            Request::post("/api/agent/knowledge")
                .header("content-type", "application/json")
                .body(Body::from(
                    create_document(
                        "energy-demo",
                        "电表压力术语",
                        "仅属于能源项目的压力说明，不得跨项目召回。",
                        true,
                    )
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(energy.status(), StatusCode::CREATED);

    let disabled = router
        .clone()
        .oneshot(
            Request::post("/api/agent/knowledge")
                .header("content-type", "application/json")
                .body(Body::from(
                    create_document(
                        "demo-plant",
                        "旧版压力手册",
                        "已停用的压力说明不应参与检索。",
                        false,
                    )
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(disabled.status(), StatusCode::CREATED);

    let scoped_list = router
        .clone()
        .oneshot(
            Request::get("/api/agent/knowledge?projectId=demo-plant")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(scoped_list.into_body(), usize::MAX).await.unwrap();
    let scoped_list: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(scoped_list.as_array().unwrap().len(), 2);
    assert!(scoped_list
        .as_array()
        .unwrap()
        .iter()
        .all(|document| document["projectId"] == "demo-plant"));

    let chat = router
        .clone()
        .oneshot(
            Request::post("/api/agent/chat")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "message": "泵站压力采集超时如何处理",
                        "projectId": "demo-plant",
                        "edgeId": "edge-dev"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(chat.status(), StatusCode::OK);
    let body = to_bytes(chat.into_body(), usize::MAX).await.unwrap();
    let chat: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(chat["citations"].as_array().unwrap().len(), 1);
    assert_eq!(chat["citations"][0]["documentId"], demo_id);
    assert_eq!(chat["citations"][0]["title"], "泵站压力超时处理");
    assert!(!chat["citations"][0]["excerpt"]
        .as_str()
        .unwrap()
        .contains("password"));
    assert!(chat["message"]
        .as_str()
        .unwrap()
        .contains("命中 1 条受管知识"));

    let reopened = app(AppState::with_sqlite(&database_url)
        .await
        .unwrap()
        .with_agent_service(AgentService::new(None)));
    let restored = reopened
        .clone()
        .oneshot(
            Request::get("/api/agent/knowledge?projectId=demo-plant")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(restored.into_body(), usize::MAX).await.unwrap();
    let restored: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(restored
        .as_array()
        .unwrap()
        .iter()
        .any(|document| document["documentId"] == demo_id));

    let deleted = reopened
        .clone()
        .oneshot(
            Request::delete(format!(
                "/api/agent/knowledge/{demo_id}?actor=knowledge-admin"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let audits = reopened
        .oneshot(
            Request::get("/api/audit-records")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(audits.into_body(), usize::MAX).await.unwrap();
    let audits: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(audits.as_array().unwrap().iter().any(|record| {
        record["action"] == "delete_knowledge_document" && record["actor"] == "knowledge-admin"
    }));
}

#[tokio::test]
async fn agent_conversations_are_operator_scoped_persistent_and_audited() {
    let tempdir = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite://{}", tempdir.path().join("cloud.db").display());
    let router = app(AppState::with_sqlite(&database_url)
        .await
        .unwrap()
        .with_agent_service(AgentService::new(None)));

    let first = router
        .clone()
        .oneshot(
            Request::post("/api/agent/chat")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "message": "检查当前发布风险",
                        "projectId": "demo-plant",
                        "operatorId": "operator-a"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let body = to_bytes(first.into_body(), usize::MAX).await.unwrap();
    let first: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let conversation_id = first["conversationId"].as_str().unwrap().to_string();
    assert_eq!(first["conversationTitle"], "检查当前发布风险");

    let continued = router
        .clone()
        .oneshot(
            Request::post("/api/agent/chat")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "message": "继续说明差异",
                        "conversationId": conversation_id.clone(),
                        "operatorId": "operator-a"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(continued.status(), StatusCode::OK);

    let owned = router
        .clone()
        .oneshot(
            Request::get("/api/agent/conversations?operatorId=operator-a&projectId=demo-plant")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(owned.into_body(), usize::MAX).await.unwrap();
    let owned: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(owned.as_array().unwrap().len(), 1);
    assert_eq!(owned[0]["messages"].as_array().unwrap().len(), 4);
    assert_eq!(owned[0]["operatorId"], "operator-a");

    let hidden = router
        .clone()
        .oneshot(
            Request::get("/api/agent/conversations?operatorId=operator-b&projectId=demo-plant")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(hidden.into_body(), usize::MAX).await.unwrap();
    let hidden: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(hidden.as_array().unwrap().is_empty());

    let reopened = app(AppState::with_sqlite(&database_url)
        .await
        .unwrap()
        .with_agent_service(AgentService::new(None)));
    let restored = reopened
        .clone()
        .oneshot(
            Request::get(format!(
                "/api/agent/conversations/{conversation_id}?operatorId=operator-a"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(restored.status(), StatusCode::OK);
    let body = to_bytes(restored.into_body(), usize::MAX).await.unwrap();
    let restored: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(restored["messages"].as_array().unwrap().len(), 4);

    let forbidden_delete = reopened
        .clone()
        .oneshot(
            Request::delete(format!(
                "/api/agent/conversations/{conversation_id}?operatorId=operator-b"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(forbidden_delete.status(), StatusCode::NOT_FOUND);

    let deleted = reopened
        .clone()
        .oneshot(
            Request::delete(format!(
                "/api/agent/conversations/{conversation_id}?operatorId=operator-a"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let audits = reopened
        .oneshot(
            Request::get("/api/audit-records")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(audits.into_body(), usize::MAX).await.unwrap();
    let audits: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(audits.as_array().unwrap().iter().any(|record| {
        record["action"] == "create_agent_conversation" && record["actor"] == "operator-a"
    }));
    assert!(audits.as_array().unwrap().iter().any(|record| {
        record["action"] == "delete_agent_conversation" && record["actor"] == "operator-a"
    }));
}

#[tokio::test]
async fn sqlite_app_state_restores_agent_proposal_and_review_audit() {
    let tempdir = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite://{}", tempdir.path().join("cloud.db").display());
    let router = app(AppState::with_sqlite(&database_url).await.unwrap());
    let created = router
        .clone()
        .oneshot(
            Request::post("/api/agent/proposals")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "agentId": "fleet-agent",
                        "kind": "rollout_plan",
                        "projectId": "demo-plant",
                        "title": "灰度发布建议",
                        "summary": "先发布单台边端",
                        "risk": "medium",
                        "createdBy": "operator-a"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(created.into_body(), usize::MAX).await.unwrap();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let proposal_id = created["proposalId"].as_str().unwrap();
    let reviewed = router
        .oneshot(
            Request::post(format!("/api/agent/proposals/{proposal_id}/reject"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"reviewer": "reviewer-a", "note": "等待维护窗口"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reviewed.status(), StatusCode::OK);

    let reopened = app(AppState::with_sqlite(&database_url).await.unwrap());
    let proposals = reopened
        .clone()
        .oneshot(
            Request::get("/api/agent/proposals")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(proposals.into_body(), usize::MAX).await.unwrap();
    let proposals: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let restored = proposals
        .as_array()
        .unwrap()
        .iter()
        .find(|proposal| proposal["proposalId"] == proposal_id)
        .unwrap();
    assert_eq!(restored["status"], "rejected");
    assert_eq!(restored["reviewedBy"], "reviewer-a");

    let audits = reopened
        .oneshot(
            Request::get("/api/audit-records")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(audits.into_body(), usize::MAX).await.unwrap();
    let audits: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(audits.as_array().unwrap().iter().any(|record| {
        record["action"] == "reject_agent_proposal" && record["actor"] == "reviewer-a"
    }));
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
                        "protocolType": "Iec101",
                        "endpoint": "/dev/ttyUSB0",
                        "serial": {
                            "port": "/dev/ttyUSB0",
                            "baudRate": 19200,
                            "dataBits": 8,
                            "stopBits": 1,
                            "parity": "even"
                        }
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
    assert_eq!(created["protocolType"], "Iec101");
    assert_eq!(created["protocol"], "IEC-101");
    assert_eq!(created["endpoint"], "/dev/ttyUSB0");
    assert_eq!(created["serial"]["baudRate"], 19200);
    assert_eq!(created["serial"]["parity"], "even");
    assert_eq!(created["policy"], "19200 baud · 8E1");

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
        "Iec101"
    );
    assert_eq!(
        config["package"]["protocol_connections"][1]["serial"]["baud_rate"],
        19200
    );
    assert_eq!(
        config["package"]["protocol_connections"][1]["serial"]["parity"],
        "even"
    );
}

#[tokio::test]
async fn edge_protocol_connection_rejects_invalid_serial_settings() {
    let response = app(AppState::default())
        .oneshot(
            Request::post("/api/edges/edge-dev/protocol-connections")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "protocolType": "Iec101",
                        "endpoint": "/dev/ttyUSB0",
                        "serial": {
                            "port": "/dev/ttyUSB0",
                            "baudRate": 9600,
                            "dataBits": 8,
                            "stopBits": 3,
                            "parity": "even"
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["message"], "stopBits must be 1 or 2");
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
                        "username": "edge-device",
                        "passwordEnv": "EDGEOPS_MQTT_PASSWORD",
                        "tlsCaPath": "/etc/edgeops/velamq-ca.pem",
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
    assert_eq!(saved["username"], "edge-device");
    assert_eq!(saved["passwordEnv"], "EDGEOPS_MQTT_PASSWORD");
    assert_eq!(saved["tlsCaPath"], "/etc/edgeops/velamq-ca.pem");

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
    assert_eq!(
        config["package"]["mqtt_uplinks"][0]["password_env"],
        "EDGEOPS_MQTT_PASSWORD"
    );
}

#[tokio::test]
async fn mqtt_uplink_endpoint_rejects_partial_secret_references() {
    let response = app(AppState::default())
        .oneshot(
            Request::put("/api/edges/edge-dev/mqtt-uplink")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "sinkId": "velamq-main",
                        "broker": "mqtts://velamq.prod:8883",
                        "clientId": "edge-dev-runtime-dev",
                        "username": "edge-device",
                        "topicTemplate": "edge/{edge_id}/telemetry",
                        "qos": 1,
                        "batchSize": 100,
                        "flushIntervalMs": 1000
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
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
async fn discovery_run_endpoint_does_not_invent_results_without_runtime_channel() {
    let state = AppState::default();
    {
        let mut store = state.store.lock().unwrap();
        let mut package = store
            .latest_config_package_for_edge("edge-dev")
            .unwrap()
            .clone();
        package
            .protocol_connections
            .push(ProtocolConnection::modbus_rtu_serial(
                "meter-rs485-bus-1",
                SerialConnectionSettings::new("/dev/ttyUSB0", 9600),
            ));
        store.upsert_config_package(package);
    }
    let router = app(state);

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
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let error: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(error["message"]
        .as_str()
        .unwrap()
        .contains("runtime is not connected"));

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
    assert_eq!(suggestions, json!([]));
}

#[tokio::test]
async fn discovery_run_rejects_unbounded_ranges_before_runtime_dispatch() {
    let response = app(AppState::default())
        .oneshot(
            Request::post("/api/edges/edge-dev/discovery/run")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "connectionId": "meter-rs485-bus-1",
                        "addressRange": "holding_register:40001-40200"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
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
async fn edge_config_delete_endpoints_remove_resources_and_protect_references() {
    let router = app(AppState::default());

    let response = router
        .clone()
        .oneshot(
            Request::delete("/api/edges/edge-dev/collection-tasks/pump-main")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = router
        .clone()
        .oneshot(
            Request::get("/api/edges/edge-dev/collection-tasks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let tasks: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(!tasks
        .as_array()
        .unwrap()
        .iter()
        .any(|task| task["taskId"] == "pump-main"));

    let response = router
        .oneshot(
            Request::delete("/api/edges/edge-dev/protocol-connections/modbus-line-a")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
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
