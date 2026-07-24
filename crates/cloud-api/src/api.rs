use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post, put},
    Extension, Json, Router,
};
use chrono::Utc;
use cloud_control::{
    AgentConversation, AgentConversationCitation, AgentConversationMessage, AgentConversationRole,
    AgentProposal, AgentProposalKind, AgentProposalReviewError, AgentProposalRisk,
    AgentProposalStatus, AuditAction, AuditRecord, EdgeAccessCredential, EdgeNode,
    KnowledgeDocument, PointSet, PointSetPoint, Product, ProductVersion, ProductVersionStatus,
    Project, ReleaseService, ReleaseStatus,
};
use edge_core::{
    validate_custom_serial_point_spec, validate_data_config_visual_graph, AlgorithmDsl,
    AlgorithmInputBinding, AlgorithmKind, AlgorithmOutput, AlgorithmReportMode,
    AlgorithmReportPolicy, AlgorithmRuntime, AlgorithmSpec, AlgorithmStep, AlgorithmTrigger,
    CollectionTask, CustomSerialPointSpec, DataConfig, DataConfigCollection, DataConfigGraphEdge,
    DataConfigGraphNode, DataConfigGraphNodeKind, DataConfigPayload, DataConfigPayloadMode,
    DataConfigPoint, DataConfigPublish, DataConfigVisualGraph, DeviceSpec, DiscoveredPoint,
    DiscoveryReport, DiscoveryRequest, EdgeConfigPackage, EdgeHealth, EdgeRuntimeEvent,
    EdgeRuntimeMetricsSnapshot, MqttUplinkConfig, NumberRange, PointAddress,
    PointMappingSuggestion, ProtocolConnection, ProtocolType, SerialConnectionSettings,
    TelemetryPoint, TelemetryPointMapping, TelemetryType,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use tokio::time::Duration;
use tower_http::services::{ServeDir, ServeFile};

use crate::{
    auth::{auth_status, authorize_api_request, ApiPrincipal},
    gateway::EdgeGatewayDispatchError,
    AppState,
};

type ApiError = (StatusCode, Json<ErrorResponse>);
type ConnectionTransport = (Option<String>, Option<SerialConnectionSettings>);

#[derive(Serialize)]
pub struct SummaryResponse {
    pub edge_count: usize,
    pub pending_release_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
    pub checks: BTreeMap<&'static str, &'static str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProjectRequest {
    pub project_id: String,
    pub name: String,
    pub environment: String,
    pub owner: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePointSetRequest {
    pub point_set_id: String,
    pub project_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub protocol: ProtocolType,
    #[serde(default)]
    pub points: Vec<PointSetPoint>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProductRequest {
    pub product_id: String,
    pub project_id: String,
    pub name: String,
    pub product_type: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProductVersionRequest {
    pub version: String,
    #[serde(default)]
    pub point_set_ids: Vec<String>,
    #[serde(default)]
    pub device_models: Vec<DeviceSpec>,
    #[serde(default)]
    pub devices: Vec<edge_core::DeviceInstance>,
    #[serde(default)]
    pub protocol_connections: Vec<ProtocolConnection>,
    #[serde(default)]
    pub collection_tasks: Vec<CollectionTask>,
    #[serde(default)]
    pub algorithms: Vec<AlgorithmSpec>,
    #[serde(default)]
    pub data_configs: Vec<DataConfig>,
    #[serde(default)]
    pub mqtt_uplinks: Vec<MqttUplinkConfig>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAgentProposalRequest {
    pub agent_id: String,
    pub kind: AgentProposalKind,
    pub project_id: Option<String>,
    pub edge_id: Option<String>,
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub payload: serde_json::Value,
    pub risk: AgentProposalRisk,
    pub created_by: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewAgentProposalRequest {
    pub reviewer: String,
    pub note: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentChatRequest {
    pub message: String,
    pub project_id: Option<String>,
    pub edge_id: Option<String>,
    pub conversation_id: Option<uuid::Uuid>,
    #[serde(default = "default_agent_operator")]
    pub operator_id: String,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConversationQuery {
    pub project_id: Option<String>,
    pub operator_id: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAgentConversationQuery {
    pub operator_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveKnowledgeDocumentRequest {
    pub project_id: Option<String>,
    pub title: String,
    pub source_uri: Option<String>,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub actor: String,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentQuery {
    pub project_id: Option<String>,
}

#[derive(Default, Deserialize)]
pub struct DeleteKnowledgeDocumentQuery {
    pub actor: Option<String>,
}

pub fn app(state: AppState) -> Router {
    let auth = state.api_auth.clone();
    let api = Router::new()
        .route("/api/auth/me", get(auth_status))
        .route("/api/summary", get(summary))
        .route("/api/projects", get(projects).post(create_project))
        .route(
            "/api/projects/{project_id}",
            put(save_project).delete(delete_project),
        )
        .route("/api/point-sets", get(point_sets).post(create_point_set))
        .route(
            "/api/point-sets/{point_set_id}",
            put(save_point_set).delete(delete_point_set),
        )
        .route("/api/products", get(products).post(create_product))
        .route(
            "/api/products/{product_id}",
            put(save_product).delete(delete_product),
        )
        .route(
            "/api/products/{product_id}/versions",
            get(product_versions).post(create_product_version),
        )
        .route(
            "/api/products/{product_id}/versions/{version}",
            put(save_product_version).delete(delete_product_version),
        )
        .route(
            "/api/products/{product_id}/versions/{version}/publish",
            post(publish_product_version),
        )
        .route(
            "/api/products/{product_id}/versions/{version}/rollback",
            post(rollback_product_version),
        )
        .route("/api/edge-nodes", get(edge_nodes).post(create_edge_node))
        .route(
            "/api/edge-nodes/{edge_id}",
            axum::routing::delete(delete_edge_node),
        )
        .route(
            "/api/edge-nodes/{edge_id}/product-binding",
            put(bind_edge_product),
        )
        .route(
            "/api/edge-nodes/{edge_id}/access-token",
            post(generate_edge_access_token),
        )
        .route(
            "/api/device-models",
            get(device_models).post(create_device_model),
        )
        .route(
            "/api/device-models/{device_type}",
            put(save_device_model).delete(delete_device_model),
        )
        .route("/api/protocol-connections", get(protocol_connections))
        .route(
            "/api/edges/{edge_id}/protocol-connections",
            get(edge_protocol_connections).post(create_edge_protocol_connection),
        )
        .route(
            "/api/edges/{edge_id}/protocol-connections/{connection_id}",
            put(save_edge_protocol_connection).delete(delete_edge_protocol_connection),
        )
        .route("/api/collection-tasks", get(collection_tasks))
        .route(
            "/api/edges/{edge_id}/data-configs",
            get(edge_data_configs).post(create_edge_data_config),
        )
        .route(
            "/api/edges/{edge_id}/data-configs/{config_id}",
            put(save_edge_data_config).delete(delete_edge_data_config),
        )
        .route(
            "/api/edges/{edge_id}/collection-tasks",
            get(edge_collection_tasks).post(create_edge_collection_task),
        )
        .route(
            "/api/edges/{edge_id}/collection-tasks/{task_id}",
            put(save_edge_collection_task).delete(delete_edge_collection_task),
        )
        .route("/api/algorithms", get(algorithms))
        .route(
            "/api/edges/{edge_id}/algorithms",
            get(edge_algorithms).post(create_edge_algorithm),
        )
        .route(
            "/api/edges/{edge_id}/algorithms/{algorithm_id}",
            put(save_edge_algorithm).delete(delete_edge_algorithm),
        )
        .route("/api/audit-records", get(audit_records))
        .route("/api/runtime-status", get(runtime_status))
        .route(
            "/api/edges/{edge_id}/desired-config",
            get(edge_desired_config),
        )
        .route(
            "/api/edges/{edge_id}/runtime-metrics",
            get(runtime_status).post(report_runtime_metrics),
        )
        .route(
            "/api/edges/{edge_id}/runtime-events",
            get(runtime_status).post(report_runtime_event),
        )
        .route(
            "/api/edges/{edge_id}/mqtt-uplink",
            get(edge_mqtt_uplink).put(save_edge_mqtt_uplink),
        )
        .route(
            "/api/edges/{edge_id}/discovery/run",
            post(run_edge_discovery),
        )
        .route(
            "/api/edges/{edge_id}/discovery/suggestions",
            get(edge_discovery_suggestions),
        )
        .route(
            "/api/edges/{edge_id}/reported-config",
            get(releases).post(edge_reported_config),
        )
        .route(
            "/api/edges/{edge_id}/config/validate",
            post(validate_edge_config),
        )
        .route("/api/point-mappings", get(point_mappings))
        .route("/api/point-mappings/{point_id}", put(save_point_mapping))
        .route(
            "/api/edges/{edge_id}/point-mappings",
            get(edge_point_mappings).post(create_edge_point_mapping),
        )
        .route(
            "/api/edges/{edge_id}/point-mappings/{point_id}",
            put(save_edge_point_mapping).delete(delete_edge_point_mapping),
        )
        .route("/api/releases", get(releases).post(create_release))
        .route(
            "/api/releases/publish",
            get(releases).post(publish_latest_release),
        )
        .route(
            "/api/edges/{edge_id}/releases/publish",
            post(publish_latest_release_for_edge),
        )
        .route("/api/edges/{edge_id}/releases/diff", post(release_diff))
        .route("/api/agent/safety-check", post(agent_safety_check))
        .route("/api/agent/suggestions", post(agent_suggestions))
        .route("/api/agent/provider", get(agent_provider_status))
        .route("/api/agent/chat", post(agent_chat))
        .route("/api/agent/conversations", get(agent_conversations))
        .route(
            "/api/agent/conversations/{conversation_id}",
            get(agent_conversation).delete(delete_agent_conversation),
        )
        .route(
            "/api/agent/knowledge",
            get(agent_knowledge_documents).post(create_agent_knowledge_document),
        )
        .route(
            "/api/agent/knowledge/{document_id}",
            put(save_agent_knowledge_document).delete(delete_agent_knowledge_document),
        )
        .route(
            "/api/agent/proposals",
            get(agent_proposals).post(create_agent_proposal),
        )
        .route(
            "/api/agent/proposals/{proposal_id}/approve",
            post(approve_agent_proposal),
        )
        .route(
            "/api/agent/proposals/{proposal_id}/reject",
            post(reject_agent_proposal),
        )
        .route_layer(middleware::from_fn_with_state(auth, authorize_api_request));

    let console_dir = console_dist_dir();
    let static_files = ServeDir::new(&console_dir)
        .not_found_service(ServeFile::new(console_dir.join("index.html")));

    Router::new()
        .route("/health/live", get(liveness))
        .route("/health/ready", get(readiness))
        .merge(api)
        .with_state(state)
        .fallback_service(static_files)
}

fn console_dist_dir() -> PathBuf {
    std::env::var("EDGEOPS_CONSOLE_DIST")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web/console/dist")
        })
}

async fn liveness() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "cloud-api",
        version: env!("CARGO_PKG_VERSION"),
        checks: BTreeMap::from([("process", "ok")]),
    })
}

async fn readiness(State(state): State<AppState>) -> impl IntoResponse {
    let mut checks = BTreeMap::from([("memory", "ok")]);
    if let Some(sqlite_store) = &state.sqlite_store {
        if sqlite_store.health_check().await.is_err() {
            checks.insert("sqlite", "unavailable");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(HealthResponse {
                    status: "not_ready",
                    service: "cloud-api",
                    version: env!("CARGO_PKG_VERSION"),
                    checks,
                }),
            );
        }
        checks.insert("sqlite", "ok");
    } else {
        checks.insert("sqlite", "not_configured");
    }

    (
        StatusCode::OK,
        Json(HealthResponse {
            status: "ready",
            service: "cloud-api",
            version: env!("CARGO_PKG_VERSION"),
            checks,
        }),
    )
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

async fn projects(State(state): State<AppState>) -> Json<Vec<Project>> {
    let store = state.store.lock().expect("store mutex poisoned");
    Json(store.projects().cloned().collect())
}

async fn create_project(
    State(state): State<AppState>,
    Json(request): Json<SaveProjectRequest>,
) -> Result<(StatusCode, Json<Project>), (StatusCode, Json<ErrorResponse>)> {
    validate_project_request(&request)?;
    {
        let store = state.store.lock().expect("store mutex poisoned");
        if store.project(&request.project_id).is_some() {
            return Err(error(StatusCode::CONFLICT, "project already exists"));
        }
    }
    let project = build_project(request, None);
    state
        .persist_project(project.clone())
        .await
        .map_err(persistence_error)?;
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.upsert_project(project.clone());
    store.push_audit(
        AuditAction::UpdateConfig,
        format!("project:{}", project.project_id),
    );
    Ok((StatusCode::CREATED, Json(project)))
}

async fn save_project(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(request): Json<SaveProjectRequest>,
) -> Result<Json<Project>, (StatusCode, Json<ErrorResponse>)> {
    if request.project_id != project_id {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "project id does not match path",
        ));
    }
    validate_project_request(&request)?;
    let existing = {
        let store = state.store.lock().expect("store mutex poisoned");
        store
            .project(&project_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing project"))?
    };
    let project = build_project(request, Some(existing));
    state
        .persist_project(project.clone())
        .await
        .map_err(persistence_error)?;
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.upsert_project(project.clone());
    store.push_audit(AuditAction::UpdateConfig, format!("project:{project_id}"));
    Ok(Json(project))
}

async fn delete_project(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    {
        let store = state.store.lock().expect("store mutex poisoned");
        if store.project(&project_id).is_none() {
            return Err(error(StatusCode::NOT_FOUND, "missing project"));
        }
        if store
            .point_sets()
            .any(|point_set| point_set.project_id == project_id)
            || store
                .products()
                .any(|product| product.project_id == project_id)
        {
            return Err(error(
                StatusCode::CONFLICT,
                "project still owns point sets or products",
            ));
        }
    }
    state
        .delete_project(&project_id)
        .await
        .map_err(persistence_error)?;
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.remove_project(&project_id);
    store.push_audit(
        AuditAction::UpdateConfig,
        format!("project:{project_id}:delete"),
    );
    Ok(StatusCode::NO_CONTENT)
}

async fn point_sets(State(state): State<AppState>) -> Json<Vec<PointSet>> {
    let store = state.store.lock().expect("store mutex poisoned");
    Json(store.point_sets().cloned().collect())
}

async fn create_point_set(
    State(state): State<AppState>,
    Json(request): Json<SavePointSetRequest>,
) -> Result<(StatusCode, Json<PointSet>), (StatusCode, Json<ErrorResponse>)> {
    validate_point_set_request(&request)?;
    {
        let store = state.store.lock().expect("store mutex poisoned");
        ensure_project_exists(&store, &request.project_id)?;
        if store.point_set(&request.point_set_id).is_some() {
            return Err(error(StatusCode::CONFLICT, "point set already exists"));
        }
    }
    let point_set = build_point_set(request, None);
    state
        .persist_point_set(point_set.clone())
        .await
        .map_err(persistence_error)?;
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.upsert_point_set(point_set.clone());
    store.push_audit(
        AuditAction::UpdateConfig,
        format!("point-set:{}", point_set.point_set_id),
    );
    Ok((StatusCode::CREATED, Json(point_set)))
}

async fn save_point_set(
    State(state): State<AppState>,
    Path(point_set_id): Path<String>,
    Json(request): Json<SavePointSetRequest>,
) -> Result<Json<PointSet>, (StatusCode, Json<ErrorResponse>)> {
    if request.point_set_id != point_set_id {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "point set id does not match path",
        ));
    }
    validate_point_set_request(&request)?;
    let existing = {
        let store = state.store.lock().expect("store mutex poisoned");
        ensure_project_exists(&store, &request.project_id)?;
        store
            .point_set(&point_set_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing point set"))?
    };
    let point_set = build_point_set(request, Some(existing));
    state
        .persist_point_set(point_set.clone())
        .await
        .map_err(persistence_error)?;
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.upsert_point_set(point_set.clone());
    store.push_audit(
        AuditAction::UpdateConfig,
        format!("point-set:{point_set_id}"),
    );
    Ok(Json(point_set))
}

async fn delete_point_set(
    State(state): State<AppState>,
    Path(point_set_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    {
        let store = state.store.lock().expect("store mutex poisoned");
        if store.point_set(&point_set_id).is_none() {
            return Err(error(StatusCode::NOT_FOUND, "missing point set"));
        }
        if store
            .product_versions()
            .any(|version| version.point_set_ids.iter().any(|id| id == &point_set_id))
        {
            return Err(error(
                StatusCode::CONFLICT,
                "point set is referenced by a product version",
            ));
        }
    }
    state
        .delete_point_set(&point_set_id)
        .await
        .map_err(persistence_error)?;
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.remove_point_set(&point_set_id);
    store.push_audit(
        AuditAction::UpdateConfig,
        format!("point-set:{point_set_id}:delete"),
    );
    Ok(StatusCode::NO_CONTENT)
}

async fn products(State(state): State<AppState>) -> Json<Vec<Product>> {
    let store = state.store.lock().expect("store mutex poisoned");
    Json(store.products().cloned().collect())
}

async fn create_product(
    State(state): State<AppState>,
    Json(request): Json<SaveProductRequest>,
) -> Result<(StatusCode, Json<Product>), (StatusCode, Json<ErrorResponse>)> {
    validate_product_request(&request)?;
    {
        let store = state.store.lock().expect("store mutex poisoned");
        ensure_project_exists(&store, &request.project_id)?;
        if store.product(&request.product_id).is_some() {
            return Err(error(StatusCode::CONFLICT, "product already exists"));
        }
    }
    let product = build_product(request, None);
    state
        .persist_product(product.clone())
        .await
        .map_err(persistence_error)?;
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.upsert_product(product.clone());
    store.push_audit(
        AuditAction::UpdateConfig,
        format!("product:{}", product.product_id),
    );
    Ok((StatusCode::CREATED, Json(product)))
}

async fn save_product(
    State(state): State<AppState>,
    Path(product_id): Path<String>,
    Json(request): Json<SaveProductRequest>,
) -> Result<Json<Product>, (StatusCode, Json<ErrorResponse>)> {
    if request.product_id != product_id {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "product id does not match path",
        ));
    }
    validate_product_request(&request)?;
    let existing = {
        let store = state.store.lock().expect("store mutex poisoned");
        ensure_project_exists(&store, &request.project_id)?;
        store
            .product(&product_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing product"))?
    };
    let product = build_product(request, Some(existing));
    state
        .persist_product(product.clone())
        .await
        .map_err(persistence_error)?;
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.upsert_product(product.clone());
    store.push_audit(AuditAction::UpdateConfig, format!("product:{product_id}"));
    Ok(Json(product))
}

async fn delete_product(
    State(state): State<AppState>,
    Path(product_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    {
        let store = state.store.lock().expect("store mutex poisoned");
        let product = store
            .product(&product_id)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing product"))?;
        if product.latest_version.is_some() {
            return Err(error(
                StatusCode::CONFLICT,
                "published product cannot be deleted",
            ));
        }
    }
    state
        .delete_product(&product_id)
        .await
        .map_err(persistence_error)?;
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.remove_product(&product_id);
    store.push_audit(
        AuditAction::UpdateConfig,
        format!("product:{product_id}:delete"),
    );
    Ok(StatusCode::NO_CONTENT)
}

async fn product_versions(
    State(state): State<AppState>,
    Path(product_id): Path<String>,
) -> Result<Json<Vec<ProductVersion>>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    if store.product(&product_id).is_none() {
        return Err(error(StatusCode::NOT_FOUND, "missing product"));
    }
    Ok(Json(
        store
            .product_versions()
            .filter(|version| version.product_id == product_id)
            .cloned()
            .collect(),
    ))
}

async fn create_product_version(
    State(state): State<AppState>,
    Path(product_id): Path<String>,
    Json(request): Json<SaveProductVersionRequest>,
) -> Result<(StatusCode, Json<ProductVersion>), (StatusCode, Json<ErrorResponse>)> {
    validate_product_version_request(&product_id, &request, &state, false)?;
    let version = build_product_version(product_id.clone(), request, None);
    state
        .persist_product_version(version.clone())
        .await
        .map_err(persistence_error)?;
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.upsert_product_version(version.clone());
    store.push_audit(
        AuditAction::UpdateConfig,
        format!("product:{product_id}:version:{}", version.version),
    );
    Ok((StatusCode::CREATED, Json(version)))
}

async fn save_product_version(
    State(state): State<AppState>,
    Path((product_id, version_id)): Path<(String, String)>,
    Json(request): Json<SaveProductVersionRequest>,
) -> Result<Json<ProductVersion>, (StatusCode, Json<ErrorResponse>)> {
    if request.version != version_id {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "product version does not match path",
        ));
    }
    validate_product_version_request(&product_id, &request, &state, true)?;
    let existing = {
        let store = state.store.lock().expect("store mutex poisoned");
        store
            .product_version(&product_id, &version_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing product version"))?
    };
    if existing.status != ProductVersionStatus::Draft {
        return Err(error(
            StatusCode::CONFLICT,
            "published or retired product version is immutable",
        ));
    }
    let version = build_product_version(product_id.clone(), request, Some(existing));
    state
        .persist_product_version(version.clone())
        .await
        .map_err(persistence_error)?;
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.upsert_product_version(version.clone());
    store.push_audit(
        AuditAction::UpdateConfig,
        format!("product:{product_id}:version:{version_id}"),
    );
    Ok(Json(version))
}

async fn delete_product_version(
    State(state): State<AppState>,
    Path((product_id, version)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    {
        let store = state.store.lock().expect("store mutex poisoned");
        let candidate = store
            .product_version(&product_id, &version)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing product version"))?;
        if candidate.status != ProductVersionStatus::Draft {
            return Err(error(
                StatusCode::CONFLICT,
                "only draft product versions can be deleted",
            ));
        }
    }
    state
        .delete_product_version(&product_id, &version)
        .await
        .map_err(persistence_error)?;
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.remove_product_version(&product_id, &version);
    store.push_audit(
        AuditAction::UpdateConfig,
        format!("product:{product_id}:version:{version}:delete"),
    );
    Ok(StatusCode::NO_CONTENT)
}

async fn publish_product_version(
    State(state): State<AppState>,
    Path((product_id, version)): Path<(String, String)>,
) -> Result<Json<ProductVersion>, (StatusCode, Json<ErrorResponse>)> {
    transition_product_version(state, product_id, version, false).await
}

async fn rollback_product_version(
    State(state): State<AppState>,
    Path((product_id, version)): Path<(String, String)>,
) -> Result<Json<ProductVersion>, (StatusCode, Json<ErrorResponse>)> {
    transition_product_version(state, product_id, version, true).await
}

async fn transition_product_version(
    state: AppState,
    product_id: String,
    target_version: String,
    rollback: bool,
) -> Result<Json<ProductVersion>, (StatusCode, Json<ErrorResponse>)> {
    let (mut product, mut target, previous) = {
        let store = state.store.lock().expect("store mutex poisoned");
        let product = store
            .product(&product_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing product"))?;
        let target = store
            .product_version(&product_id, &target_version)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing product version"))?;
        let previous = product
            .latest_version
            .as_ref()
            .filter(|version| *version != &target_version)
            .and_then(|version| store.product_version(&product_id, version))
            .cloned();
        (product, target, previous)
    };

    if product.latest_version.as_deref() == Some(target_version.as_str())
        && target.status == ProductVersionStatus::Published
    {
        return Ok(Json(target));
    }

    if rollback {
        if target.status == ProductVersionStatus::Draft {
            return Err(error(
                StatusCode::CONFLICT,
                "cannot roll back to a draft product version",
            ));
        }
    } else if target.status != ProductVersionStatus::Draft {
        return Err(error(
            StatusCode::CONFLICT,
            "only draft product versions can be published",
        ));
    }
    validate_publishable_product_version(&target)?;

    let mut changed_versions = Vec::new();
    if let Some(mut previous) = previous {
        previous.status = ProductVersionStatus::Retired;
        changed_versions.push(previous);
    }
    target.status = ProductVersionStatus::Published;
    changed_versions.push(target.clone());
    product.latest_version = Some(target_version.clone());
    product.updated_at = Utc::now();

    let (rollout_edges, rollout_packages, rollout_releases) = {
        let store = state.store.lock().expect("store mutex poisoned");
        let mut edges = store
            .edge_nodes()
            .filter(|edge| edge.product_id.as_deref() == Some(product_id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let mut packages = Vec::with_capacity(edges.len());
        let mut releases = Vec::new();

        for edge in &mut edges {
            edge.desired_product_version = Some(target_version.clone());
            let package = materialize_product_config_package(&store, &edge.edge_id, &target)?;

            releases.extend(
                store
                    .releases()
                    .filter(|release| {
                        release.edge_id == edge.edge_id && release.status == ReleaseStatus::Pending
                    })
                    .cloned()
                    .map(|mut release| {
                        release.status = ReleaseStatus::Superseded;
                        release
                    }),
            );
            releases.push(ReleaseService::prepare_release(&package).map_err(|errors| {
                error(
                    StatusCode::BAD_REQUEST,
                    errors
                        .into_iter()
                        .map(|error| error.message)
                        .collect::<Vec<_>>()
                        .join("; "),
                )
            })?);
            packages.push(package);
        }

        (edges, packages, releases)
    };

    state
        .persist_product_version_transition(
            product.clone(),
            changed_versions.clone(),
            rollout_edges.clone(),
            rollout_packages.clone(),
            rollout_releases.clone(),
        )
        .await
        .map_err(persistence_error)?;

    let mut store = state.store.lock().expect("store mutex poisoned");
    store.upsert_product(product);
    for version in changed_versions {
        store.upsert_product_version(version);
    }
    for edge in rollout_edges {
        store.register_edge(edge);
    }
    for package in rollout_packages {
        store.upsert_config_package(package);
    }
    for release in rollout_releases {
        let is_pending = release.status == ReleaseStatus::Pending;
        let release_id = release.release_id;
        store.insert_release(release);
        if is_pending {
            store.push_audit(AuditAction::CreateRelease, release_id.to_string());
        }
    }
    let action = if rollback { "rollback" } else { "publish" };
    store.push_audit(
        AuditAction::UpdateConfig,
        format!("product:{product_id}:version:{target_version}:{action}"),
    );
    Ok(Json(target))
}

async fn point_mappings(State(state): State<AppState>) -> Json<Vec<PointMappingResponse>> {
    let store = state.store.lock().expect("store mutex poisoned");
    let mut points = Vec::new();

    for edge in store.edge_nodes() {
        let Some(package) = store.latest_config_package_for_edge(&edge.edge_id) else {
            continue;
        };
        for mapping in &package.point_mappings {
            points.push(point_mapping_response(package, mapping));
        }
    }

    Json(points)
}

async fn edge_point_mappings(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<Vec<PointMappingResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    let package = store
        .latest_config_package_for_edge(&edge_id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
    let mut points = package
        .point_mappings
        .iter()
        .map(|mapping| point_mapping_response(package, mapping))
        .collect::<Vec<_>>();
    points.sort_by(|left, right| left.point_id.cmp(&right.point_id));

    Ok(Json(points))
}

async fn create_edge_point_mapping(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    request: Option<Json<CreatePointMappingRequest>>,
) -> Result<(StatusCode, Json<PointMappingResponse>), (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let mapping = match request {
            Some(Json(request)) => build_point_mapping_from_create_request(&package, request)?,
            None => {
                let point_id = next_point_id(&package);
                let device_id = package
                    .devices
                    .first()
                    .map(|device| device.device_id.clone())
                    .unwrap_or_else(|| "device-draft-1".to_string());
                let connection_id = package
                    .protocol_connections
                    .first()
                    .map(|connection| connection.connection_id.clone())
                    .unwrap_or_else(|| "simulated-main".to_string());
                TelemetryPointMapping::new(
                    point_id.clone(),
                    device_id,
                    format!("pump.{point_id}"),
                    connection_id,
                    PointAddress::simulated(point_id.clone()),
                    TelemetryType::Float,
                )
                .with_unit("-")
            }
        };

        package.version = next_version(&package.version);
        package.point_mappings.push(mapping);
        let response = point_mapping_response(
            &package,
            package
                .point_mappings
                .last()
                .expect("new point mapping exists"),
        );
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id);
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok((StatusCode::CREATED, Json(response)))
}

async fn edge_nodes(
    Query(query): Query<EdgeNodesQuery>,
    State(state): State<AppState>,
) -> axum::response::Response {
    let store = state.store.lock().expect("store mutex poisoned");
    let mut rows = store
        .edge_nodes()
        .map(|edge| {
            let runtime = store.runtime_metrics(&edge.edge_id);
            edge_node_response(edge, runtime)
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| left.edge_id.cmp(&right.edge_id));

    if query.page.is_none() && query.page_size.is_none() {
        return Json(rows).into_response();
    }

    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
    let total = rows.len();
    let total_pages = total.div_ceil(page_size).max(1);
    let start = (page.saturating_sub(1) * page_size).min(total);
    let items = rows
        .into_iter()
        .skip(start)
        .take(page_size)
        .collect::<Vec<_>>();

    Json(EdgeNodesPageResponse {
        items,
        page,
        page_size,
        total,
        total_pages,
    })
    .into_response()
}

async fn edge_mqtt_uplink(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<MqttUplinkResponse>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    let uplink = store
        .mqtt_uplink(&edge_id)
        .cloned()
        .or_else(|| {
            store
                .latest_config_package_for_edge(&edge_id)
                .and_then(|package| package.mqtt_uplinks.first().cloned())
        })
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing mqtt uplink config"))?;

    Ok(Json(mqtt_uplink_response(uplink)))
}

async fn save_edge_mqtt_uplink(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    Json(request): Json<SaveMqttUplinkRequest>,
) -> Result<Json<MqttUplinkResponse>, (StatusCode, Json<ErrorResponse>)> {
    let username = non_empty(request.username);
    let password_env = non_empty(request.password_env);
    let tls_ca_path = non_empty(request.tls_ca_path);
    if username.is_some() != password_env.is_some() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "MQTT username and password environment reference must be configured together",
        ));
    }
    if tls_ca_path.is_some()
        && !matches!(
            request
                .broker
                .split_once("://")
                .map(|(scheme, _)| scheme.to_ascii_lowercase())
                .as_deref(),
            Some("mqtts" | "ssl")
        )
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "MQTT TLS CA path requires an mqtts:// broker",
        ));
    }
    let uplink = MqttUplinkConfig {
        sink_id: request.sink_id,
        broker: request.broker,
        client_id: request.client_id,
        username,
        password_env,
        tls_ca_path,
        topic_template: request.topic_template,
        qos: request.qos,
        batch_size: request.batch_size,
        flush_interval_ms: request.flush_interval_ms,
    };

    let package_to_persist = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        package.version = next_version(&package.version);
        package.mqtt_uplinks = vec![uplink.clone()];
        store.upsert_config_package(package.clone());
        store.upsert_mqtt_uplink(edge_id.clone(), uplink.clone());
        package
    };

    state
        .persist_config_package(package_to_persist)
        .await
        .map_err(persistence_error)?;
    state
        .persist_mqtt_uplink(&edge_id, uplink.clone())
        .await
        .map_err(persistence_error)?;

    Ok(Json(mqtt_uplink_response(uplink)))
}

async fn run_edge_discovery(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    Json(request): Json<RunDiscoveryRequest>,
) -> Result<(StatusCode, Json<DiscoveryReportResponse>), (StatusCode, Json<ErrorResponse>)> {
    let (start_address, end_address) = parse_holding_register_range(&request.address_range)
        .map_err(|message| error(StatusCode::BAD_REQUEST, message))?;
    let discovery = DiscoveryRequest::modbus_holding_registers(
        format!("discovery-{edge_id}-{}", Utc::now().timestamp_millis()),
        request.connection_id.clone(),
        start_address,
        end_address,
    );
    discovery
        .validate()
        .map_err(|message| error(StatusCode::BAD_REQUEST, message))?;

    {
        let store = state.store.lock().expect("store mutex poisoned");
        let Some(package) = store.latest_config_package_for_edge(&edge_id) else {
            return Err(error(StatusCode::NOT_FOUND, "missing edge config package"));
        };
        let Some(connection) = package
            .protocol_connections
            .iter()
            .find(|connection| connection.connection_id == request.connection_id)
        else {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "discovery connection is not configured for this edge",
            ));
        };
        if connection.protocol != ProtocolType::ModbusRtu {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "runtime discovery currently supports Modbus RTU connections only",
            ));
        }
    }

    let report = state
        .gateway_commands
        .dispatch_discovery(&edge_id, discovery, Duration::from_secs(10))
        .await
        .map_err(discovery_dispatch_error)?;
    state
        .store
        .lock()
        .expect("store mutex poisoned")
        .push_audit(AuditAction::UpdateConfig, format!("{edge_id}:discovery"));
    Ok((StatusCode::CREATED, Json(discovery_report_response(report))))
}

