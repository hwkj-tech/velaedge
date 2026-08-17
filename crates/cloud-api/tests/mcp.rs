use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use cloud_api::{app, ApiAuthConfig, ApiRole, AppState, McpConfig};
use cloud_control::{AgentChangeSetStatus, AuditAction};
use serde_json::{json, Value};
use tower::ServiceExt;

const VIEWER_TOKEN: &str = "viewer-token-for-mcp-integration-tests";
const OPERATOR_TOKEN: &str = "operator-token-for-mcp-integration-tests";

fn test_state() -> AppState {
    let auth = ApiAuthConfig::required(vec![
        (
            "mcp-viewer".to_owned(),
            ApiRole::Viewer,
            VIEWER_TOKEN.to_owned(),
        ),
        (
            "mcp-operator".to_owned(),
            ApiRole::Operator,
            OPERATOR_TOKEN.to_owned(),
        ),
    ])
    .unwrap();
    AppState::default().with_api_auth(auth).with_mcp_config(
        McpConfig::with_allowed_origins(["https://agent.example"])
            .with_scope(["demo-plant"], ["edge-dev"]),
    )
}

fn mcp_request(token: &str, payload: Value) -> Request<Body> {
    Request::post("/mcp")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("origin", "https://agent.example")
        .header("x-velaedge-project-id", "demo-plant")
        .header("x-velaedge-edge-id", "edge-dev")
        .body(Body::from(payload.to_string()))
        .unwrap()
}

