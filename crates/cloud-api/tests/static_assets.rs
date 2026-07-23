use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use cloud_api::{app, AppState};
use tower::ServiceExt;

#[tokio::test]
async fn api_still_responds_when_static_console_is_configured() {
    let response = app(AppState::default())
        .oneshot(Request::get("/api/summary").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn serves_built_console_index() {
    let response = app(AppState::default())
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = std::str::from_utf8(&body).unwrap();
    assert!(html.contains("VelaEdge · 边缘智能控制中心"));
}