fn discovery_dispatch_error(
    error_value: EdgeGatewayDispatchError,
) -> (StatusCode, Json<ErrorResponse>) {
    let status = match error_value {
        EdgeGatewayDispatchError::Offline => StatusCode::SERVICE_UNAVAILABLE,
        EdgeGatewayDispatchError::Busy => StatusCode::CONFLICT,
        EdgeGatewayDispatchError::Timeout => StatusCode::GATEWAY_TIMEOUT,
        EdgeGatewayDispatchError::Failed(_) => StatusCode::BAD_GATEWAY,
    };
    error(status, error_value.to_string())
}

async fn edge_discovery_suggestions(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Json<Vec<PointMappingSuggestionResponse>> {
    let store = state.store.lock().expect("store mutex poisoned");
    Json(
        store
            .discovery_suggestions(&edge_id)
            .into_iter()
            .map(point_mapping_suggestion_response)
            .collect(),
    )
}

fn parse_holding_register_range(value: &str) -> Result<(u32, u32), String> {
    let (kind, range) = value
        .split_once(':')
        .ok_or_else(|| "addressRange must use holding_register:start-end".to_string())?;
    if kind != "holding_register" {
        return Err("only holding_register discovery ranges are supported".to_string());
    }
    let (start, end) = range
        .split_once('-')
        .ok_or_else(|| "addressRange must include a start and end address".to_string())?;
    let start = start
        .parse::<u32>()
        .map_err(|_| "invalid discovery start address".to_string())?;
    let end = end
        .parse::<u32>()
        .map_err(|_| "invalid discovery end address".to_string())?;
    Ok((start, end))
}

async fn create_edge_node(
    State(state): State<AppState>,
    Json(request): Json<CreateEdgeNodeRequest>,
) -> Result<(StatusCode, Json<EdgeNodeResponse>), (StatusCode, Json<ErrorResponse>)> {
    let (node, package, credential, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let edge_id = next_edge_id(&store.edge_nodes().cloned().collect::<Vec<_>>());
        let mut node = EdgeNode::new(
            edge_id.clone(),
            request
                .display_name
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "新边端待确认".to_string()),
        )
        .at_site(
            request
                .site
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "待分配".to_string()),
        )
        .with_capability("registration:draft");
        let product_id = request
            .product_id
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| error(StatusCode::BAD_REQUEST, "productId is required"))?;
        let product = store
            .product(&product_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing product"))?;
        let project_id = request
            .project_id
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| product.project_id.clone());
        if project_id != product.project_id {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "edge project does not match product project",
            ));
        }
        let desired_version = product
            .latest_version
            .clone()
            .ok_or_else(|| error(StatusCode::CONFLICT, "product has no published version"))?;
        let product_version = store
            .product_version(&product_id, &desired_version)
            .cloned()
            .ok_or_else(|| error(StatusCode::CONFLICT, "missing published product version"))?;
        if product_version.status != ProductVersionStatus::Published {
            return Err(error(
                StatusCode::CONFLICT,
                "product latest version is not published",
            ));
        }
        node = node
            .with_capability(format!("project:{project_id}"))
            .with_capability(format!("product:{product_id}"))
            .bind_product(&project_id, &product_id, &desired_version);
        let package = materialize_product_config_package(&store, &node.edge_id, &product_version)?;
        let (access_token, credential) = new_edge_access_credential(&node.edge_id);

        store.register_edge(node.clone());
        store.replace_edge_credential(credential.clone());
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, node.edge_id.clone());

        let mut response = edge_node_response(&node, None);
        response.access_token = Some(access_token);
        (node, package, credential, response)
    };

    state
        .persist_edge_node(node)
        .await
        .map_err(persistence_error)?;
    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;
    state
        .persist_edge_credential(credential)
        .await
        .map_err(persistence_error)?;

    Ok((StatusCode::CREATED, Json(response)))
}

async fn bind_edge_product(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    Json(request): Json<BindEdgeProductRequest>,
) -> Result<Json<EdgeNodeResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (node, package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut node = store
            .edge_nodes()
            .find(|edge| edge.edge_id == edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge node"))?;
        let product = store
            .product(&request.product_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing product"))?;
        if product.project_id != request.project_id {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "edge project does not match product project",
            ));
        }
        let desired_version = request
            .desired_version
            .filter(|value| !value.trim().is_empty())
            .or_else(|| product.latest_version.clone())
            .ok_or_else(|| error(StatusCode::CONFLICT, "product has no published version"))?;
        let product_version = store
            .product_version(&request.product_id, &desired_version)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing product version"))?;
        if product_version.status != ProductVersionStatus::Published {
            return Err(error(
                StatusCode::CONFLICT,
                "only published product versions can be bound to an edge",
            ));
        }

        node.project_id = Some(request.project_id.clone());
        node.product_id = Some(request.product_id.clone());
        node.desired_product_version = Some(desired_version);
        node.capabilities.retain(|capability| {
            !capability.starts_with("project:") && !capability.starts_with("product:")
        });
        node.capabilities
            .push(format!("project:{}", request.project_id));
        node.capabilities
            .push(format!("product:{}", request.product_id));
        let package = materialize_product_config_package(&store, &edge_id, &product_version)?;
        store.register_edge(node.clone());
        store.upsert_config_package(package.clone());
        store.push_audit(
            AuditAction::UpdateConfig,
            format!(
                "{edge_id}:product-binding:{}@{}",
                product_version.product_id, product_version.version
            ),
        );
        let runtime = store.runtime_metrics(&edge_id);
        let response = edge_node_response(&node, runtime);
        (node, package, response)
    };

    state
        .persist_edge_node(node)
        .await
        .map_err(persistence_error)?;
    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok(Json(response))
}

async fn generate_edge_access_token(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<EdgeAccessTokenResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (credential, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        if store.edge_nodes().all(|edge| edge.edge_id != edge_id) {
            return Err(error(StatusCode::NOT_FOUND, "missing edge node"));
        }
        let (access_token, credential) = new_edge_access_credential(&edge_id);
        store.replace_edge_credential(credential.clone());
        store.push_audit(
            AuditAction::UpdateConfig,
            format!("{edge_id}:access-token:{}", credential.credential_id),
        );
        let response = EdgeAccessTokenResponse {
            access_token,
            created_at: credential.created_at,
            credential_id: credential.credential_id.to_string(),
            edge_id,
        };
        (credential, response)
    };

    state
        .persist_edge_credential(credential)
        .await
        .map_err(persistence_error)?;
    Ok(Json(response))
}

