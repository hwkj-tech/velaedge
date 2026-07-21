use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{extract::State, http::HeaderMap, routing::post, Json, Router};
use cloud_api::{AgentModelConfig, AgentService};
use serde_json::{json, Value};

#[tokio::test]
async fn openai_compatible_provider_receives_bounded_advisory_context() {
    let captured = Arc::new(Mutex::new(None::<(HeaderMap, Value)>));
    let app = Router::new()
        .route(
            "/v1/chat/completions",
            post(
                |State(captured): State<Arc<Mutex<Option<(HeaderMap, Value)>>>>,
                 headers: HeaderMap,
                 Json(body): Json<Value>| async move {
                    *captured.lock().unwrap() = Some((headers, body));
                    Json(json!({
                        "choices": [{
                            "message": {"content": "观察：边端健康。建议：保存草案后人工审核。"}
                        }]
                    }))
                },
            ),
        )
        .with_state(captured.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let service = AgentService::new(Some(AgentModelConfig {
        endpoint: format!("http://{address}/v1/chat/completions"),
        api_key: Some("test-agent-key".to_string()),
        model: "test-model".to_string(),
        timeout: Duration::from_secs(2),
    }));
    let result = service
        .chat(
            "分析 edge-dev 状态",
            &json!({
                "contextVersion": "edgeops-agent-context/v1",
                "fleet": {"edgeCount": 1},
                "governance": {"pendingReleaseCount": 0, "pendingProposalCount": 1},
                "knowledge": [{
                    "documentId": "knowledge-1",
                    "title": "Modbus 运维手册",
                    "sourceUri": "kb://manual/modbus",
                    "excerpt": "超时后检查串口参数。"
                }]
            }),
        )
        .await
        .unwrap();
    assert_eq!(result.model, "test-model");
    assert_eq!(result.message, "观察：边端健康。建议：保存草案后人工审核。");
    assert_eq!(result.citations.len(), 1);
    assert_eq!(result.citations[0].title, "Modbus 运维手册");

    let (headers, body) = captured.lock().unwrap().take().unwrap();
    assert_eq!(
        headers.get("authorization").unwrap(),
        "Bearer test-agent-key"
    );
    assert_eq!(body["model"], "test-model");
    let system = body["messages"][0]["content"].as_str().unwrap();
    assert!(system.contains("must never claim"));
    let user = body["messages"][1]["content"].as_str().unwrap();
    assert!(user.contains("edgeops-agent-context/v1"));
    assert!(user.contains("Modbus 运维手册"));
    assert!(user.contains("分析 edge-dev 状态"));

    server.abort();
}