async fn body_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn initializes_and_negotiates_the_mcp_protocol() {
    let response = app(test_state())
        .oneshot(mcp_request(
            VIEWER_TOKEN,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": {"name": "integration-test", "version": "1"}
                }
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let payload = body_json(response).await;
    assert_eq!(payload["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(payload["result"]["serverInfo"]["name"], "velaedge");
    assert_eq!(
        payload["result"]["capabilities"]["tools"]["listChanged"],
        false
    );
}

#[tokio::test]
async fn status_reports_effective_controls_and_role_visible_tools() {
    let request = Request::get("/api/mcp/status")
        .header("authorization", format!("Bearer {VIEWER_TOKEN}"))
        .body(Body::empty())
        .unwrap();
    let response = app(test_state()).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let payload = body_json(response).await;
    assert_eq!(payload["enabled"], true);
    assert_eq!(payload["endpoint"], "/mcp");
    assert_eq!(payload["protocolVersion"], "2025-11-25");
    assert_eq!(payload["authentication"]["scheme"], "bearer");
    assert_eq!(payload["controls"]["executionToolsExposed"], false);
    assert_eq!(payload["scope"]["projects"][0], "demo-plant");
    let names = payload["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"runtime.metrics"));
    assert!(!names.contains(&"configuration.change_set.draft"));
}

#[tokio::test]
async fn model_tool_visibility_follows_role_and_never_exposes_execution() {
    let router = app(test_state());
    let list = |token| {
        mcp_request(
            token,
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        )
    };

    let viewer = body_json(router.clone().oneshot(list(VIEWER_TOKEN)).await.unwrap()).await;
    let viewer_names = tool_names(&viewer);
    assert!(viewer_names.contains(&"runtime.metrics"));
    assert!(!viewer_names.contains(&"configuration.change_set.draft"));
    assert!(!viewer_names.contains(&"device.command.draft"));

    let operator = body_json(router.oneshot(list(OPERATOR_TOKEN)).await.unwrap()).await;
    let operator_names = tool_names(&operator);
    assert!(operator_names.contains(&"configuration.change_set.draft"));
    assert!(operator_names.contains(&"device.command.draft"));
    assert!(!operator_names.contains(&"configuration.change_set.apply"));
    assert!(!operator_names.contains(&"device.command.dispatch"));
}

#[tokio::test]
async fn rejects_untrusted_browser_origins() {
    let request = Request::post("/mcp")
        .header("authorization", format!("Bearer {VIEWER_TOKEN}"))
        .header("content-type", "application/json")
        .header("origin", "https://attacker.example")
        .body(Body::from(
            json!({"jsonrpc": "2.0", "id": 3, "method": "ping"}).to_string(),
        ))
        .unwrap();
    let response = app(test_state()).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn read_tool_is_scoped_and_audited() {
    let state = test_state();
    let response = app(state.clone())
        .oneshot(mcp_request(
            VIEWER_TOKEN,
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {"name": "project.list", "arguments": {}}
            }),
        ))
        .await
        .unwrap();
    let payload = body_json(response).await;
    assert_eq!(payload["result"]["isError"], false);
    assert_eq!(payload["result"]["structuredContent"]["count"], 1);
    assert_eq!(
        payload["result"]["structuredContent"]["items"][0]["project"]["projectId"],
        "demo-plant"
    );
    let store = state.store.lock().unwrap();
    let audit = store.audit_records().last().unwrap();
    assert_eq!(audit.action, AuditAction::InvokeAgentTool);
    assert_eq!(audit.actor, "mcp-viewer");
    assert!(audit.target.contains("project.list:succeeded"));
}

#[tokio::test]
async fn change_set_draft_is_persisted_for_review_but_never_applied() {
    let state = test_state();
    assert!(state
        .store
        .lock()
        .unwrap()
        .product_version("pump-collection-uplink", "v1.4.4")
        .is_none());

    let response = app(state.clone())
        .oneshot(mcp_request(
            OPERATOR_TOKEN,
            json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {
                    "name": "configuration.change_set.draft",
                    "arguments": {
                        "projectId": "demo-plant",
                        "productId": "pump-collection-uplink",
                        "baseVersion": "v1.4.3",
                        "targetVersion": "v1.4.4",
                        "title": "Tune pump point set binding",
                        "rationale": "Exercise the review-only MCP path",
                        "patch": {"pointSetIds": ["pump-standard-points"]}
                    }
                }
            }),
        ))
        .await
        .unwrap();
    let payload = body_json(response).await;
    assert_eq!(payload["result"]["isError"], false, "{payload:#}");
    assert_eq!(
        payload["result"]["structuredContent"]["nextAction"],
        "human_review_required"
    );
    let change_set_id = payload["result"]["structuredContent"]["changeSet"]["changeSetId"]
        .as_str()
        .unwrap();

    let store = state.store.lock().unwrap();
    let change_set = store.agent_change_set(change_set_id).unwrap();
    assert_eq!(change_set.created_by, "mcp-operator");
    assert_eq!(
        change_set.status,
        AgentChangeSetStatus::AwaitingConfirmation
    );
    assert!(store
        .product_version("pump-collection-uplink", "v1.4.4")
        .is_none());
}

#[tokio::test]
async fn tool_arguments_cannot_widen_the_header_scope() {
    let state = test_state();
    let response = app(state.clone())
        .oneshot(mcp_request(
            VIEWER_TOKEN,
            json!({
                "jsonrpc": "2.0",
                "id": 6,
                "method": "tools/call",
                "params": {
                    "name": "project.list",
                    "arguments": {"projectId": "energy-demo"}
                }
            }),
        ))
        .await
        .unwrap();
    let payload = body_json(response).await;
    assert_eq!(payload["result"]["isError"], true);
    assert_eq!(
        payload["result"]["structuredContent"]["error"]["code"],
        "tool_execution_failed"
    );
    let store = state.store.lock().unwrap();
    let audit = store.audit_records().last().unwrap();
    assert_eq!(audit.action, AuditAction::InvokeAgentTool);
    assert!(audit.target.contains("project.list:failed"));
}

#[tokio::test]
async fn rate_limit_is_enforced_per_authenticated_principal() {
    let state = AppState::default()
        .with_api_auth(
            ApiAuthConfig::required(vec![(
                "mcp-viewer".to_owned(),
                ApiRole::Viewer,
                VIEWER_TOKEN.to_owned(),
            )])
            .unwrap(),
        )
        .with_mcp_config(
            McpConfig::with_allowed_origins(["https://agent.example"])
                .with_scope(["demo-plant"], ["edge-dev"])
                .with_rate_limit(1),
        );
    let router = app(state);
    let request = || {
        mcp_request(
            VIEWER_TOKEN,
            json!({"jsonrpc": "2.0", "id": 7, "method": "ping"}),
        )
    };
    assert_eq!(
        router.clone().oneshot(request()).await.unwrap().status(),
        StatusCode::OK
    );
    assert_eq!(
        router.oneshot(request()).await.unwrap().status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

fn tool_names(payload: &Value) -> Vec<&str> {
    payload["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect()
}