async fn delete_edge_node(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    {
        let mut store = state.store.lock().expect("store mutex poisoned");
        if store.edge_nodes().all(|edge| edge.edge_id != edge_id) {
            return Err(error(StatusCode::NOT_FOUND, "missing edge node"));
        }
        if store
            .runtime_metrics(&edge_id)
            .map(|snapshot| snapshot.health != EdgeHealth::Offline)
            .unwrap_or(false)
        {
            return Err(error(
                StatusCode::CONFLICT,
                "edge node has active runtime metrics; stop runtime or wait until offline before removal",
            ));
        }
        store.remove_edge_node(&edge_id);
        store.push_audit(AuditAction::UpdateConfig, format!("{edge_id}:delete"));
    }

    state
        .delete_edge_node(&edge_id)
        .await
        .map_err(persistence_error)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn device_models(State(state): State<AppState>) -> Json<Vec<DeviceModelResponse>> {
    let store = state.store.lock().expect("store mutex poisoned");
    let mut rows = store
        .device_models()
        .map(device_model_response)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| left.device_type.cmp(&right.device_type));

    Json(rows)
}

async fn create_device_model(
    State(state): State<AppState>,
    request: Option<Json<CreateDeviceModelRequest>>,
) -> Result<(StatusCode, Json<DeviceModelResponse>), (StatusCode, Json<ErrorResponse>)> {
    let (package, model, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let edge_id = default_config_edge_id(&store)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge node"))?;
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let model = match request {
            Some(Json(request)) => build_device_model_from_request(request)?,
            None => DeviceSpec::new(next_device_model_type(&package), "v1")
                .with_telemetry(vec![TelemetryPoint::new("status", TelemetryType::Boolean)
                    .with_description("设备状态")]),
        };

        package.version = next_version(&package.version);
        package.device_models.push(model.clone());
        store.upsert_device_model(model.clone());
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id);

        let response = device_model_response(&model);
        (package, model, response)
    };

    state
        .persist_device_model(model)
        .await
        .map_err(persistence_error)?;
    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok((StatusCode::CREATED, Json(response)))
}

async fn save_device_model(
    State(state): State<AppState>,
    Path(device_type): Path<String>,
    Json(request): Json<SaveDeviceModelRequest>,
) -> Result<Json<DeviceModelResponse>, (StatusCode, Json<ErrorResponse>)> {
    let model = build_device_model(device_type.clone(), request.version, request.telemetry)?;
    let (packages, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        if store.device_model(&device_type).is_none()
            && !store.config_packages().any(|package| {
                package
                    .device_models
                    .iter()
                    .any(|candidate| candidate.device_type == device_type)
            })
        {
            return Err(error(StatusCode::NOT_FOUND, "missing device model"));
        }

        store.upsert_device_model(model.clone());
        let edge_ids = store
            .edge_nodes()
            .map(|edge| edge.edge_id.clone())
            .collect::<Vec<_>>();
        let mut packages = Vec::new();
        for edge_id in edge_ids {
            let Some(mut package) = store.latest_config_package_for_edge(&edge_id).cloned() else {
                continue;
            };
            let Some(model_index) = package
                .device_models
                .iter()
                .position(|candidate| candidate.device_type == device_type)
            else {
                continue;
            };
            package.version = next_version(&package.version);
            package.device_models[model_index] = model.clone();
            store.upsert_config_package(package.clone());
            store.push_audit(AuditAction::UpdateConfig, edge_id);
            packages.push(package);
        }
        let response = device_model_response(&model);
        (packages, response)
    };

    state
        .persist_device_model(model)
        .await
        .map_err(persistence_error)?;
    for package in packages {
        state
            .persist_config_package(package)
            .await
            .map_err(persistence_error)?;
    }

    Ok(Json(response))
}

async fn delete_device_model(
    State(state): State<AppState>,
    Path(device_type): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let packages_to_persist = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        if store.device_model(&device_type).is_none()
            && !store.config_packages().any(|package| {
                package
                    .device_models
                    .iter()
                    .any(|candidate| candidate.device_type == device_type)
            })
        {
            return Err(error(StatusCode::NOT_FOUND, "missing device model"));
        }
        if store.config_packages().any(|package| {
            package
                .devices
                .iter()
                .any(|device| device.device_type == device_type)
        }) {
            return Err(error(
                StatusCode::CONFLICT,
                format!("device model `{device_type}` is referenced by edge devices"),
            ));
        }

        store.remove_device_model(&device_type);
        let edge_ids = store
            .edge_nodes()
            .map(|edge| edge.edge_id.clone())
            .collect::<Vec<_>>();
        let mut packages = Vec::new();
        for edge_id in edge_ids {
            let Some(mut package) = store.latest_config_package_for_edge(&edge_id).cloned() else {
                continue;
            };
            let previous_len = package.device_models.len();
            package
                .device_models
                .retain(|candidate| candidate.device_type != device_type);
            if package.device_models.len() == previous_len {
                continue;
            }
            package.version = next_version(&package.version);
            store.upsert_config_package(package.clone());
            store.push_audit(AuditAction::UpdateConfig, edge_id);
            packages.push(package);
        }
        packages
    };

    state
        .delete_device_model(&device_type)
        .await
        .map_err(persistence_error)?;
    for package in packages_to_persist {
        state
            .persist_config_package(package)
            .await
            .map_err(persistence_error)?;
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn protocol_connections(
    State(state): State<AppState>,
) -> Json<Vec<ProtocolConnectionResponse>> {
    let store = state.store.lock().expect("store mutex poisoned");
    let mut rows = Vec::new();

    for package in store
        .edge_nodes()
        .filter_map(|edge| store.latest_config_package_for_edge(&edge.edge_id))
    {
        let runtime = store.runtime_metrics(&package.edge_id);
        for connection in &package.protocol_connections {
            let runtime_connection = runtime.and_then(|snapshot| {
                snapshot
                    .protocols
                    .iter()
                    .find(|protocol| protocol.connection_id == connection.connection_id)
            });
            rows.push(protocol_connection_response(
                package,
                connection,
                runtime_connection.map(|metrics| metrics.connected),
            ));
        }
    }
    rows.sort_by(|left, right| left.connection_id.cmp(&right.connection_id));

    Json(rows)
}

async fn edge_protocol_connections(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<Vec<ProtocolConnectionResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    let package = store
        .latest_config_package_for_edge(&edge_id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
    let runtime = store.runtime_metrics(&edge_id);
    let mut connections = package
        .protocol_connections
        .iter()
        .map(|connection| {
            let connected = runtime.and_then(|snapshot| {
                snapshot
                    .protocols
                    .iter()
                    .find(|protocol| protocol.connection_id == connection.connection_id)
                    .map(|metrics| metrics.connected)
            });
            protocol_connection_response(package, connection, connected)
        })
        .collect::<Vec<_>>();
    connections.sort_by(|left, right| left.connection_id.cmp(&right.connection_id));

    Ok(Json(connections))
}

async fn create_edge_protocol_connection(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    Json(request): Json<CreateProtocolConnectionRequest>,
) -> Result<(StatusCode, Json<ProtocolConnectionResponse>), (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let connection_id = next_connection_id(&package);
        let protocol = request.protocol_type.unwrap_or(ProtocolType::ModbusTcp);
        let (endpoint, serial) =
            normalize_connection_transport(protocol, request.endpoint, request.serial, None)?;
        let connection = ProtocolConnection {
            connection_id,
            protocol,
            endpoint,
            serial,
        };

        package.version = next_version(&package.version);
        package.protocol_connections.push(connection);
        let response = protocol_connection_response(
            &package,
            package
                .protocol_connections
                .last()
                .expect("new protocol connection exists"),
            None,
        );
        store.upsert_config_package(package.clone());
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok((StatusCode::CREATED, Json(response)))
}

async fn collection_tasks(State(state): State<AppState>) -> Json<Vec<CollectionTaskResponse>> {
    let store = state.store.lock().expect("store mutex poisoned");
    let mut rows = Vec::new();

    for package in store
        .edge_nodes()
        .filter_map(|edge| store.latest_config_package_for_edge(&edge.edge_id))
    {
        for task in &package.collection_tasks {
            rows.push(collection_task_response(package, task));
        }
    }
    rows.sort_by(|left, right| left.task_id.cmp(&right.task_id));

    Json(rows)
}

async fn edge_data_configs(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<Vec<DataConfigResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    let package = store
        .latest_config_package_for_edge(&edge_id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
    let mut configs = package
        .data_configs
        .iter()
        .map(|data_config| data_config_response(package, data_config))
        .collect::<Vec<_>>();
    configs.sort_by(|left, right| left.config_id.cmp(&right.config_id));

    Ok(Json(configs))
}

async fn create_edge_data_config(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    Json(request): Json<SaveDataConfigRequest>,
) -> Result<(StatusCode, Json<DataConfigResponse>), (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let data_config = build_data_config_from_request(&package, None, request)?;
        if package
            .data_configs
            .iter()
            .any(|candidate| candidate.config_id == data_config.config_id)
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!("data config `{}` already exists", data_config.config_id),
            ));
        }

        package.version = next_version(&package.version);
        package.data_configs.push(data_config);
        let response = data_config_response(
            &package,
            package.data_configs.last().expect("new data config exists"),
        );
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id);
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok((StatusCode::CREATED, Json(response)))
}

async fn save_edge_data_config(
    State(state): State<AppState>,
    Path((edge_id, config_id)): Path<(String, String)>,
    Json(request): Json<SaveDataConfigRequest>,
) -> Result<Json<DataConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let config_index = package
            .data_configs
            .iter()
            .position(|candidate| candidate.config_id == config_id)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing data config"))?;

        let data_config = build_data_config_from_request(&package, Some(&config_id), request)?;
        if data_config.config_id != config_id
            && package
                .data_configs
                .iter()
                .any(|candidate| candidate.config_id == data_config.config_id)
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!("data config `{}` already exists", data_config.config_id),
            ));
        }

        package.version = next_version(&package.version);
        package.data_configs[config_index] = data_config;
        let response = data_config_response(&package, &package.data_configs[config_index]);
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id);
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok(Json(response))
}

async fn delete_edge_data_config(
    State(state): State<AppState>,
    Path((edge_id, config_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let package = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let before = package.data_configs.len();
        package
            .data_configs
            .retain(|candidate| candidate.config_id != config_id);
        if package.data_configs.len() == before {
            return Err(error(StatusCode::NOT_FOUND, "missing data config"));
        }
        package.version = next_version(&package.version);
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id);
        package
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn edge_collection_tasks(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<Vec<CollectionTaskResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    let package = store
        .latest_config_package_for_edge(&edge_id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
    let mut tasks = package
        .collection_tasks
        .iter()
        .map(|task| collection_task_response(package, task))
        .collect::<Vec<_>>();
    tasks.sort_by(|left, right| left.task_id.cmp(&right.task_id));

    Ok(Json(tasks))
}

async fn create_edge_collection_task(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    request: Option<Json<CreateCollectionTaskRequest>>,
) -> Result<(StatusCode, Json<CollectionTaskResponse>), (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let task = match request {
            Some(Json(request)) => build_collection_task_from_create_request(&package, request)?,
            None => {
                let device_id = package
                    .devices
                    .first()
                    .map(|device| device.device_id.clone())
                    .unwrap_or_else(|| "device-draft-1".to_string());
                let point_ids = package
                    .point_mappings
                    .iter()
                    .map(|mapping| mapping.point_id.clone())
                    .collect::<Vec<_>>();
                if point_ids.is_empty() {
                    return Err(error(
                        StatusCode::BAD_REQUEST,
                        "collection task requires at least one point mapping",
                    ));
                }
                CollectionTask::interval(next_task_id(&package), device_id, point_ids, 1000)
            }
        };

        package.version = next_version(&package.version);
        package.collection_tasks.push(task);
        let response = collection_task_response(
            &package,
            package
                .collection_tasks
                .last()
                .expect("new collection task exists"),
        );
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id);
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok((StatusCode::CREATED, Json(response)))
}

async fn algorithms(State(state): State<AppState>) -> Json<Vec<AlgorithmResponse>> {
    let store = state.store.lock().expect("store mutex poisoned");
    let mut rows = Vec::new();

    for package in store
        .edge_nodes()
        .filter_map(|edge| store.latest_config_package_for_edge(&edge.edge_id))
    {
        for algorithm in &package.algorithms {
            rows.push(algorithm_response(package, algorithm));
        }
    }
    rows.sort_by(|left, right| left.algorithm_id.cmp(&right.algorithm_id));

    Json(rows)
}

async fn edge_algorithms(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<Vec<AlgorithmResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    let package = store
        .latest_config_package_for_edge(&edge_id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
    let mut algorithms = package
        .algorithms
        .iter()
        .map(|algorithm| algorithm_response(package, algorithm))
        .collect::<Vec<_>>();
    algorithms.sort_by(|left, right| left.algorithm_id.cmp(&right.algorithm_id));

    Ok(Json(algorithms))
}

async fn create_edge_algorithm(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    request: Option<Json<CreateAlgorithmRequest>>,
) -> Result<(StatusCode, Json<AlgorithmResponse>), (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let algorithm = match request {
            Some(Json(request)) => build_algorithm_from_create_request(&package, request)?,
            None => {
                let input = package
                    .point_mappings
                    .first()
                    .map(|mapping| mapping.point_id.clone())
                    .ok_or_else(|| {
                        error(StatusCode::BAD_REQUEST, "algorithm requires an input point")
                    })?;
                let algorithm_id = next_algorithm_id(&package);
                AlgorithmSpec {
                    id: algorithm_id.clone(),
                    version: "0.1.0".to_string(),
                    kind: AlgorithmKind::ChangeReport,
                    dsl: AlgorithmDsl::default(),
                    runtime: AlgorithmRuntime::Rule,
                    inputs: vec![input],
                    outputs: vec![format!("{algorithm_id}.output")],
                }
            }
        };

        package.version = next_version(&package.version);
        package.algorithms.push(algorithm);
        let response = algorithm_response(
            &package,
            package.algorithms.last().expect("new algorithm exists"),
        );
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id);
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok((StatusCode::CREATED, Json(response)))
}

async fn audit_records(State(state): State<AppState>) -> Json<Vec<AuditRecordResponse>> {
    let store = state.store.lock().expect("store mutex poisoned");
    let mut rows = store
        .audit_records()
        .iter()
        .map(|record| AuditRecordResponse {
            created_at: record.created_at.to_rfc3339(),
            time: record.created_at.format("%H:%M:%S").to_string(),
            actor: record.actor.clone(),
            action: format_audit_action(record.action),
            target: record.target.clone(),
            result: "成功".to_string(),
        })
        .collect::<Vec<_>>();
    rows.reverse();

    Json(rows)
}

async fn releases(State(state): State<AppState>) -> Json<ReleaseListResponse> {
    let store = state.store.lock().expect("store mutex poisoned");
    Json(release_list_response(&store))
}

async fn runtime_status(State(state): State<AppState>) -> Json<RuntimeStatusResponse> {
    let store = state.store.lock().expect("store mutex poisoned");
    Json(runtime_status_response(&store))
}

async fn edge_desired_config(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<EdgeDesiredConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    let package = store
        .latest_config_package_for_edge(&edge_id)
        .cloned()
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;

    Ok(Json(EdgeDesiredConfigResponse {
        edge_id,
        desired_version: package.version.clone(),
        package,
    }))
}

async fn edge_reported_config(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    Json(request): Json<EdgeReportedConfigRequest>,
) -> Result<Json<ReleaseListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let reported_version = request.reported_version;
    let desired_version = request.desired_version;
    let (release_id, reported_node, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let pending = store
            .releases()
            .filter(|release| {
                release.edge_id == edge_id && release.status == ReleaseStatus::Pending
            })
            .cloned()
            .collect::<Vec<_>>();
        let release_id = if let Some(desired_version) = desired_version.as_deref() {
            pending
                .iter()
                .find(|release| release.desired_version == desired_version)
                .map(|release| release.release_id)
                .ok_or_else(|| {
                    error(
                        StatusCode::CONFLICT,
                        "no pending release matches desired version",
                    )
                })?
        } else if let Some(exact) = pending
            .iter()
            .find(|release| release.desired_version == reported_version)
        {
            exact.release_id
        } else if pending.len() == 1 {
            pending[0].release_id
        } else if pending.is_empty() {
            return Err(error(
                StatusCode::NOT_FOUND,
                "missing pending release for edge",
            ));
        } else {
            return Err(error(
                StatusCode::CONFLICT,
                "multiple pending releases require desiredVersion",
            ));
        };

        let updated =
            ReleaseService::mark_reported(&mut store, release_id, reported_version.clone())
                .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing release for edge"))?;
        let reported_node = if updated.status == ReleaseStatus::Applied {
            let mut node = store
                .edge_nodes()
                .find(|node| node.edge_id == edge_id)
                .cloned();
            if let Some(node) = node.as_mut() {
                node.reported_product_version = Some(reported_version.clone());
                store.register_edge(node.clone());
            }
            node
        } else {
            None
        };

        (release_id, reported_node, release_list_response(&store))
    };

    state
        .persist_release_report(release_id, reported_version)
        .await
        .map_err(persistence_error)?;
    if let Some(node) = reported_node {
        state
            .persist_edge_node(node)
            .await
            .map_err(persistence_error)?;
    }

    Ok(Json(response))
}

async fn report_runtime_metrics(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    Json(snapshot): Json<EdgeRuntimeMetricsSnapshot>,
) -> Result<Json<RuntimeStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    if snapshot.edge_id != edge_id {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "runtime metrics edge_id does not match request path",
        ));
    }

    let response = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        store.upsert_runtime_metrics(snapshot.clone());
        runtime_status_response(&store)
    };

    state
        .persist_runtime_metrics(snapshot)
        .await
        .map_err(persistence_error)?;

    Ok(Json(response))
}

async fn report_runtime_event(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
    Json(event): Json<EdgeRuntimeEvent>,
) -> Result<Json<RuntimeStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    if event.edge_id != edge_id {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "runtime event edge_id does not match request path",
        ));
    }

    let response = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        store.push_runtime_event(event.clone());
        runtime_status_response(&store)
    };

    state
        .persist_runtime_event(event)
        .await
        .map_err(persistence_error)?;

    Ok(Json(response))
}

async fn validate_edge_config(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<ManagementActionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    let package = store
        .latest_config_package_for_edge(&edge_id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;

    Ok(Json(config_validation_response(package)))
}

async fn release_diff(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<ManagementActionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let store = state.store.lock().expect("store mutex poisoned");
    let package = store
        .latest_config_package_for_edge(&edge_id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
    let latest_release = store
        .releases()
        .filter(|release| release.edge_id == edge_id)
        .max_by(|left, right| left.desired_version.cmp(&right.desired_version));
    let baseline = latest_release
        .map(|release| release.desired_version.clone())
        .unwrap_or_else(|| "-".to_string());

    Ok(Json(ManagementActionResponse {
        action: "release_diff".to_string(),
        details: vec![
            format!("基线版本 {baseline}"),
            format!("草稿版本 {}", package.version),
            format!("点位 {} 个", package.point_mappings.len()),
            format!("算法 {} 个", package.algorithms.len()),
        ],
        message: "配置差异摘要已生成".to_string(),
        status: "已生成".to_string(),
    }))
}

async fn agent_safety_check(State(state): State<AppState>) -> Json<AgentActionResponse> {
    let store = state.store.lock().expect("store mutex poisoned");
    let edge_count = store.edge_nodes().count();
    let pending_count = store
        .releases()
        .filter(|release| release.status == ReleaseStatus::Pending)
        .count();

    Json(AgentActionResponse {
        action: "agent_safety_check".to_string(),
        details: vec![
            format!("受管边端 {edge_count} 个"),
            format!("待发布版本 {pending_count} 个"),
            "高风险命令仍需人工确认".to_string(),
        ],
        message: "安全策略检查已完成".to_string(),
        status: "已通过".to_string(),
        suggestions: agent_suggestion_list(&store),
    })
}

async fn agent_suggestions(State(state): State<AppState>) -> Json<AgentActionResponse> {
    let store = state.store.lock().expect("store mutex poisoned");
    let suggestions = agent_suggestion_list(&store);

    Json(AgentActionResponse {
        action: "agent_generate_suggestions".to_string(),
        details: vec![format!("建议 {} 条", suggestions.len())],
        message: "Agent 建议已生成".to_string(),
        status: "待确认".to_string(),
        suggestions,
    })
}

async fn agent_provider_status(
    State(state): State<AppState>,
) -> Json<crate::agent_service::AgentProviderStatus> {
    Json(state.agent_service.status())
}

async fn agent_chat(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiPrincipal>,
    Json(request): Json<AgentChatRequest>,
) -> Result<Json<crate::agent_service::AgentChatResult>, (StatusCode, Json<ErrorResponse>)> {
    let message = request.message.trim();
    if message.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "agent message is required"));
    }
    if message.chars().count() > 4_000 {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "agent message exceeds 4000 characters",
        ));
    }

    let operator_id = effective_actor(&principal, &request.operator_id)?;
    let requested_project_id = normalized_optional(request.project_id);
    let requested_edge_id = normalized_optional(request.edge_id);
    let existing_conversation = if let Some(conversation_id) = request.conversation_id {
        let store = state.store.lock().expect("store mutex poisoned");
        let conversation = store
            .agent_conversation(conversation_id)
            .filter(|conversation| conversation.operator_id == operator_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing agent conversation"))?;
        if requested_project_id.is_some()
            && requested_project_id.as_deref() != conversation.project_id.as_deref()
        {
            return Err(error(
                StatusCode::CONFLICT,
                "agent conversation project scope cannot change",
            ));
        }
        if requested_edge_id.is_some()
            && requested_edge_id.as_deref() != conversation.edge_id.as_deref()
        {
            return Err(error(
                StatusCode::CONFLICT,
                "agent conversation edge scope cannot change",
            ));
        }
        Some(conversation)
    } else {
        None
    };

    let context_project_id = existing_conversation
        .as_ref()
        .and_then(|conversation| conversation.project_id.clone())
        .or_else(|| requested_project_id.clone());
    let context_edge_id = existing_conversation
        .as_ref()
        .and_then(|conversation| conversation.edge_id.clone())
        .or_else(|| requested_edge_id.clone());

    let mut context = {
        let store = state.store.lock().expect("store mutex poisoned");
        build_agent_context(
            &store,
            context_project_id.as_deref(),
            context_edge_id.as_deref(),
        )?
    };
    let effective_project_id = context_project_id.clone().or_else(|| {
        context
            .get("edge")
            .and_then(|edge| edge.get("projectId"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    });
    let knowledge = {
        let store = state.store.lock().expect("store mutex poisoned");
        retrieve_agent_knowledge(&store, message, effective_project_id.as_deref())
    };
    context["knowledge"] = serde_json::Value::Array(knowledge);
    if let Some(conversation) = &existing_conversation {
        context["conversationHistory"] = agent_conversation_history(conversation);
    }
    let mut result = state
        .agent_service
        .chat(message, &context)
        .await
        .map_err(agent_provider_error)?;

    let is_new = existing_conversation.is_none();
    let mut conversation = existing_conversation.unwrap_or_else(|| {
        AgentConversation::new(
            effective_project_id,
            context_edge_id,
            operator_id.clone(),
            agent_conversation_title(message),
        )
    });
    conversation.push_message(AgentConversationMessage::new(
        AgentConversationRole::User,
        message,
    ));
    conversation.push_message(
        AgentConversationMessage::new(AgentConversationRole::Assistant, result.message.clone())
            .with_citations(
                result
                    .citations
                    .iter()
                    .map(|citation| AgentConversationCitation {
                        document_id: citation.document_id.clone(),
                        title: citation.title.clone(),
                        source_uri: citation.source_uri.clone(),
                        excerpt: citation.excerpt.clone(),
                    })
                    .collect(),
            ),
    );
    result.conversation_id = Some(conversation.conversation_id.to_string());
    result.conversation_title = Some(conversation.title.clone());

    let audit = is_new.then(|| {
        AuditRecord::by_actor(
            AuditAction::CreateAgentConversation,
            format!("agent-conversation:{}", conversation.conversation_id),
            operator_id,
        )
    });
    if let Some(audit) = &audit {
        state
            .persist_agent_conversation_transition(conversation.clone(), audit.clone())
            .await
            .map_err(persistence_error)?;
    } else {
        state
            .persist_agent_conversation(conversation.clone())
            .await
            .map_err(persistence_error)?;
    }
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.upsert_agent_conversation(conversation);
    if let Some(audit) = audit {
        store.push_audit_record(audit);
    }
    Ok(Json(result))
}

async fn agent_conversations(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiPrincipal>,
    Query(query): Query<AgentConversationQuery>,
) -> Result<Json<Vec<AgentConversation>>, (StatusCode, Json<ErrorResponse>)> {
    let operator_id =
        effective_actor(&principal, query.operator_id.as_deref().unwrap_or_default())?;
    let project_filter_requested = query.project_id.is_some();
    let project_id = normalized_optional(query.project_id);
    let store = state.store.lock().expect("store mutex poisoned");
    let mut conversations = store
        .agent_conversations()
        .filter(|conversation| conversation.operator_id == operator_id)
        .filter(|conversation| {
            if project_filter_requested {
                conversation.project_id.as_deref() == project_id.as_deref()
            } else {
                true
            }
        })
        .cloned()
        .collect::<Vec<_>>();
    conversations.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.conversation_id.cmp(&left.conversation_id))
    });
    Ok(Json(conversations))
}

async fn agent_conversation(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiPrincipal>,
    Path(conversation_id): Path<uuid::Uuid>,
    Query(query): Query<AgentConversationQuery>,
) -> Result<Json<AgentConversation>, (StatusCode, Json<ErrorResponse>)> {
    let operator_id =
        effective_actor(&principal, query.operator_id.as_deref().unwrap_or_default())?;
    let store = state.store.lock().expect("store mutex poisoned");
    let conversation = store
        .agent_conversation(conversation_id)
        .filter(|conversation| conversation.operator_id == operator_id)
        .cloned()
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing agent conversation"))?;
    Ok(Json(conversation))
}

async fn delete_agent_conversation(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiPrincipal>,
    Path(conversation_id): Path<uuid::Uuid>,
    Query(query): Query<DeleteAgentConversationQuery>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let operator_id =
        effective_actor(&principal, query.operator_id.as_deref().unwrap_or_default())?;
    {
        let store = state.store.lock().expect("store mutex poisoned");
        if store
            .agent_conversation(conversation_id)
            .filter(|conversation| conversation.operator_id == operator_id)
            .is_none()
        {
            return Err(error(StatusCode::NOT_FOUND, "missing agent conversation"));
        }
    }
    let audit = AuditRecord::by_actor(
        AuditAction::DeleteAgentConversation,
        format!("agent-conversation:{conversation_id}"),
        operator_id,
    );
    state
        .delete_agent_conversation_transition(conversation_id, audit.clone())
        .await
        .map_err(persistence_error)?;
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.remove_agent_conversation(conversation_id);
    store.push_audit_record(audit);
    Ok(StatusCode::NO_CONTENT)
}

async fn agent_knowledge_documents(
    State(state): State<AppState>,
    Query(query): Query<KnowledgeDocumentQuery>,
) -> Json<Vec<KnowledgeDocument>> {
    let project_id = normalized_optional(query.project_id);
    let store = state.store.lock().expect("store mutex poisoned");
    let mut documents = store
        .knowledge_documents()
        .filter(|document| {
            project_id
                .as_deref()
                .map(|project_id| {
                    document.project_id.is_none()
                        || document.project_id.as_deref() == Some(project_id)
                })
                .unwrap_or(true)
        })
        .cloned()
        .collect::<Vec<_>>();
    documents.sort_by_key(|document| std::cmp::Reverse(document.updated_at));
    Json(documents)
}

async fn create_agent_knowledge_document(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiPrincipal>,
    Json(request): Json<SaveKnowledgeDocumentRequest>,
) -> Result<(StatusCode, Json<KnowledgeDocument>), (StatusCode, Json<ErrorResponse>)> {
    let actor = effective_actor(&principal, &request.actor)?;
    validate_knowledge_document_request(&state, &request, &actor)?;
    let mut document = KnowledgeDocument::new(
        normalized_optional(request.project_id.clone()),
        request.title.trim(),
        request.content.trim(),
        &actor,
    );
    apply_knowledge_document_request(&mut document, &request);
    let audit = AuditRecord::by_actor(
        AuditAction::CreateKnowledgeDocument,
        format!("knowledge:{}", document.document_id),
        &actor,
    );
    state
        .persist_knowledge_document_transition(document.clone(), audit.clone())
        .await
        .map_err(persistence_error)?;
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.upsert_knowledge_document(document.clone());
    store.push_audit_record(audit);
    Ok((StatusCode::CREATED, Json(document)))
}

async fn save_agent_knowledge_document(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiPrincipal>,
    Path(document_id): Path<uuid::Uuid>,
    Json(request): Json<SaveKnowledgeDocumentRequest>,
) -> Result<Json<KnowledgeDocument>, (StatusCode, Json<ErrorResponse>)> {
    let actor = effective_actor(&principal, &request.actor)?;
    validate_knowledge_document_request(&state, &request, &actor)?;
    let mut document = state
        .store
        .lock()
        .expect("store mutex poisoned")
        .knowledge_document(document_id)
        .cloned()
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing knowledge document"))?;
    apply_knowledge_document_request(&mut document, &request);
    document.updated_at = Utc::now();
    let audit = AuditRecord::by_actor(
        AuditAction::UpdateKnowledgeDocument,
        format!("knowledge:{document_id}"),
        &actor,
    );
    state
        .persist_knowledge_document_transition(document.clone(), audit.clone())
        .await
        .map_err(persistence_error)?;
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.upsert_knowledge_document(document.clone());
    store.push_audit_record(audit);
    Ok(Json(document))
}

async fn delete_agent_knowledge_document(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiPrincipal>,
    Path(document_id): Path<uuid::Uuid>,
    Query(query): Query<DeleteKnowledgeDocumentQuery>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    if state
        .store
        .lock()
        .expect("store mutex poisoned")
        .knowledge_document(document_id)
        .is_none()
    {
        return Err(error(StatusCode::NOT_FOUND, "missing knowledge document"));
    }
    let submitted_actor =
        normalized_optional(query.actor).unwrap_or_else(|| "console-operator".to_string());
    let actor = effective_actor(&principal, &submitted_actor)?;
    let audit = AuditRecord::by_actor(
        AuditAction::DeleteKnowledgeDocument,
        format!("knowledge:{document_id}"),
        &actor,
    );
    state
        .delete_knowledge_document_transition(document_id, audit.clone())
        .await
        .map_err(persistence_error)?;
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.remove_knowledge_document(document_id);
    store.push_audit_record(audit);
    Ok(StatusCode::NO_CONTENT)
}

async fn agent_proposals(State(state): State<AppState>) -> Json<Vec<AgentProposal>> {
    let store = state.store.lock().expect("store mutex poisoned");
    let mut proposals = store.agent_proposals().cloned().collect::<Vec<_>>();
    proposals.sort_by_key(|proposal| std::cmp::Reverse(proposal.created_at));
    Json(proposals)
}

async fn create_agent_proposal(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiPrincipal>,
    Json(request): Json<CreateAgentProposalRequest>,
) -> Result<(StatusCode, Json<AgentProposal>), (StatusCode, Json<ErrorResponse>)> {
    let created_by = effective_actor(&principal, &request.created_by)?;
    validate_agent_proposal_request(&state, &request, &created_by)?;
    let mut proposal = AgentProposal::new(
        request.agent_id.trim(),
        request.kind,
        request.title.trim(),
        request.summary.trim(),
        &created_by,
    );
    proposal.project_id = normalized_optional(request.project_id);
    proposal.edge_id = normalized_optional(request.edge_id);
    proposal.payload = request.payload;
    proposal.risk = request.risk;
    let audit = AuditRecord::by_actor(
        AuditAction::CreateAgentProposal,
        format!("agent-proposal:{}", proposal.proposal_id),
        proposal.created_by.clone(),
    );

    state
        .persist_agent_proposal_transition(proposal.clone(), audit.clone())
        .await
        .map_err(persistence_error)?;
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.upsert_agent_proposal(proposal.clone());
    store.push_audit_record(audit);
    Ok((StatusCode::CREATED, Json(proposal)))
}

async fn approve_agent_proposal(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiPrincipal>,
    Path(proposal_id): Path<uuid::Uuid>,
    Json(request): Json<ReviewAgentProposalRequest>,
) -> Result<Json<AgentProposal>, (StatusCode, Json<ErrorResponse>)> {
    review_agent_proposal(
        state,
        principal,
        proposal_id,
        request,
        AgentProposalStatus::Approved,
    )
    .await
}

async fn reject_agent_proposal(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiPrincipal>,
    Path(proposal_id): Path<uuid::Uuid>,
    Json(request): Json<ReviewAgentProposalRequest>,
) -> Result<Json<AgentProposal>, (StatusCode, Json<ErrorResponse>)> {
    review_agent_proposal(
        state,
        principal,
        proposal_id,
        request,
        AgentProposalStatus::Rejected,
    )
    .await
}

async fn review_agent_proposal(
    state: AppState,
    principal: ApiPrincipal,
    proposal_id: uuid::Uuid,
    request: ReviewAgentProposalRequest,
    decision: AgentProposalStatus,
) -> Result<Json<AgentProposal>, (StatusCode, Json<ErrorResponse>)> {
    let reviewer = effective_actor(&principal, &request.reviewer)?;
    let mut proposal = {
        let store = state.store.lock().expect("store mutex poisoned");
        store
            .agent_proposal(proposal_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing agent proposal"))?
    };
    proposal
        .review(decision, &reviewer, request.note)
        .map_err(agent_proposal_review_error)?;
    let action = match decision {
        AgentProposalStatus::Approved => AuditAction::ApproveAgentProposal,
        AgentProposalStatus::Rejected => AuditAction::RejectAgentProposal,
        AgentProposalStatus::PendingReview => unreachable!("review decision is validated"),
    };
    let audit = AuditRecord::by_actor(action, format!("agent-proposal:{proposal_id}"), &reviewer);
    state
        .persist_agent_proposal_transition(proposal.clone(), audit.clone())
        .await
        .map_err(persistence_error)?;
    let mut store = state.store.lock().expect("store mutex poisoned");
    store.upsert_agent_proposal(proposal.clone());
    store.push_audit_record(audit);
    Ok(Json(proposal))
}

fn agent_proposal_review_error(
    review_error: AgentProposalReviewError,
) -> (StatusCode, Json<ErrorResponse>) {
    let status = match review_error {
        AgentProposalReviewError::AlreadyReviewed | AgentProposalReviewError::SelfReview => {
            StatusCode::CONFLICT
        }
        AgentProposalReviewError::ReviewNoteTooLong => StatusCode::PAYLOAD_TOO_LARGE,
        AgentProposalReviewError::InvalidDecision
        | AgentProposalReviewError::MissingReviewer
        | AgentProposalReviewError::ApprovalNoteRequired => StatusCode::BAD_REQUEST,
    };
    error(status, review_error.to_string())
}

async fn save_point_mapping(
    State(state): State<AppState>,
    Path(point_id): Path<String>,
    Json(request): Json<SavePointMappingRequest>,
) -> Result<Json<PointMappingResponse>, (StatusCode, Json<ErrorResponse>)> {
    save_point_mapping_for_edge_id(state, "edge-dev", point_id, request).await
}

async fn save_edge_point_mapping(
    State(state): State<AppState>,
    Path((edge_id, point_id)): Path<(String, String)>,
    Json(request): Json<SavePointMappingRequest>,
) -> Result<Json<PointMappingResponse>, (StatusCode, Json<ErrorResponse>)> {
    save_point_mapping_for_edge_id(state, &edge_id, point_id, request).await
}

async fn delete_edge_point_mapping(
    State(state): State<AppState>,
    Path((edge_id, point_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let package = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;

        if package.collection_tasks.iter().any(|task| {
            task.point_ids
                .iter()
                .any(|candidate| candidate == &point_id)
        }) {
            return Err(error(
                StatusCode::CONFLICT,
                format!("point `{point_id}` is referenced by collection tasks"),
            ));
        }
        if package.data_configs.iter().any(|data_config| {
            data_config
                .points
                .iter()
                .any(|point| point.point_id == point_id)
        }) {
            return Err(error(
                StatusCode::CONFLICT,
                format!("point `{point_id}` is referenced by data configs"),
            ));
        }
        if package.algorithms.iter().any(|algorithm| {
            algorithm
                .dsl
                .inputs
                .iter()
                .any(|input| input.point_id == point_id)
        }) {
            return Err(error(
                StatusCode::CONFLICT,
                format!("point `{point_id}` is referenced by algorithms"),
            ));
        }

        let before = package.point_mappings.len();
        package
            .point_mappings
            .retain(|mapping| mapping.point_id != point_id);
        if package.point_mappings.len() == before {
            return Err(error(StatusCode::NOT_FOUND, "missing point mapping"));
        }

        package.version = next_version(&package.version);
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id);
        package
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn save_point_mapping_for_edge_id(
    state: AppState,
    edge_id: &str,
    point_id: String,
    request: SavePointMappingRequest,
) -> Result<Json<PointMappingResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        package.version = next_version(&package.version);

        let mapping_index = package
            .point_mappings
            .iter()
            .position(|mapping| mapping.point_id == point_id)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing point mapping"))?;

        {
            let mapping = &mut package.point_mappings[mapping_index];
            mapping.address = PointAddress {
                kind: request.address_kind,
                value: request.address_value,
            };
            mapping.interval_ms = request.interval_ms;
            mapping.unit = Some(request.unit);
        }

        let response = point_mapping_response(&package, &package.point_mappings[mapping_index]);
        store.upsert_config_package(package.clone());
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok(Json(response))
}

async fn save_edge_collection_task(
    State(state): State<AppState>,
    Path((edge_id, task_id)): Path<(String, String)>,
    Json(request): Json<SaveCollectionTaskRequest>,
) -> Result<Json<CollectionTaskResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;

        if !package
            .devices
            .iter()
            .any(|device| device.device_id == request.device_id)
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "collection task device missing",
            ));
        }
        if request.point_ids.is_empty() {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "collection task must include at least one point",
            ));
        }
        if let Some(missing_point_id) = request.point_ids.iter().find(|point_id| {
            !package
                .point_mappings
                .iter()
                .any(|mapping| mapping.point_id == **point_id)
        }) {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!("collection task point `{missing_point_id}` missing"),
            ));
        }

        let task_index = package
            .collection_tasks
            .iter()
            .position(|task| task.task_id == task_id)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing collection task"))?;

        package.version = next_version(&package.version);
        {
            let task = &mut package.collection_tasks[task_index];
            task.device_id = request.device_id;
            task.point_ids = request.point_ids;
            task.interval_ms = request.interval_ms.max(100);
            task.enabled = request.enabled;
        }

        let response = collection_task_response(&package, &package.collection_tasks[task_index]);
        store.upsert_config_package(package.clone());
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok(Json(response))
}

async fn delete_edge_collection_task(
    State(state): State<AppState>,
    Path((edge_id, task_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let package = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;

        let before = package.collection_tasks.len();
        package
            .collection_tasks
            .retain(|task| task.task_id != task_id);
        if package.collection_tasks.len() == before {
            return Err(error(StatusCode::NOT_FOUND, "missing collection task"));
        }

        package.version = next_version(&package.version);
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id);
        package
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn save_edge_protocol_connection(
    State(state): State<AppState>,
    Path((edge_id, connection_id)): Path<(String, String)>,
    Json(request): Json<SaveProtocolConnectionRequest>,
) -> Result<Json<ProtocolConnectionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let connection_index = package
            .protocol_connections
            .iter()
            .position(|connection| connection.connection_id == connection_id)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing protocol connection"))?;
        let existing_serial = package.protocol_connections[connection_index]
            .serial
            .clone();
        let (endpoint, serial) = normalize_connection_transport(
            request.protocol_type,
            request.endpoint,
            request.serial,
            existing_serial.as_ref(),
        )?;

        package.version = next_version(&package.version);
        {
            let connection = &mut package.protocol_connections[connection_index];
            connection.protocol = request.protocol_type;
            connection.endpoint = endpoint;
            connection.serial = serial;
        }

        let response = protocol_connection_response(
            &package,
            &package.protocol_connections[connection_index],
            None,
        );
        store.upsert_config_package(package.clone());
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok(Json(response))
}

async fn delete_edge_protocol_connection(
    State(state): State<AppState>,
    Path((edge_id, connection_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let package = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;

        if package
            .point_mappings
            .iter()
            .any(|mapping| mapping.protocol_connection_id == connection_id)
        {
            return Err(error(
                StatusCode::CONFLICT,
                format!("protocol connection `{connection_id}` is referenced by points"),
            ));
        }
        if package
            .data_configs
            .iter()
            .any(|data_config| data_config.protocol_connection_id == connection_id)
        {
            return Err(error(
                StatusCode::CONFLICT,
                format!("protocol connection `{connection_id}` is referenced by data configs"),
            ));
        }

        let before = package.protocol_connections.len();
        package
            .protocol_connections
            .retain(|connection| connection.connection_id != connection_id);
        if package.protocol_connections.len() == before {
            return Err(error(StatusCode::NOT_FOUND, "missing protocol connection"));
        }

        package.version = next_version(&package.version);
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id);
        package
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn save_edge_algorithm(
    State(state): State<AppState>,
    Path((edge_id, algorithm_id)): Path<(String, String)>,
    Json(request): Json<SaveAlgorithmRequest>,
) -> Result<Json<AlgorithmResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (package, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;

        validate_algorithm_dsl(&package, &request.dsl)?;

        let algorithm_index = package
            .algorithms
            .iter()
            .position(|algorithm| algorithm.id == algorithm_id)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing algorithm"))?;

        package.version = next_version(&package.version);
        {
            let algorithm = &mut package.algorithms[algorithm_index];
            algorithm.version = request.version;
            algorithm.kind = request.algorithm_kind;
            algorithm.dsl = request.dsl;
            algorithm.runtime = AlgorithmRuntime::Rule;
            algorithm.inputs = algorithm.inputs();
            algorithm.outputs = algorithm.outputs();
        }

        let response = algorithm_response(&package, &package.algorithms[algorithm_index]);
        store.upsert_config_package(package.clone());
        (package, response)
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok(Json(response))
}

async fn delete_edge_algorithm(
    State(state): State<AppState>,
    Path((edge_id, algorithm_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let package = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let mut package = store
            .latest_config_package_for_edge(&edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;

        if package.data_configs.iter().any(|data_config| {
            data_config
                .algorithm_ids
                .iter()
                .any(|id| id == &algorithm_id)
        }) {
            return Err(error(
                StatusCode::CONFLICT,
                format!("algorithm `{algorithm_id}` is referenced by data configs"),
            ));
        }

        let before = package.algorithms.len();
        package
            .algorithms
            .retain(|algorithm| algorithm.id != algorithm_id);
        if package.algorithms.len() == before {
            return Err(error(StatusCode::NOT_FOUND, "missing algorithm"));
        }

        package.version = next_version(&package.version);
        store.upsert_config_package(package.clone());
        store.push_audit(AuditAction::UpdateConfig, edge_id);
        package
    };

    state
        .persist_config_package(package)
        .await
        .map_err(persistence_error)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn publish_latest_release(
    State(state): State<AppState>,
) -> Result<Json<ReleaseListResponse>, (StatusCode, Json<ErrorResponse>)> {
    publish_latest_release_for_edge_id(state, "edge-dev").await
}

async fn publish_latest_release_for_edge(
    State(state): State<AppState>,
    Path(edge_id): Path<String>,
) -> Result<Json<ReleaseListResponse>, (StatusCode, Json<ErrorResponse>)> {
    publish_latest_release_for_edge_id(state, &edge_id).await
}

async fn publish_latest_release_for_edge_id(
    state: AppState,
    edge_id: &str,
) -> Result<Json<ReleaseListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (release, response) = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        let package = store
            .latest_config_package_for_edge(edge_id)
            .cloned()
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing edge config package"))?;
        let release = ReleaseService::create_release(&mut store, package).map_err(|errors| {
            error(
                StatusCode::BAD_REQUEST,
                errors
                    .into_iter()
                    .map(|error| error.message)
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })?;
        (release, release_list_response(&store))
    };

    state
        .persist_release(release)
        .await
        .map_err(persistence_error)?;

    Ok(Json(response))
}

fn release_list_response(store: &cloud_control::CloudControlStore) -> ReleaseListResponse {
    let mut releases = store.releases().cloned().collect::<Vec<_>>();
    releases.sort_by(|left, right| {
        right
            .desired_version
            .cmp(&left.desired_version)
            .then_with(|| release_status_rank(left.status).cmp(&release_status_rank(right.status)))
            .then_with(|| left.release_id.cmp(&right.release_id))
    });

    let draft_version = store
        .edge_nodes()
        .filter_map(|edge| store.latest_config_package_for_edge(&edge.edge_id))
        .map(|package| package.version.clone())
        .max()
        .or_else(|| {
            releases
                .first()
                .map(|release| release.desired_version.clone())
        })
        .unwrap_or_else(|| "-".to_string());

    ReleaseListResponse {
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
                    ReleaseStatus::Superseded => "已取代",
                }
                .to_string(),
                heartbeat: "18 秒前".to_string(),
            })
            .collect(),
    }
}

fn release_status_rank(status: ReleaseStatus) -> u8 {
    match status {
        ReleaseStatus::Pending => 0,
        ReleaseStatus::Failed => 1,
        ReleaseStatus::Superseded => 2,
        ReleaseStatus::Applied => 3,
    }
}

fn runtime_status_response(store: &cloud_control::CloudControlStore) -> RuntimeStatusResponse {
    let mut edges = store
        .runtime_metrics_snapshots()
        .cloned()
        .collect::<Vec<_>>();
    let now = Utc::now();
    for edge in &mut edges {
        let heartbeat_age_seconds = now
            .signed_duration_since(edge.timestamp)
            .num_seconds()
            .max(0) as u64;
        edge.cloud_sync.last_sync_seconds_ago = heartbeat_age_seconds;
        if heartbeat_age_seconds > 30 {
            edge.health = EdgeHealth::Offline;
            edge.cloud_sync.connected = false;
            for protocol in &mut edge.protocols {
                protocol.connected = false;
            }
        }
    }
    edges.sort_by(|left, right| left.edge_id.cmp(&right.edge_id));

    let online_edges = edges
        .iter()
        .filter(|edge| edge.health != EdgeHealth::Offline)
        .collect::<Vec<_>>();
    let average_collection_latency_ms = if online_edges.is_empty() {
        0
    } else {
        online_edges
            .iter()
            .map(|edge| edge.collection.average_latency_ms)
            .sum::<u64>()
            / online_edges.len() as u64
    };

    RuntimeStatusResponse {
        healthy_edge_count: edges
            .iter()
            .filter(|edge| edge.health == EdgeHealth::Healthy)
            .count(),
        degraded_edge_count: edges
            .iter()
            .filter(|edge| edge.health == EdgeHealth::Degraded)
            .count(),
        critical_edge_count: edges
            .iter()
            .filter(|edge| {
                edge.health == EdgeHealth::Critical || edge.health == EdgeHealth::Offline
            })
            .count(),
        average_collection_latency_ms,
        edges,
        events: store.runtime_events().to_vec(),
    }
}

async fn create_release(
    State(state): State<AppState>,
    Json(package): Json<EdgeConfigPackage>,
) -> Result<(StatusCode, Json<ReleaseResponse>), (StatusCode, Json<ErrorResponse>)> {
    let package_for_persist = package.clone();
    let release = {
        let mut store = state.store.lock().expect("store mutex poisoned");
        ReleaseService::create_release(&mut store, package).map_err(|errors| {
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
        })?
    };

    state
        .persist_config_package(package_for_persist)
        .await
        .map_err(persistence_error)?;
    state
        .persist_release(release.clone())
        .await
        .map_err(persistence_error)?;

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
#[serde(rename_all = "camelCase")]
pub struct EdgeNodeResponse {
    pub edge_id: String,
    pub display_name: String,
    pub site: String,
    pub runtime_id: String,
    pub status: String,
    pub resources: String,
    pub heartbeat: String,
    pub capabilities: Vec<String>,
    pub project_id: Option<String>,
    pub product_id: Option<String>,
    pub desired_product_version: Option<String>,
    pub reported_product_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeNodesQuery {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeNodesPageResponse {
    pub items: Vec<EdgeNodeResponse>,
    pub page: usize,
    pub page_size: usize,
    pub total: usize,
    pub total_pages: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateEdgeNodeRequest {
    pub display_name: Option<String>,
    pub product_id: Option<String>,
    pub project_id: Option<String>,
    pub site: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindEdgeProductRequest {
    pub project_id: String,
    pub product_id: String,
    pub desired_version: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeAccessTokenResponse {
    pub access_token: String,
    pub created_at: chrono::DateTime<Utc>,
    pub credential_id: String,
    pub edge_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceModelResponse {
    pub device_type: String,
    pub version: String,
    pub telemetry: Vec<TelemetryModelResponse>,
    pub command_count: usize,
    pub event_count: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDeviceModelRequest {
    pub device_type: String,
    pub version: String,
    pub telemetry: Vec<CreateTelemetryModelRequest>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDeviceModelRequest {
    pub version: String,
    pub telemetry: Vec<CreateTelemetryModelRequest>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTelemetryModelRequest {
    pub telemetry_id: String,
    pub value_type: String,
    pub unit: Option<String>,
    pub range: Option<String>,
    pub description: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagementActionResponse {
    pub action: String,
    pub details: Vec<String>,
    pub message: String,
    pub status: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSuggestionResponse {
    pub title: String,
    pub detail: String,
    pub state: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentActionResponse {
    pub action: String,
    pub details: Vec<String>,
    pub message: String,
    pub status: String,
    pub suggestions: Vec<AgentSuggestionResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryModelResponse {
    pub telemetry_id: String,
    pub name: String,
    pub value_type: String,
    pub unit: String,
    pub range: String,
    pub description: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolConnectionResponse {
    pub edge_id: String,
    pub connection_id: String,
    pub protocol_type: ProtocolType,
    pub protocol: String,
    pub endpoint: String,
    pub serial: Option<SerialConnectionSettingsDto>,
    pub status: String,
    pub policy: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionTaskResponse {
    pub edge_id: String,
    pub task_id: String,
    pub device_id: String,
    pub point_ids: Vec<String>,
    pub point_list: String,
    pub interval_ms: u64,
    pub interval: String,
    pub enabled: bool,
    pub status: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataConfigResponse {
    pub edge_id: String,
    pub config_id: String,
    pub name: String,
    pub enabled: bool,
    pub device_id: String,
    pub protocol_connection_id: String,
    pub collection: DataConfigCollectionDto,
    pub points: Vec<DataConfigPointDto>,
    pub algorithm_ids: Vec<String>,
    pub visual_graph: DataConfigVisualGraphDto,
    pub publish: DataConfigPublishDto,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataConfigCollectionDto {
    pub period_ms: u64,
    pub timeout_ms: u64,
    pub retry_count: u32,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataConfigPointDto {
    pub point_id: String,
    pub semantic_id: String,
    pub address_kind: String,
    pub address_value: String,
    pub value_type: String,
    pub unit: Option<String>,
    pub json_field: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataConfigPublishDto {
    pub sink_id: String,
    pub topic_template: String,
    pub qos: u8,
    pub payload: DataConfigPayloadDto,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataConfigPayloadDto {
    pub mode: String,
    pub timestamp_field: String,
    pub include_quality: bool,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataConfigVisualGraphDto {
    #[serde(default)]
    pub nodes: Vec<DataConfigGraphNodeDto>,
    #[serde(default)]
    pub edges: Vec<DataConfigGraphEdgeDto>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataConfigGraphNodeDto {
    pub node_id: String,
    pub kind: String,
    pub label: String,
    pub ref_id: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, serde_json::Value>,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataConfigGraphEdgeDto {
    pub edge_id: String,
    pub from: String,
    pub from_port: Option<String>,
    pub to: String,
    pub to_port: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlgorithmResponse {
    pub edge_id: String,
    pub algorithm_id: String,
    pub version: String,
    pub algorithm_kind: AlgorithmKind,
    pub dsl: AlgorithmDsl,
    pub runtime: AlgorithmRuntime,
    pub kind: String,
    pub input_ids: Vec<String>,
    pub output_ids: Vec<String>,
    pub inputs: String,
    pub outputs: String,
    pub execution: String,
    pub validation: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRecordResponse {
    pub created_at: String,
    pub time: String,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub result: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatusResponse {
    pub healthy_edge_count: usize,
    pub degraded_edge_count: usize,
    pub critical_edge_count: usize,
    pub average_collection_latency_ms: u64,
    pub edges: Vec<EdgeRuntimeMetricsSnapshot>,
    pub events: Vec<EdgeRuntimeEvent>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MqttUplinkResponse {
    pub sink_id: String,
    pub broker: String,
    pub client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_ca_path: Option<String>,
    pub topic_template: String,
    pub qos: u8,
    pub batch_size: u32,
    pub flush_interval_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveMqttUplinkRequest {
    pub sink_id: String,
    pub broker: String,
    pub client_id: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password_env: Option<String>,
    #[serde(default)]
    pub tls_ca_path: Option<String>,
    pub topic_template: String,
    pub qos: u8,
    pub batch_size: u32,
    pub flush_interval_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDiscoveryRequest {
    pub connection_id: String,
    pub address_range: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryReportResponse {
    pub job_id: String,
    pub protocol_connection_id: String,
    pub discovered_points: Vec<DiscoveredPointResponse>,
    pub suggestions: Vec<PointMappingSuggestionResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredPointResponse {
    pub protocol_connection_id: String,
    pub address: String,
    pub value_type: String,
    pub sample_values: Vec<String>,
    pub confidence: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PointMappingSuggestionResponse {
    pub point_id: String,
    pub device_id: String,
    pub semantic_id: String,
    pub protocol_connection_id: String,
    pub address: String,
    pub value_type: String,
    pub unit: String,
    pub confidence: f64,
    pub evidence: String,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeDesiredConfigResponse {
    pub edge_id: String,
    pub desired_version: String,
    pub package: EdgeConfigPackage,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeReportedConfigRequest {
    #[serde(default)]
    pub desired_version: Option<String>,
    pub reported_version: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePointMappingRequest {
    pub address_kind: String,
    pub address_value: String,
    pub interval_ms: u64,
    pub unit: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePointMappingRequest {
    pub point_id: Option<String>,
    pub device_id: Option<String>,
    pub semantic_id: Option<String>,
    pub connection_id: Option<String>,
    pub address_kind: Option<String>,
    pub address_value: Option<String>,
    pub value_type: Option<String>,
    pub unit: Option<String>,
    pub interval_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveCollectionTaskRequest {
    pub device_id: String,
    pub point_ids: Vec<String>,
    pub interval_ms: u64,
    pub enabled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCollectionTaskRequest {
    pub task_id: Option<String>,
    pub device_id: String,
    pub point_ids: Vec<String>,
    pub interval_ms: u64,
    pub enabled: Option<bool>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDataConfigRequest {
    pub config_id: String,
    pub name: String,
    pub enabled: bool,
    pub device_id: String,
    pub protocol_connection_id: String,
    pub collection: DataConfigCollectionDto,
    pub points: Vec<DataConfigPointDto>,
    #[serde(default)]
    pub algorithm_ids: Vec<String>,
    #[serde(default)]
    pub visual_graph: DataConfigVisualGraphDto,
    pub publish: DataConfigPublishDto,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProtocolConnectionRequest {
    pub protocol_type: ProtocolType,
    pub endpoint: Option<String>,
    #[serde(default)]
    pub serial: Option<SerialConnectionSettingsDto>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProtocolConnectionRequest {
    pub protocol_type: Option<ProtocolType>,
    pub endpoint: Option<String>,
    #[serde(default)]
    pub serial: Option<SerialConnectionSettingsDto>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerialConnectionSettingsDto {
    pub port: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub stop_bits: u8,
    pub parity: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAlgorithmRequest {
    pub version: String,
    pub algorithm_kind: AlgorithmKind,
    pub dsl: AlgorithmDsl,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAlgorithmRequest {
    pub algorithm_id: Option<String>,
    pub version: Option<String>,
    pub algorithm_kind: Option<AlgorithmKind>,
    pub dsl: Option<AlgorithmDsl>,
    #[serde(default)]
    pub input_ids: Vec<String>,
    #[serde(default)]
    pub output_ids: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PointMappingResponse {
    pub edge_id: String,
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

fn device_model_response(model: &DeviceSpec) -> DeviceModelResponse {
    DeviceModelResponse {
        device_type: model.device_type.clone(),
        version: model.version.clone(),
        telemetry: model
            .telemetry
            .iter()
            .map(|telemetry| TelemetryModelResponse {
                telemetry_id: telemetry.id.clone(),
                name: telemetry.id.clone(),
                value_type: format_telemetry_type(telemetry.value_type),
                unit: telemetry.unit.clone().unwrap_or_else(|| "-".to_string()),
                range: telemetry
                    .range
                    .as_ref()
                    .map(|range| format!("{}-{}", range.min, range.max))
                    .unwrap_or_else(|| "-".to_string()),
                description: telemetry
                    .description
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            })
            .collect(),
        command_count: model.commands.len(),
        event_count: model.events.len(),
    }
}

fn build_device_model_from_request(
    request: CreateDeviceModelRequest,
) -> Result<DeviceSpec, (StatusCode, Json<ErrorResponse>)> {
    build_device_model(request.device_type, request.version, request.telemetry)
}

fn build_device_model(
    device_type: String,
    version: String,
    telemetry_request: Vec<CreateTelemetryModelRequest>,
) -> Result<DeviceSpec, (StatusCode, Json<ErrorResponse>)> {
    let device_type = non_empty_field(device_type, "deviceType")?;
    let version = non_empty_field(version, "version")?;
    if telemetry_request.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "device model requires at least one telemetry point",
        ));
    }

    let mut telemetry = Vec::with_capacity(telemetry_request.len());
    for point in telemetry_request {
        let telemetry_id = non_empty_field(point.telemetry_id, "telemetryId")?;
        let value_type = parse_telemetry_type(&point.value_type)?;
        let mut telemetry_point = TelemetryPoint::new(telemetry_id, value_type);
        if let Some(unit) = non_empty_optional(point.unit) {
            telemetry_point = telemetry_point.with_unit(unit);
        }
        if let Some(range) = non_empty_optional(point.range) {
            telemetry_point = telemetry_point.with_range(parse_number_range(&range)?);
        }
        if let Some(description) = non_empty_optional(point.description) {
            telemetry_point = telemetry_point.with_description(description);
        }
        telemetry.push(telemetry_point);
    }

    Ok(DeviceSpec::new(device_type, version).with_telemetry(telemetry))
}

fn build_point_mapping_from_create_request(
    package: &EdgeConfigPackage,
    request: CreatePointMappingRequest,
) -> Result<TelemetryPointMapping, (StatusCode, Json<ErrorResponse>)> {
    let point_id = non_empty_optional(request.point_id).unwrap_or_else(|| next_point_id(package));
    if package
        .point_mappings
        .iter()
        .any(|mapping| mapping.point_id == point_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("point mapping `{point_id}` already exists"),
        ));
    }

    let device_id = non_empty_optional(request.device_id)
        .or_else(|| {
            package
                .devices
                .first()
                .map(|device| device.device_id.clone())
        })
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "point mapping requires a device"))?;
    if !package
        .devices
        .iter()
        .any(|device| device.device_id == device_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("point mapping device `{device_id}` missing"),
        ));
    }

    let connection_id = non_empty_optional(request.connection_id)
        .or_else(|| {
            package
                .protocol_connections
                .first()
                .map(|connection| connection.connection_id.clone())
        })
        .ok_or_else(|| {
            error(
                StatusCode::BAD_REQUEST,
                "point mapping requires a protocol connection",
            )
        })?;
    if !package
        .protocol_connections
        .iter()
        .any(|connection| connection.connection_id == connection_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("point mapping connection `{connection_id}` missing"),
        ));
    }

    let semantic_id = non_empty_optional(request.semantic_id).unwrap_or_else(|| point_id.clone());
    let address = PointAddress {
        kind: non_empty_optional(request.address_kind).unwrap_or_else(|| "simulated".to_string()),
        value: non_empty_optional(request.address_value).unwrap_or_else(|| point_id.clone()),
    };
    let value_type = request
        .value_type
        .as_deref()
        .map(parse_telemetry_type)
        .transpose()?
        .unwrap_or(TelemetryType::Float);

    Ok(TelemetryPointMapping::new(
        point_id,
        device_id,
        semantic_id,
        connection_id,
        address,
        value_type,
    )
    .with_unit(non_empty_optional(request.unit).unwrap_or_else(|| "-".to_string()))
    .with_interval_ms(request.interval_ms.unwrap_or(1000).max(100)))
}

fn build_collection_task_from_create_request(
    package: &EdgeConfigPackage,
    request: CreateCollectionTaskRequest,
) -> Result<CollectionTask, (StatusCode, Json<ErrorResponse>)> {
    if !package
        .devices
        .iter()
        .any(|device| device.device_id == request.device_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "collection task device missing",
        ));
    }
    if request.point_ids.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "collection task must include at least one point",
        ));
    }
    if let Some(missing_point_id) = request.point_ids.iter().find(|point_id| {
        !package
            .point_mappings
            .iter()
            .any(|mapping| mapping.point_id == **point_id)
    }) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("collection task point `{missing_point_id}` missing"),
        ));
    }

    let task_id = non_empty_optional(request.task_id).unwrap_or_else(|| next_task_id(package));
    if package
        .collection_tasks
        .iter()
        .any(|task| task.task_id == task_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("collection task `{task_id}` already exists"),
        ));
    }

    let mut task = CollectionTask::interval(
        task_id,
        request.device_id,
        request.point_ids,
        request.interval_ms.max(100),
    );
    task.enabled = request.enabled.unwrap_or(true);
    Ok(task)
}

fn build_data_config_from_request(
    package: &EdgeConfigPackage,
    existing_config_id: Option<&str>,
    request: SaveDataConfigRequest,
) -> Result<DataConfig, (StatusCode, Json<ErrorResponse>)> {
    let config_id = non_empty_field(request.config_id, "configId")?;
    if let Some(existing_config_id) = existing_config_id {
        if config_id != existing_config_id
            && package
                .data_configs
                .iter()
                .any(|candidate| candidate.config_id == config_id)
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!("data config `{config_id}` already exists"),
            ));
        }
    }

    let device_id = non_empty_field(request.device_id, "deviceId")?;
    if !package
        .devices
        .iter()
        .any(|device| device.device_id == device_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("data config device `{device_id}` missing"),
        ));
    }

    let protocol_connection_id =
        non_empty_field(request.protocol_connection_id, "protocolConnectionId")?;
    if !package
        .protocol_connections
        .iter()
        .any(|connection| connection.connection_id == protocol_connection_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("data config connection `{protocol_connection_id}` missing"),
        ));
    }

    if !package
        .mqtt_uplinks
        .iter()
        .any(|uplink| uplink.sink_id == request.publish.sink_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!(
                "data config mqtt sink `{}` missing",
                request.publish.sink_id
            ),
        ));
    }
    if request.points.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "data config must include at least one point",
        ));
    }
    let mut algorithm_ids = Vec::new();
    for algorithm_id in request.algorithm_ids {
        let algorithm_id = non_empty_field(algorithm_id, "algorithmIds")?;
        if !package
            .algorithms
            .iter()
            .any(|algorithm| algorithm.id == algorithm_id)
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!("data config algorithm `{algorithm_id}` missing"),
            ));
        }
        if !algorithm_ids.contains(&algorithm_id) {
            algorithm_ids.push(algorithm_id);
        }
    }

    let collection = DataConfigCollection {
        period_ms: request.collection.period_ms.max(100),
        timeout_ms: request.collection.timeout_ms.max(1),
        retry_count: request.collection.retry_count,
    };
    let publish = DataConfigPublish {
        sink_id: non_empty_field(request.publish.sink_id, "sinkId")?,
        topic_template: non_empty_field(request.publish.topic_template, "topicTemplate")?,
        qos: request.publish.qos.min(2),
        payload: DataConfigPayload {
            mode: parse_data_config_payload_mode(&request.publish.payload.mode)?,
            timestamp_field: non_empty_field(
                request.publish.payload.timestamp_field,
                "timestampField",
            )?,
            include_quality: request.publish.payload.include_quality,
        },
    };
    let mut data_config = DataConfig::new(
        config_id,
        non_empty_field(request.name, "name")?,
        device_id,
        protocol_connection_id,
        collection,
        publish,
    );
    data_config.enabled = request.enabled;
    data_config.algorithm_ids = algorithm_ids;
    data_config.visual_graph = data_config_visual_graph_from_request(request.visual_graph)?;

    let mut json_fields = BTreeSet::new();
    for point in request.points {
        let point_id = non_empty_field(point.point_id, "pointId")?;
        if !package
            .point_mappings
            .iter()
            .any(|mapping| mapping.point_id == point_id)
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!("data config point `{point_id}` missing"),
            ));
        }
        let json_field = non_empty_field(point.json_field, "jsonField")?;
        if !json_fields.insert(json_field.clone()) {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!("data config json field `{json_field}` duplicated"),
            ));
        }
        let mut data_point = DataConfigPoint::new(
            point_id,
            non_empty_field(point.semantic_id, "semanticId")?,
            PointAddress {
                kind: non_empty_field(point.address_kind, "addressKind")?,
                value: non_empty_field(point.address_value, "addressValue")?,
            },
            parse_telemetry_type(&point.value_type)?,
            json_field,
        );
        if let Some(unit) = non_empty_optional(point.unit) {
            data_point = data_point.with_unit(unit);
        }
        data_config = data_config.with_point(data_point);
    }

    validate_data_config_algorithm_bindings(package, &data_config)?;
    validate_data_config_visual_graph(&data_config)
        .map_err(|cause| error(StatusCode::BAD_REQUEST, cause))?;

    Ok(data_config)
}

fn validate_data_config_algorithm_bindings(
    package: &EdgeConfigPackage,
    data_config: &DataConfig,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if data_config.algorithm_ids.is_empty() {
        return Ok(());
    }

    let point_ids = data_config
        .points
        .iter()
        .map(|point| point.point_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut json_fields = data_config
        .points
        .iter()
        .map(|point| point.json_field.clone())
        .collect::<BTreeSet<_>>();

    for algorithm_id in &data_config.algorithm_ids {
        let algorithm = package
            .algorithms
            .iter()
            .find(|algorithm| algorithm.id == *algorithm_id)
            .ok_or_else(|| {
                error(
                    StatusCode::BAD_REQUEST,
                    format!("data config algorithm `{algorithm_id}` missing"),
                )
            })?;

        for input_id in algorithm.inputs() {
            if !point_ids.contains(input_id.as_str()) {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    format!(
                        "data config algorithm `{algorithm_id}` input point `{input_id}` is not included in data config points"
                    ),
                ));
            }
        }

        for output_field in algorithm_output_json_fields(algorithm) {
            if !json_fields.insert(output_field.clone()) {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    format!(
                        "data config algorithm `{algorithm_id}` output json field `{output_field}` conflicts with another payload field"
                    ),
                ));
            }
        }
    }

    Ok(())
}

fn algorithm_output_json_fields(algorithm: &AlgorithmSpec) -> Vec<String> {
    if algorithm.dsl.outputs.is_empty() {
        return algorithm
            .outputs()
            .into_iter()
            .map(|point_id| data_config_json_field_from_point_id(&point_id))
            .collect();
    }

    algorithm
        .dsl
        .outputs
        .iter()
        .map(|output| {
            if output.name.trim().is_empty() {
                data_config_json_field_from_point_id(&output.point_id)
            } else {
                output.name.clone()
            }
        })
        .collect()
}

fn data_config_json_field_from_point_id(point_id: &str) -> String {
    point_id
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() {
                value
            } else {
                '_'
            }
        })
        .collect()
}

fn data_config_visual_graph_from_request(
    graph: DataConfigVisualGraphDto,
) -> Result<DataConfigVisualGraph, (StatusCode, Json<ErrorResponse>)> {
    let mut nodes = Vec::new();
    for node in graph.nodes {
        nodes.push(DataConfigGraphNode {
            node_id: non_empty_field(node.node_id, "visualGraph.nodes.nodeId")?,
            kind: parse_data_config_graph_node_kind(&node.kind)?,
            label: non_empty_field(node.label, "visualGraph.nodes.label")?,
            ref_id: non_empty_optional(node.ref_id),
            params: node.params,
            x: node.x,
            y: node.y,
        });
    }
    let mut edges = Vec::new();
    for edge in graph.edges {
        edges.push(DataConfigGraphEdge {
            edge_id: non_empty_field(edge.edge_id, "visualGraph.edges.edgeId")?,
            from: non_empty_field(edge.from, "visualGraph.edges.from")?,
            from_port: non_empty_optional(edge.from_port),
            to: non_empty_field(edge.to, "visualGraph.edges.to")?,
            to_port: non_empty_optional(edge.to_port),
        });
    }
    Ok(DataConfigVisualGraph { nodes, edges })
}

fn parse_data_config_graph_node_kind(
    kind: &str,
) -> Result<DataConfigGraphNodeKind, (StatusCode, Json<ErrorResponse>)> {
    match kind {
        "point" | "Point" => Ok(DataConfigGraphNodeKind::Point),
        "algorithm" | "Algorithm" => Ok(DataConfigGraphNodeKind::Algorithm),
        "json" | "Json" | "JSON" => Ok(DataConfigGraphNodeKind::Json),
        "mqtt" | "Mqtt" | "MQTT" => Ok(DataConfigGraphNodeKind::Mqtt),
        _ => Err(error(
            StatusCode::BAD_REQUEST,
            format!("unsupported data config graph node kind `{kind}`"),
        )),
    }
}

fn build_algorithm_from_create_request(
    package: &EdgeConfigPackage,
    request: CreateAlgorithmRequest,
) -> Result<AlgorithmSpec, (StatusCode, Json<ErrorResponse>)> {
    let algorithm_kind = request
        .algorithm_kind
        .unwrap_or(AlgorithmKind::ChangeReport);
    let dsl = request.dsl.unwrap_or_else(|| {
        legacy_algorithm_dsl(
            request.input_ids.clone(),
            request.output_ids.clone(),
            AlgorithmReportMode::OnChange,
        )
    });
    validate_algorithm_dsl(package, &dsl)?;

    let algorithm_id =
        non_empty_optional(request.algorithm_id).unwrap_or_else(|| next_algorithm_id(package));
    if package
        .algorithms
        .iter()
        .any(|algorithm| algorithm.id == algorithm_id)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("algorithm `{algorithm_id}` already exists"),
        ));
    }

    let version = non_empty_optional(request.version).unwrap_or_else(|| "0.1.0".to_string());
    Ok(AlgorithmSpec::dsl(
        algorithm_id,
        version,
        algorithm_kind,
        dsl,
    ))
}

fn legacy_algorithm_dsl(
    input_ids: Vec<String>,
    output_ids: Vec<String>,
    mode: AlgorithmReportMode,
) -> AlgorithmDsl {
    let first_input = input_ids
        .first()
        .cloned()
        .unwrap_or_else(|| "input".to_string());
    let first_output = output_ids
        .first()
        .cloned()
        .unwrap_or_else(|| format!("{first_input}.reported"));
    AlgorithmDsl {
        inputs: input_ids
            .into_iter()
            .enumerate()
            .map(|(index, point_id)| AlgorithmInputBinding::new(format!("p{index}"), point_id))
            .collect(),
        trigger: AlgorithmTrigger::on_sample(),
        steps: vec![AlgorithmStep::change_filter("p0", 0.0)],
        outputs: vec![AlgorithmOutput::virtual_point("p0", first_output)],
        report: AlgorithmReportPolicy::new(mode, "velamq-main"),
    }
}

fn validate_algorithm_dsl(
    package: &EdgeConfigPackage,
    dsl: &AlgorithmDsl,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if dsl.inputs.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "algorithm DSL must include at least one input point",
        ));
    }
    if dsl.outputs.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "algorithm DSL must include at least one output point",
        ));
    }
    if let Some(input) = dsl.inputs.iter().find(|input| {
        let is_device_point = package
            .point_mappings
            .iter()
            .any(|mapping| mapping.point_id == input.point_id);
        let is_algorithm_output = package.algorithms.iter().any(|algorithm| {
            algorithm
                .dsl
                .outputs
                .iter()
                .any(|output| output.point_id == input.point_id)
        });
        !is_device_point && !is_algorithm_output
    }) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("algorithm input point `{}` missing", input.point_id),
        ));
    }
    let aliases = dsl
        .inputs
        .iter()
        .map(|input| input.alias.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for step in &dsl.steps {
        let source = match step {
            AlgorithmStep::ChangeFilter { source, .. }
            | AlgorithmStep::WindowAggregate { source, .. }
            | AlgorithmStep::Scale { source, .. }
            | AlgorithmStep::Clamp { source, .. }
            | AlgorithmStep::RateOfChange { source, .. }
            | AlgorithmStep::Debounce { source, .. }
            | AlgorithmStep::DurationCondition { source, .. }
            | AlgorithmStep::ConditionalRoute { source, .. }
            | AlgorithmStep::ThresholdRule { source, .. } => Some(source.as_str()),
            AlgorithmStep::Expression { .. } => None,
        };
        if let Some(source) = source {
            if !aliases.contains(source) {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    format!("algorithm step references missing input alias `{source}`"),
                ));
            }
        }
    }
    Ok(())
}

fn non_empty_field(
    value: String,
    field: &str,
) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err(error(
            StatusCode::BAD_REQUEST,
            format!("{field} cannot be empty"),
        ))
    } else {
        Ok(value)
    }
}

fn non_empty_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != "-")
}

fn edge_node_response(
    edge: &EdgeNode,
    runtime: Option<&EdgeRuntimeMetricsSnapshot>,
) -> EdgeNodeResponse {
    EdgeNodeResponse {
        edge_id: edge.edge_id.clone(),
        display_name: edge_display_name(edge),
        site: edge.site.clone().unwrap_or_else(|| "-".to_string()),
        runtime_id: runtime
            .map(|snapshot| snapshot.runtime_id.clone())
            .unwrap_or_else(|| "-".to_string()),
        status: edge_status(edge, runtime),
        resources: runtime
            .map(|snapshot| {
                format!(
                    "{} / {} / {}",
                    format_percent(snapshot.system.cpu_percent),
                    format_percent(snapshot.system.memory_percent),
                    format_percent(snapshot.system.disk_percent)
                )
            })
            .unwrap_or_else(|| "-".to_string()),
        heartbeat: runtime
            .map(|snapshot| format!("{} 秒前", snapshot.cloud_sync.last_sync_seconds_ago))
            .unwrap_or_else(|| "-".to_string()),
        capabilities: edge.capabilities.clone(),
        project_id: edge.project_id.clone(),
        product_id: edge.product_id.clone(),
        desired_product_version: edge.desired_product_version.clone(),
        reported_product_version: edge.reported_product_version.clone(),
        access_token: None,
    }
}

fn new_edge_access_credential(edge_id: &str) -> (String, EdgeAccessCredential) {
    let access_token = format!("edge_{}_{}", edge_id, uuid::Uuid::new_v4().simple());
    let digest = Sha256::digest(access_token.as_bytes());
    let token_hash = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let credential = EdgeAccessCredential::new(edge_id, token_hash);
    (access_token, credential)
}

fn materialize_product_config_package(
    store: &cloud_control::CloudControlStore,
    edge_id: &str,
    version: &ProductVersion,
) -> Result<EdgeConfigPackage, (StatusCode, Json<ErrorResponse>)> {
    let default_connection_id = version
        .data_configs
        .first()
        .map(|config| config.protocol_connection_id.clone())
        .or_else(|| {
            version
                .protocol_connections
                .first()
                .map(|connection| connection.connection_id.clone())
        })
        .ok_or_else(|| {
            error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "product version has no protocol connection",
            )
        })?;
    let default_device_id = version
        .data_configs
        .first()
        .map(|config| config.device_id.clone())
        .or_else(|| {
            version
                .devices
                .first()
                .map(|device| device.device_id.clone())
        })
        .unwrap_or_else(|| "device-1".to_string());

    let mut point_mappings = Vec::new();
    for point_set_id in &version.point_set_ids {
        let point_set = store.point_set(point_set_id).ok_or_else(|| {
            error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "product version references a missing point set",
            )
        })?;
        point_mappings.extend(point_set.points.iter().map(|point| {
            let mut mapping = TelemetryPointMapping::new(
                point.point_id.clone(),
                default_device_id.clone(),
                point.semantic_id.clone(),
                default_connection_id.clone(),
                point.address.clone(),
                point.value_type,
            )
            .with_interval_ms(point.interval_ms);
            mapping.unit = point.unit.clone();
            mapping
        }));
    }

    Ok(EdgeConfigPackage {
        edge_id: edge_id.to_string(),
        version: version.version.clone(),
        device_models: version.device_models.clone(),
        devices: version.devices.clone(),
        protocol_connections: version.protocol_connections.clone(),
        mqtt_uplinks: version
            .mqtt_uplinks
            .iter()
            .cloned()
            .map(|mut uplink| {
                uplink.client_id = if uplink.client_id.contains("{edge_id}") {
                    uplink.client_id.replace("{edge_id}", edge_id)
                } else if let Some(suffix) = uplink.client_id.strip_prefix("edge-dev") {
                    format!("{edge_id}{suffix}")
                } else {
                    uplink.client_id
                };
                uplink
            })
            .collect(),
        data_configs: version.data_configs.clone(),
        point_mappings,
        collection_tasks: version.collection_tasks.clone(),
        algorithms: version.algorithms.clone(),
    })
}

fn edge_display_name(edge: &EdgeNode) -> String {
    match edge.display_name.as_str() {
        "" | "新边端注册草稿" => "新边端待确认".to_string(),
        value => value.to_string(),
    }
}

fn edge_status(edge: &EdgeNode, runtime: Option<&EdgeRuntimeMetricsSnapshot>) -> String {
    if edge
        .capabilities
        .iter()
        .any(|capability| capability == "mode:maintenance")
    {
        return "维护中".to_string();
    }

    runtime
        .map(|snapshot| format_health(snapshot.health))
        .unwrap_or_else(|| "未上报".to_string())
}

fn next_edge_id(edges: &[EdgeNode]) -> String {
    let mut next = edges.len() + 1;
    loop {
        let candidate = format!("edge-draft-{next}");
        if edges.iter().all(|edge| edge.edge_id != candidate) {
            return candidate;
        }
        next += 1;
    }
}

fn default_config_edge_id(store: &cloud_control::CloudControlStore) -> Option<String> {
    store.edge_nodes().next().map(|edge| edge.edge_id.clone())
}

fn config_validation_response(package: &EdgeConfigPackage) -> ManagementActionResponse {
    ManagementActionResponse {
        action: "validate_config".to_string(),
        details: vec![
            format!("协议连接 {} 个", package.protocol_connections.len()),
            format!("点位 {} 个", package.point_mappings.len()),
            format!("采集任务 {} 个", package.collection_tasks.len()),
            format!("算法 {} 个", package.algorithms.len()),
        ],
        message: "配置校验已完成".to_string(),
        status: "已通过".to_string(),
    }
}

fn agent_suggestion_list(store: &cloud_control::CloudControlStore) -> Vec<AgentSuggestionResponse> {
    let total_points = store
        .edge_nodes()
        .filter_map(|edge| store.latest_config_package_for_edge(&edge.edge_id))
        .map(|package| package.point_mappings.len())
        .sum::<usize>();
    let pending_count = store
        .releases()
        .filter(|release| release.status == ReleaseStatus::Pending)
        .count();

    vec![
        AgentSuggestionResponse {
            title: "点位补全".to_string(),
            detail: format!("当前已配置 {total_points} 个点位，可按设备模型继续生成缺失映射"),
            state: "生成草稿".to_string(),
        },
        AgentSuggestionResponse {
            title: "发布风险".to_string(),
            detail: format!("当前有 {pending_count} 个待发布版本，建议先单边端灰度"),
            state: "需确认".to_string(),
        },
        AgentSuggestionResponse {
            title: "故障解释".to_string(),
            detail: "协议超时、点位质量和 runtime 事件可共同用于定位采集中断".to_string(),
            state: "可查看".to_string(),
        },
    ]
}

fn build_agent_context(
    store: &cloud_control::CloudControlStore,
    project_id: Option<&str>,
    edge_id: Option<&str>,
) -> Result<serde_json::Value, (StatusCode, Json<ErrorResponse>)> {
    let project_id = project_id.map(str::trim).filter(|value| !value.is_empty());
    let edge_id = edge_id.map(str::trim).filter(|value| !value.is_empty());
    if let Some(project_id) = project_id {
        if store.project(project_id).is_none() {
            return Err(error(
                StatusCode::NOT_FOUND,
                "missing Agent context project",
            ));
        }
    }
    let edge = if let Some(edge_id) = edge_id {
        Some(
            store
                .edge_nodes()
                .find(|edge| edge.edge_id == edge_id)
                .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing Agent context edge"))?,
        )
    } else {
        None
    };
    if let (Some(project_id), Some(edge)) = (project_id, edge) {
        if edge.project_id.as_deref() != Some(project_id) {
            return Err(error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Agent context edge does not belong to project",
            ));
        }
    }

    let package = edge.and_then(|edge| store.latest_config_package_for_edge(&edge.edge_id));
    let metrics = edge.and_then(|edge| store.runtime_metrics(&edge.edge_id));
    let online_count = store
        .runtime_metrics_snapshots()
        .filter(|snapshot| snapshot.health != EdgeHealth::Offline)
        .count();
    let context = serde_json::json!({
        "contextVersion": "edgeops-agent-context/v1",
        "fleet": {
            "edgeCount": store.edge_nodes().count(),
            "onlineCount": online_count,
        },
        "governance": {
            "pendingReleaseCount": store
                .releases()
                .filter(|release| release.status == ReleaseStatus::Pending)
                .count(),
            "pendingProposalCount": store
                .agent_proposals()
                .filter(|proposal| proposal.status == AgentProposalStatus::PendingReview)
                .count(),
        },
        "scope": {
            "projectId": project_id,
            "edgeId": edge_id,
        },
        "edge": edge.map(|edge| serde_json::json!({
            "edgeId": edge.edge_id,
            "displayName": edge.display_name,
            "site": edge.site,
            "projectId": edge.project_id,
            "productId": edge.product_id,
            "desiredProductVersion": edge.desired_product_version,
            "reportedProductVersion": edge.reported_product_version,
        })),
        "runtime": metrics.map(|metrics| serde_json::json!({
            "health": format_health(metrics.health),
            "configVersion": metrics.config_version,
            "cpuPercent": metrics.system.cpu_percent,
            "memoryPercent": metrics.system.memory_percent,
            "diskPercent": metrics.system.disk_percent,
            "collectionSuccessRate": metrics.collection.success_rate,
            "pendingUploads": metrics.cloud_sync.pending_uploads,
        })),
        "configuration": package.map(|package| serde_json::json!({
            "version": package.version,
            "protocolConnectionCount": package.protocol_connections.len(),
            "pointCount": package.point_mappings.len(),
            "collectionTaskCount": package.collection_tasks.len(),
            "algorithmCount": package.algorithms.len(),
            "dataConfigCount": package.data_configs.len(),
            "mqttSinkCount": package.mqtt_uplinks.len(),
        })),
    });
    Ok(context)
}

fn point_mapping_response(
    package: &EdgeConfigPackage,
    mapping: &TelemetryPointMapping,
) -> PointMappingResponse {
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

    PointMappingResponse {
        edge_id: package.edge_id.clone(),
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
    }
}

fn collection_task_response(
    package: &EdgeConfigPackage,
    task: &CollectionTask,
) -> CollectionTaskResponse {
    CollectionTaskResponse {
        edge_id: package.edge_id.clone(),
        task_id: task.task_id.clone(),
        device_id: task.device_id.clone(),
        point_ids: task.point_ids.clone(),
        point_list: task.point_ids.join(", "),
        interval_ms: task.interval_ms,
        interval: format!("{}ms", task.interval_ms),
        enabled: task.enabled,
        status: if task.enabled { "启用" } else { "暂停" }.to_string(),
    }
}

fn data_config_response(
    package: &EdgeConfigPackage,
    data_config: &DataConfig,
) -> DataConfigResponse {
    DataConfigResponse {
        edge_id: package.edge_id.clone(),
        config_id: data_config.config_id.clone(),
        name: data_config.name.clone(),
        enabled: data_config.enabled,
        device_id: data_config.device_id.clone(),
        protocol_connection_id: data_config.protocol_connection_id.clone(),
        collection: DataConfigCollectionDto {
            period_ms: data_config.collection.period_ms,
            timeout_ms: data_config.collection.timeout_ms,
            retry_count: data_config.collection.retry_count,
        },
        algorithm_ids: data_config.algorithm_ids.clone(),
        visual_graph: data_config_visual_graph_response(&data_config.visual_graph),
        points: data_config
            .points
            .iter()
            .map(|point| DataConfigPointDto {
                point_id: point.point_id.clone(),
                semantic_id: point.semantic_id.clone(),
                address_kind: point.address.kind.clone(),
                address_value: point.address.value.clone(),
                value_type: format_telemetry_type(point.value_type),
                unit: point.unit.clone(),
                json_field: point.json_field.clone(),
            })
            .collect(),
        publish: DataConfigPublishDto {
            sink_id: data_config.publish.sink_id.clone(),
            topic_template: data_config.publish.topic_template.clone(),
            qos: data_config.publish.qos,
            payload: DataConfigPayloadDto {
                mode: format_data_config_payload_mode(data_config.publish.payload.mode),
                timestamp_field: data_config.publish.payload.timestamp_field.clone(),
                include_quality: data_config.publish.payload.include_quality,
            },
        },
    }
}

fn data_config_visual_graph_response(graph: &DataConfigVisualGraph) -> DataConfigVisualGraphDto {
    DataConfigVisualGraphDto {
        nodes: graph
            .nodes
            .iter()
            .map(|node| DataConfigGraphNodeDto {
                node_id: node.node_id.clone(),
                kind: format_data_config_graph_node_kind(node.kind).to_string(),
                label: node.label.clone(),
                ref_id: node.ref_id.clone(),
                params: node.params.clone(),
                x: node.x,
                y: node.y,
            })
            .collect(),
        edges: graph
            .edges
            .iter()
            .map(|edge| DataConfigGraphEdgeDto {
                edge_id: edge.edge_id.clone(),
                from: edge.from.clone(),
                from_port: edge.from_port.clone(),
                to: edge.to.clone(),
                to_port: edge.to_port.clone(),
            })
            .collect(),
    }
}

fn format_data_config_graph_node_kind(kind: DataConfigGraphNodeKind) -> &'static str {
    match kind {
        DataConfigGraphNodeKind::Point => "point",
        DataConfigGraphNodeKind::Algorithm => "algorithm",
        DataConfigGraphNodeKind::Json => "json",
        DataConfigGraphNodeKind::Mqtt => "mqtt",
    }
}

fn protocol_connection_response(
    package: &EdgeConfigPackage,
    connection: &ProtocolConnection,
    connected: Option<bool>,
) -> ProtocolConnectionResponse {
    ProtocolConnectionResponse {
        edge_id: package.edge_id.clone(),
        connection_id: connection.connection_id.clone(),
        protocol_type: connection.protocol,
        protocol: format_protocol(connection.protocol),
        endpoint: connection
            .endpoint
            .clone()
            .unwrap_or_else(|| "runtime://simulated".to_string()),
        serial: connection
            .serial
            .as_ref()
            .map(|serial| SerialConnectionSettingsDto {
                port: serial.port.clone(),
                baud_rate: serial.baud_rate,
                data_bits: serial.data_bits,
                stop_bits: serial.stop_bits,
                parity: serial.parity.clone(),
            }),
        status: connected
            .map(|connected| {
                if connected {
                    "启用".to_string()
                } else {
                    "异常".to_string()
                }
            })
            .unwrap_or_else(|| "启用".to_string()),
        policy: connection
            .serial
            .as_ref()
            .map(format_serial_policy)
            .unwrap_or_else(|| "1000ms timeout / 3 retry".to_string()),
    }
}

fn normalize_connection_transport(
    protocol: ProtocolType,
    endpoint: Option<String>,
    requested_serial: Option<SerialConnectionSettingsDto>,
    existing_serial: Option<&SerialConnectionSettings>,
) -> Result<ConnectionTransport, ApiError> {
    let endpoint = endpoint.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    });
    if !is_serial_protocol(protocol) {
        return Ok((endpoint, None));
    }

    let mut serial = if let Some(requested) = requested_serial {
        SerialConnectionSettings {
            port: requested.port.trim().to_string(),
            baud_rate: requested.baud_rate,
            data_bits: requested.data_bits,
            stop_bits: requested.stop_bits,
            parity: requested.parity,
        }
    } else if let Some(existing) = existing_serial {
        existing.clone()
    } else {
        default_serial_settings(protocol, endpoint.clone().unwrap_or_default())
    };

    if let Some(endpoint) = endpoint {
        serial.port = endpoint;
    }
    serial.port = serial.port.trim().to_string();
    if serial.port.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "serial port is required for the selected protocol",
        ));
    }
    if serial.baud_rate == 0 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "baudRate must be greater than zero",
        ));
    }
    if !(5..=8).contains(&serial.data_bits) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "dataBits must be between 5 and 8",
        ));
    }
    if !matches!(serial.stop_bits, 1 | 2) {
        return Err(error(StatusCode::BAD_REQUEST, "stopBits must be 1 or 2"));
    }
    serial.parity = normalize_parity(&serial.parity)?;

    Ok((Some(serial.port.clone()), Some(serial)))
}

fn is_serial_protocol(protocol: ProtocolType) -> bool {
    matches!(
        protocol,
        ProtocolType::ModbusRtu
            | ProtocolType::Dlt645
            | ProtocolType::Iec101
            | ProtocolType::CustomSerial
    )
}

fn default_serial_settings(protocol: ProtocolType, port: String) -> SerialConnectionSettings {
    let baud_rate = if protocol == ProtocolType::Dlt645 {
        2_400
    } else {
        9_600
    };
    let parity = if matches!(protocol, ProtocolType::Dlt645 | ProtocolType::Iec101) {
        "even"
    } else {
        "none"
    };
    SerialConnectionSettings::new(port, baud_rate).with_parity(parity)
}

fn normalize_parity(parity: &str) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    match parity.trim().to_ascii_lowercase().as_str() {
        "n" | "none" => Ok("none".to_string()),
        "e" | "even" => Ok("even".to_string()),
        "o" | "odd" => Ok("odd".to_string()),
        _ => Err(error(
            StatusCode::BAD_REQUEST,
            "parity must be none, even, or odd",
        )),
    }
}

fn format_serial_policy(serial: &SerialConnectionSettings) -> String {
    let parity = match serial.parity.as_str() {
        "even" => "E",
        "odd" => "O",
        _ => "N",
    };
    format!(
        "{} baud · {}{}{}",
        serial.baud_rate, serial.data_bits, parity, serial.stop_bits
    )
}

fn algorithm_response(package: &EdgeConfigPackage, algorithm: &AlgorithmSpec) -> AlgorithmResponse {
    AlgorithmResponse {
        edge_id: package.edge_id.clone(),
        algorithm_id: algorithm.id.clone(),
        version: algorithm.version.clone(),
        algorithm_kind: algorithm.kind,
        dsl: algorithm.dsl.clone(),
        runtime: algorithm.runtime,
        kind: format_algorithm_kind(algorithm.kind),
        input_ids: algorithm.inputs(),
        output_ids: algorithm.outputs(),
        inputs: algorithm.inputs().join(", "),
        outputs: algorithm.outputs().join(", "),
        execution: "边端本地执行".to_string(),
        validation: "已通过".to_string(),
    }
}

fn next_version(version: &str) -> String {
    let Some((prefix, suffix)) = version.rsplit_once('-') else {
        return format!("{version}-001");
    };
    let next = suffix.parse::<u64>().unwrap_or(0) + 1;
    format!("{prefix}-{next:03}")
}

fn next_connection_id(package: &EdgeConfigPackage) -> String {
    let mut next = package.protocol_connections.len() + 1;
    loop {
        let candidate = format!("connection-draft-{next}");
        if package
            .protocol_connections
            .iter()
            .all(|connection| connection.connection_id != candidate)
        {
            return candidate;
        }
        next += 1;
    }
}

fn next_point_id(package: &EdgeConfigPackage) -> String {
    let mut next = package.point_mappings.len() + 1;
    loop {
        let candidate = format!("point-draft-{next}");
        if package
            .point_mappings
            .iter()
            .all(|mapping| mapping.point_id != candidate)
        {
            return candidate;
        }
        next += 1;
    }
}

fn next_task_id(package: &EdgeConfigPackage) -> String {
    let mut next = package.collection_tasks.len() + 1;
    loop {
        let candidate = format!("task-draft-{next}");
        if package
            .collection_tasks
            .iter()
            .all(|task| task.task_id != candidate)
        {
            return candidate;
        }
        next += 1;
    }
}

fn next_algorithm_id(package: &EdgeConfigPackage) -> String {
    let mut next = package.algorithms.len() + 1;
    loop {
        let candidate = format!("algorithm-draft-{next}");
        if package
            .algorithms
            .iter()
            .all(|algorithm| algorithm.id != candidate)
        {
            return candidate;
        }
        next += 1;
    }
}

fn next_device_model_type(package: &EdgeConfigPackage) -> String {
    let mut next = package.device_models.len() + 1;
    loop {
        let candidate = format!("device-model-draft-{next}");
        if package
            .device_models
            .iter()
            .all(|model| model.device_type != candidate)
        {
            return candidate;
        }
        next += 1;
    }
}

fn validate_catalog_id(label: &str, value: &str) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let value = value.trim();
    if value.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("{label} is required"),
        ));
    }
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("{label} contains unsupported characters"),
        ));
    }
    Ok(())
}

fn validate_required_text(
    label: &str,
    value: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if value.trim().is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("{label} is required"),
        ));
    }
    Ok(())
}

fn validate_project_request(
    request: &SaveProjectRequest,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    validate_catalog_id("projectId", &request.project_id)?;
    validate_required_text("name", &request.name)?;
    validate_required_text("environment", &request.environment)?;
    validate_required_text("owner", &request.owner)
}

fn build_project(request: SaveProjectRequest, existing: Option<Project>) -> Project {
    let now = Utc::now();
    Project {
        project_id: request.project_id,
        name: request.name.trim().to_string(),
        environment: request.environment.trim().to_string(),
        owner: request.owner.trim().to_string(),
        description: request.description.trim().to_string(),
        created_at: existing
            .as_ref()
            .map(|project| project.created_at)
            .unwrap_or(now),
        updated_at: now,
    }
}

fn validate_point_set_request(
    request: &SavePointSetRequest,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    validate_catalog_id("pointSetId", &request.point_set_id)?;
    validate_catalog_id("projectId", &request.project_id)?;
    validate_required_text("name", &request.name)?;
    let mut point_ids = BTreeSet::new();
    for point in &request.points {
        validate_catalog_id("pointId", &point.point_id)?;
        validate_required_text("semanticId", &point.semantic_id)?;
        validate_required_text("address.kind", &point.address.kind)?;
        validate_required_text("address.value", &point.address.value)?;
        if request.protocol == ProtocolType::CustomSerial {
            if point.address.kind != "custom_serial_frame" {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    format!(
                        "point {} must use custom_serial_frame address kind",
                        point.point_id
                    ),
                ));
            }
            let spec = serde_json::from_str::<CustomSerialPointSpec>(&point.address.value)
                .map_err(|parse_error| {
                    error(
                        StatusCode::BAD_REQUEST,
                        format!(
                            "point {} custom serial frame is invalid JSON: {parse_error}",
                            point.point_id
                        ),
                    )
                })?;
            validate_custom_serial_point_spec(&spec).map_err(|validation_error| {
                error(
                    StatusCode::BAD_REQUEST,
                    format!(
                        "point {} custom serial frame is invalid: {validation_error}",
                        point.point_id
                    ),
                )
            })?;
        }
        if point.interval_ms == 0 {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!(
                    "point {} intervalMs must be greater than zero",
                    point.point_id
                ),
            ));
        }
        if !point_ids.insert(point.point_id.as_str()) {
            return Err(error(
                StatusCode::CONFLICT,
                "duplicate point id in point set",
            ));
        }
    }
    Ok(())
}

fn build_point_set(request: SavePointSetRequest, existing: Option<PointSet>) -> PointSet {
    let now = Utc::now();
    PointSet {
        point_set_id: request.point_set_id,
        project_id: request.project_id,
        name: request.name.trim().to_string(),
        description: request.description.trim().to_string(),
        protocol: request.protocol,
        points: request.points,
        created_at: existing
            .as_ref()
            .map(|point_set| point_set.created_at)
            .unwrap_or(now),
        updated_at: now,
    }
}

fn validate_product_request(
    request: &SaveProductRequest,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    validate_catalog_id("productId", &request.product_id)?;
    validate_catalog_id("projectId", &request.project_id)?;
    validate_required_text("name", &request.name)?;
    validate_required_text("productType", &request.product_type)
}

fn build_product(request: SaveProductRequest, existing: Option<Product>) -> Product {
    let now = Utc::now();
    Product {
        product_id: request.product_id,
        project_id: request.project_id,
        name: request.name.trim().to_string(),
        product_type: request.product_type.trim().to_string(),
        description: request.description.trim().to_string(),
        latest_version: existing
            .as_ref()
            .and_then(|product| product.latest_version.clone()),
        created_at: existing
            .as_ref()
            .map(|product| product.created_at)
            .unwrap_or(now),
        updated_at: now,
    }
}

fn ensure_project_exists(
    store: &cloud_control::CloudControlStore,
    project_id: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if store.project(project_id).is_none() {
        return Err(error(StatusCode::NOT_FOUND, "missing project"));
    }
    Ok(())
}

fn validate_product_version_request(
    product_id: &str,
    request: &SaveProductVersionRequest,
    state: &AppState,
    updating: bool,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    validate_catalog_id("productId", product_id)?;
    validate_catalog_id("version", &request.version)?;
    let store = state.store.lock().expect("store mutex poisoned");
    let product = store
        .product(product_id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "missing product"))?;
    if !updating
        && store
            .product_version(product_id, &request.version)
            .is_some()
    {
        return Err(error(
            StatusCode::CONFLICT,
            "product version already exists",
        ));
    }

    let mut point_set_ids = BTreeSet::new();
    for point_set_id in &request.point_set_ids {
        if !point_set_ids.insert(point_set_id.as_str()) {
            return Err(error(
                StatusCode::CONFLICT,
                "duplicate point set reference in product version",
            ));
        }
        let point_set = store
            .point_set(point_set_id)
            .ok_or_else(|| error(StatusCode::BAD_REQUEST, "missing product point set"))?;
        if point_set.project_id != product.project_id {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "product version cannot reference a point set from another project",
            ));
        }
    }

    ensure_unique_ids(
        request
            .protocol_connections
            .iter()
            .map(|connection| connection.connection_id.as_str()),
        "protocol connection",
    )?;
    ensure_unique_ids(
        request
            .collection_tasks
            .iter()
            .map(|task| task.task_id.as_str()),
        "collection task",
    )?;
    ensure_unique_ids(
        request
            .algorithms
            .iter()
            .map(|algorithm| algorithm.id.as_str()),
        "algorithm",
    )?;
    ensure_unique_ids(
        request
            .data_configs
            .iter()
            .map(|config| config.config_id.as_str()),
        "data config",
    )?;
    ensure_unique_ids(
        request
            .mqtt_uplinks
            .iter()
            .map(|uplink| uplink.sink_id.as_str()),
        "MQTT sink",
    )?;
    for uplink in &request.mqtt_uplinks {
        if uplink.username.is_some() != uplink.password_env.is_some() {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!(
                    "MQTT sink {} username and password environment reference must be configured together",
                    uplink.sink_id
                ),
            ));
        }
        if uplink
            .username
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
            || uplink
                .password_env
                .as_deref()
                .is_some_and(|value| value.trim().is_empty())
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!(
                    "MQTT sink {} credential references must not be empty",
                    uplink.sink_id
                ),
            ));
        }
        if uplink
            .tls_ca_path
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!("MQTT sink {} TLS CA path must not be empty", uplink.sink_id),
            ));
        }
        if uplink.tls_ca_path.is_some()
            && !matches!(
                uplink
                    .broker
                    .split_once("://")
                    .map(|(scheme, _)| scheme.to_ascii_lowercase())
                    .as_deref(),
                Some("mqtts" | "ssl")
            )
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!(
                    "MQTT sink {} TLS CA path requires an mqtts:// broker",
                    uplink.sink_id
                ),
            ));
        }
    }

    let connection_ids = request
        .protocol_connections
        .iter()
        .map(|connection| connection.connection_id.as_str())
        .collect::<BTreeSet<_>>();
    let algorithm_ids = request
        .algorithms
        .iter()
        .map(|algorithm| algorithm.id.as_str())
        .collect::<BTreeSet<_>>();
    let sink_ids = request
        .mqtt_uplinks
        .iter()
        .map(|uplink| uplink.sink_id.as_str())
        .collect::<BTreeSet<_>>();
    for config in &request.data_configs {
        if !connection_ids.contains(config.protocol_connection_id.as_str()) {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!(
                    "data config {} references missing protocol connection {}",
                    config.config_id, config.protocol_connection_id
                ),
            ));
        }
        if !sink_ids.contains(config.publish.sink_id.as_str()) {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!(
                    "data config {} references missing MQTT sink {}",
                    config.config_id, config.publish.sink_id
                ),
            ));
        }
        if let Some(missing) = config
            .algorithm_ids
            .iter()
            .find(|algorithm_id| !algorithm_ids.contains(algorithm_id.as_str()))
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!(
                    "data config {} references missing algorithm {}",
                    config.config_id, missing
                ),
            ));
        }
        validate_data_config_visual_graph(config)
            .map_err(|cause| error(StatusCode::UNPROCESSABLE_ENTITY, cause))?;
    }
    Ok(())
}

fn ensure_unique_ids<'a>(
    ids: impl IntoIterator<Item = &'a str>,
    label: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let mut seen = BTreeSet::new();
    for id in ids {
        validate_catalog_id(label, id)?;
        if !seen.insert(id) {
            return Err(error(StatusCode::CONFLICT, format!("duplicate {label} id")));
        }
    }
    Ok(())
}

fn validate_publishable_product_version(
    version: &ProductVersion,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if version.point_set_ids.is_empty() {
        return Err(error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "product version requires at least one point set before publishing",
        ));
    }
    if version.protocol_connections.is_empty() {
        return Err(error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "product version requires a protocol connection before publishing",
        ));
    }
    if version.data_configs.is_empty() {
        return Err(error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "product version requires a data config before publishing",
        ));
    }
    if version.mqtt_uplinks.is_empty() {
        return Err(error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "product version requires an MQTT sink before publishing",
        ));
    }
    Ok(())
}

fn build_product_version(
    product_id: String,
    request: SaveProductVersionRequest,
    existing: Option<ProductVersion>,
) -> ProductVersion {
    ProductVersion {
        product_id,
        version: request.version,
        status: ProductVersionStatus::Draft,
        point_set_ids: request.point_set_ids,
        device_models: request.device_models,
        devices: request.devices,
        protocol_connections: request.protocol_connections,
        collection_tasks: request.collection_tasks,
        algorithms: request.algorithms,
        data_configs: request.data_configs,
        mqtt_uplinks: request.mqtt_uplinks,
        created_at: existing
            .map(|version| version.created_at)
            .unwrap_or_else(Utc::now),
    }
}

fn validate_agent_proposal_request(
    state: &AppState,
    request: &CreateAgentProposalRequest,
    created_by: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    for (label, value) in [
        ("agent id", request.agent_id.as_str()),
        ("title", request.title.as_str()),
        ("summary", request.summary.as_str()),
        ("created by", created_by),
    ] {
        if value.trim().is_empty() {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!("{label} is required"),
            ));
        }
    }

    let store = state.store.lock().expect("store mutex poisoned");
    if let Some(project_id) = request.project_id.as_deref().map(str::trim) {
        if !project_id.is_empty() && store.project(project_id).is_none() {
            return Err(error(StatusCode::NOT_FOUND, "missing proposal project"));
        }
    }
    if let Some(edge_id) = request.edge_id.as_deref().map(str::trim) {
        if !edge_id.is_empty() && !store.edge_nodes().any(|edge| edge.edge_id == edge_id) {
            return Err(error(StatusCode::NOT_FOUND, "missing proposal edge"));
        }
    }
    Ok(())
}

fn default_true() -> bool {
    true
}

fn default_agent_operator() -> String {
    "console-operator".to_string()
}

fn normalized_required_actor(actor: &str) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    let actor = actor.trim();
    if actor.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "agent operator is required"));
    }
    if actor.chars().count() > 120 {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "agent operator exceeds 120 characters",
        ));
    }
    Ok(actor.to_string())
}

fn effective_actor(
    principal: &ApiPrincipal,
    submitted_actor: &str,
) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    if principal.authentication_enabled {
        normalized_required_actor(&principal.subject)
    } else {
        normalized_required_actor(submitted_actor)
    }
}

fn validate_knowledge_document_request(
    state: &AppState,
    request: &SaveKnowledgeDocumentRequest,
    actor: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let title = request.title.trim();
    let content = request.content.trim();
    if title.is_empty() || content.is_empty() || actor.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "knowledge title, content and actor are required",
        ));
    }
    if title.chars().count() > 120
        || content.chars().count() > 100_000
        || actor.chars().count() > 120
        || request
            .source_uri
            .as_deref()
            .is_some_and(|value| value.chars().count() > 500)
    {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "knowledge document exceeds field limits",
        ));
    }
    if request.tags.len() > 20
        || request
            .tags
            .iter()
            .any(|tag| tag.trim().is_empty() || tag.chars().count() > 64)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "knowledge tags must contain at most 20 non-empty values of 64 characters",
        ));
    }
    if let Some(project_id) = request
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if state
            .store
            .lock()
            .expect("store mutex poisoned")
            .project(project_id)
            .is_none()
        {
            return Err(error(
                StatusCode::NOT_FOUND,
                "missing knowledge document project",
            ));
        }
    }
    Ok(())
}

fn apply_knowledge_document_request(
    document: &mut KnowledgeDocument,
    request: &SaveKnowledgeDocumentRequest,
) {
    document.project_id = normalized_optional(request.project_id.clone());
    document.title = request.title.trim().to_string();
    document.source_uri = normalized_optional(request.source_uri.clone());
    document.content = request.content.trim().to_string();
    document.tags = request
        .tags
        .iter()
        .map(|tag| tag.trim().to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    document.enabled = request.enabled;
}

fn retrieve_agent_knowledge(
    store: &cloud_control::CloudControlStore,
    query: &str,
    project_id: Option<&str>,
) -> Vec<serde_json::Value> {
    let terms = knowledge_search_terms(query);
    let mut matches = store
        .knowledge_documents()
        .filter(|document| {
            document.enabled
                && (document.project_id.is_none() || document.project_id.as_deref() == project_id)
        })
        .filter_map(|document| {
            let title = document.title.to_lowercase();
            let content = document.content.to_lowercase();
            let tags = document.tags.join(" ").to_lowercase();
            let score = terms.iter().fold(0usize, |score, term| {
                score
                    + usize::from(title.contains(term)) * 5
                    + usize::from(tags.contains(term)) * 3
                    + usize::from(content.contains(term))
            });
            (score > 0).then_some((score, document))
        })
        .collect::<Vec<_>>();
    matches.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .cmp(left_score)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
    });
    matches
        .into_iter()
        .take(5)
        .map(|(_, document)| {
            serde_json::json!({
                "documentId": document.document_id,
                "title": document.title,
                "sourceUri": document.source_uri,
                "excerpt": safe_knowledge_excerpt(&document.content, 600),
            })
        })
        .collect()
}

fn knowledge_search_terms(query: &str) -> BTreeSet<String> {
    const CJK_STOP: &str = "的是了在和与及或一个这那请帮我如何当前进行支持";
    let normalized = query.to_lowercase();
    let mut terms = normalized
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| term.chars().count() >= 2 && term.chars().count() <= 32)
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    terms.extend(
        normalized
            .chars()
            .filter(|character| {
                ('\u{4e00}'..='\u{9fff}').contains(character) && !CJK_STOP.contains(*character)
            })
            .map(|character| character.to_string()),
    );
    terms
}

fn safe_knowledge_excerpt(content: &str, max_chars: usize) -> String {
    const SENSITIVE_MARKERS: [&str; 7] = [
        "password",
        "secret",
        "api_key",
        "apikey",
        "access_token",
        "private key",
        "authorization:",
    ];
    content
        .lines()
        .filter(|line| {
            let normalized = line.to_lowercase();
            !SENSITIVE_MARKERS
                .iter()
                .any(|marker| normalized.contains(marker))
        })
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .take(max_chars)
        .collect()
}

fn agent_conversation_title(message: &str) -> String {
    let title = message.trim().chars().take(48).collect::<String>();
    if message.trim().chars().count() > 48 {
        format!("{title}...")
    } else {
        title
    }
}

fn agent_conversation_history(conversation: &AgentConversation) -> serde_json::Value {
    serde_json::Value::Array(
        conversation
            .messages
            .iter()
            .rev()
            .take(12)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|message| {
                serde_json::json!({
                    "role": match message.role {
                        AgentConversationRole::User => "user",
                        AgentConversationRole::Assistant => "assistant",
                    },
                    "content": message.content,
                })
            })
            .collect(),
    )
}

fn normalized_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

fn error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (
        status,
        Json(ErrorResponse {
            message: message.into(),
        }),
    )
}

fn persistence_error(cause: anyhow::Error) -> (StatusCode, Json<ErrorResponse>) {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("persist cloud state failed: {cause}"),
    )
}

fn agent_provider_error(cause: anyhow::Error) -> (StatusCode, Json<ErrorResponse>) {
    error(
        StatusCode::BAD_GATEWAY,
        format!("Agent provider unavailable: {cause}"),
    )
}

fn format_address(address: &PointAddress) -> String {
    format!("{}:{}", address.kind, address.value)
}

fn format_health(health: EdgeHealth) -> String {
    match health {
        EdgeHealth::Healthy => "健康",
        EdgeHealth::Degraded => "降级",
        EdgeHealth::Critical => "严重",
        EdgeHealth::Offline => "离线",
    }
    .to_string()
}

fn format_percent(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}%")
    } else {
        format!("{value:.1}%")
    }
}

fn format_protocol(protocol: ProtocolType) -> String {
    match protocol {
        ProtocolType::Simulated => "Simulated",
        ProtocolType::ModbusTcp => "Modbus TCP",
        ProtocolType::ModbusRtu => "Modbus RTU",
        ProtocolType::Dlt645 => "DL/T645",
        ProtocolType::Iec101 => "IEC-101",
        ProtocolType::CustomSerial => "自定义串口",
        ProtocolType::OpcUa => "OPC UA",
        ProtocolType::SiemensS7 => "Siemens S7",
    }
    .to_string()
}

fn mqtt_uplink_response(uplink: MqttUplinkConfig) -> MqttUplinkResponse {
    MqttUplinkResponse {
        sink_id: uplink.sink_id,
        broker: uplink.broker,
        client_id: uplink.client_id,
        username: uplink.username,
        password_env: uplink.password_env,
        tls_ca_path: uplink.tls_ca_path,
        topic_template: uplink.topic_template,
        qos: uplink.qos,
        batch_size: uplink.batch_size,
        flush_interval_ms: uplink.flush_interval_ms,
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

fn discovery_report_response(report: DiscoveryReport) -> DiscoveryReportResponse {
    DiscoveryReportResponse {
        job_id: report.job_id,
        protocol_connection_id: report.protocol_connection_id,
        discovered_points: report
            .discovered_points
            .into_iter()
            .map(discovered_point_response)
            .collect(),
        suggestions: report
            .suggestions
            .into_iter()
            .map(point_mapping_suggestion_response)
            .collect(),
    }
}

fn discovered_point_response(point: DiscoveredPoint) -> DiscoveredPointResponse {
    DiscoveredPointResponse {
        protocol_connection_id: point.protocol_connection_id,
        address: format_address(&point.address),
        value_type: format_telemetry_type(point.value_type),
        sample_values: point.sample_values,
        confidence: point.confidence,
    }
}

fn point_mapping_suggestion_response(
    suggestion: PointMappingSuggestion,
) -> PointMappingSuggestionResponse {
    PointMappingSuggestionResponse {
        point_id: suggestion.point_id,
        device_id: suggestion.device_id,
        semantic_id: suggestion.semantic_id,
        protocol_connection_id: suggestion.protocol_connection_id,
        address: format_address(&suggestion.address),
        value_type: format_telemetry_type(suggestion.value_type),
        unit: suggestion.unit.unwrap_or_else(|| "-".to_string()),
        confidence: suggestion.confidence,
        evidence: suggestion.evidence,
    }
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

fn parse_telemetry_type(
    value_type: &str,
) -> Result<TelemetryType, (StatusCode, Json<ErrorResponse>)> {
    match value_type.trim() {
        "bool" | "boolean" => Ok(TelemetryType::Boolean),
        "int64" | "int" | "integer" => Ok(TelemetryType::Integer),
        "float32" | "float" | "double" => Ok(TelemetryType::Float),
        "string" | "text" => Ok(TelemetryType::Text),
        value => Err(error(
            StatusCode::BAD_REQUEST,
            format!("unsupported telemetry valueType: {value}"),
        )),
    }
}

fn format_data_config_payload_mode(mode: DataConfigPayloadMode) -> String {
    match mode {
        DataConfigPayloadMode::Object => "object",
        DataConfigPayloadMode::Array => "array",
    }
    .to_string()
}

fn parse_data_config_payload_mode(
    mode: &str,
) -> Result<DataConfigPayloadMode, (StatusCode, Json<ErrorResponse>)> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "object" => Ok(DataConfigPayloadMode::Object),
        "array" => Ok(DataConfigPayloadMode::Array),
        value => Err(error(
            StatusCode::BAD_REQUEST,
            format!("unsupported data config payload mode: {value}"),
        )),
    }
}

fn parse_number_range(range: &str) -> Result<NumberRange, (StatusCode, Json<ErrorResponse>)> {
    let Some((min, max)) = range.split_once('-') else {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "range must use min-max format",
        ));
    };
    let min = min
        .trim()
        .parse::<f64>()
        .map_err(|_| error(StatusCode::BAD_REQUEST, "range min must be a number"))?;
    let max = max
        .trim()
        .parse::<f64>()
        .map_err(|_| error(StatusCode::BAD_REQUEST, "range max must be a number"))?;
    Ok(NumberRange::new(min, max))
}

fn format_algorithm_kind(kind: AlgorithmKind) -> String {
    match kind {
        AlgorithmKind::ChangeReport => "变化上报",
        AlgorithmKind::WindowAggregate => "窗口聚合",
        AlgorithmKind::ExpressionAggregate => "表达式聚合",
        AlgorithmKind::ThresholdRule => "阈值告警",
        AlgorithmKind::DurationRule => "持续条件",
        AlgorithmKind::Deadband => "死区过滤",
        AlgorithmKind::Debounce => "去抖动",
        AlgorithmKind::Statistics => "统计计算",
    }
    .to_string()
}

fn format_audit_action(action: AuditAction) -> String {
    match action {
        AuditAction::CreateRelease => "create_release",
        AuditAction::ApplyRelease => "apply_release",
        AuditAction::UpdateConfig => "update_config",
        AuditAction::CreateAgentProposal => "create_agent_proposal",
        AuditAction::ApproveAgentProposal => "approve_agent_proposal",
        AuditAction::RejectAgentProposal => "reject_agent_proposal",
        AuditAction::CreateKnowledgeDocument => "create_knowledge_document",
        AuditAction::UpdateKnowledgeDocument => "update_knowledge_document",
        AuditAction::DeleteKnowledgeDocument => "delete_knowledge_document",
        AuditAction::CreateAgentConversation => "create_agent_conversation",
        AuditAction::DeleteAgentConversation => "delete_agent_conversation",
    }
    .to_string()
}
