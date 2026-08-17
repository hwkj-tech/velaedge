use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use cloud_control::{
    AgentAuthorizationRequest, AgentChangeSet, AgentCommandCandidate, AgentPermission,
    AgentToolCaller, AgentToolDecision, AgentToolDescriptor, AgentToolEffect, AgentToolRegistry,
    AuditAction, AuditRecord,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    agent_service::{AgentToolCall, AgentToolRuntime},
    agent_tools::CloudAgentToolRuntime,
    api::{
        persist_agent_change_set_transition, persist_agent_command_candidate_transition,
        validate_agent_change_set_candidate, validate_agent_command_candidate,
    },
    ApiPrincipal, ApiRole, AppState,
};

pub const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
const JSON_RPC_VERSION: &str = "2.0";
const TOOL_PAGE_SIZE: usize = 50;

#[derive(Clone, Debug)]
pub struct McpConfig {
    enabled: bool,
    allowed_origins: BTreeSet<String>,
    allowed_projects: BTreeSet<String>,
    allowed_edges: BTreeSet<String>,
    rate_limit_per_minute: u32,
    rate_windows: Arc<Mutex<BTreeMap<String, RateWindow>>>,
}

#[derive(Debug)]
struct RateWindow {
    started_at: Instant,
    requests: u32,
}

impl McpConfig {
    pub fn from_env() -> Result<Self> {
        let enabled = match std::env::var("VELAEDGE_MCP_ENABLED")
            .unwrap_or_else(|_| "true".to_owned())
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "true" | "1" | "yes" => true,
            "false" | "0" | "no" => false,
            value => bail!("VELAEDGE_MCP_ENABLED must be true or false, got `{value}`"),
        };
        let origins = std::env::var("VELAEDGE_MCP_ALLOWED_ORIGINS").unwrap_or_else(|_| {
            default_allowed_origins(std::env::var("EDGEOPS_HTTP_ADDR").ok().as_deref())
                .into_iter()
                .collect::<Vec<_>>()
                .join(",")
        });
        let allowed_origins = origins
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect();
        let allowed_projects = comma_separated_env("VELAEDGE_MCP_ALLOWED_PROJECTS");
        let allowed_edges = comma_separated_env("VELAEDGE_MCP_ALLOWED_EDGES");
        let rate_limit_per_minute = std::env::var("VELAEDGE_MCP_RATE_LIMIT_PER_MINUTE")
            .unwrap_or_else(|_| "120".to_owned())
            .parse::<u32>()
            .context("VELAEDGE_MCP_RATE_LIMIT_PER_MINUTE must be an integer")?;
        if !(1..=10_000).contains(&rate_limit_per_minute) {
            bail!("VELAEDGE_MCP_RATE_LIMIT_PER_MINUTE must be between 1 and 10000");
        }
        Ok(Self {
            enabled,
            allowed_origins,
            allowed_projects,
            allowed_edges,
            rate_limit_per_minute,
            rate_windows: Arc::default(),
        })
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            allowed_origins: BTreeSet::new(),
            allowed_projects: BTreeSet::new(),
            allowed_edges: BTreeSet::new(),
            rate_limit_per_minute: 1,
            rate_windows: Arc::default(),
        }
    }

    pub fn with_allowed_origins(origins: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            enabled: true,
            allowed_origins: origins.into_iter().map(Into::into).collect(),
            allowed_projects: BTreeSet::new(),
            allowed_edges: BTreeSet::new(),
            rate_limit_per_minute: 120,
            rate_windows: Arc::default(),
        }
    }

    pub fn with_scope(
        mut self,
        projects: impl IntoIterator<Item = impl Into<String>>,
        edges: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.allowed_projects = projects.into_iter().map(Into::into).collect();
        self.allowed_edges = edges.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_rate_limit(mut self, requests_per_minute: u32) -> Self {
        self.rate_limit_per_minute = requests_per_minute.clamp(1, 10_000);
        self
    }

    fn permits_origin(&self, origin: Option<&HeaderValue>) -> bool {
        let Some(origin) = origin else {
            return true;
        };
        origin
            .to_str()
            .ok()
            .is_some_and(|origin| self.allowed_origins.contains(origin))
    }

    fn permits_request(&self, subject: &str) -> bool {
        let Ok(mut windows) = self.rate_windows.lock() else {
            return false;
        };
        windows.retain(|_, window| window.started_at.elapsed() < Duration::from_secs(60));
        let window = windows.entry(subject.to_owned()).or_insert(RateWindow {
            started_at: Instant::now(),
            requests: 0,
        });
        if window.started_at.elapsed() >= Duration::from_secs(60) {
            window.started_at = Instant::now();
            window.requests = 0;
        }
        if window.requests >= self.rate_limit_per_minute {
            return false;
        }
        window.requests += 1;
        true
    }
}

fn comma_separated_env(name: &str) -> BTreeSet<String> {
    std::env::var(name)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

impl Default for McpConfig {
    fn default() -> Self {
        Self::with_allowed_origins(default_allowed_origins(None))
    }
}

fn default_allowed_origins(http_addr: Option<&str>) -> BTreeSet<String> {
    let mut origins = [
        "http://127.0.0.1".to_owned(),
        "http://localhost".to_owned(),
        "http://127.0.0.1:8080".to_owned(),
        "http://localhost:8080".to_owned(),
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let Some(port) = http_addr
        .and_then(|address| address.rsplit_once(':'))
        .and_then(|(_, port)| port.parse::<u16>().ok())
    else {
        return origins;
    };
    origins.insert(format!("http://127.0.0.1:{port}"));
    origins.insert(format!("http://localhost:{port}"));
    origins
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolCallParams {
    name: String,
    #[serde(default = "empty_object")]
    arguments: Value,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ToolListParams {
    cursor: Option<String>,
}

fn empty_object() -> Value {
    json!({})
}

pub async fn mcp_get(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(response) = validate_transport(&state.mcp, &headers) {
        return response;
    }
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(header::ALLOW, "POST, OPTIONS")],
        "This stateless MCP endpoint does not open a server event stream",
    )
        .into_response()
}

pub async fn mcp_status(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiPrincipal>,
) -> Response {
    let runtime = CloudAgentToolRuntime::new(state.store.clone());
    let registry = match tool_registry(&runtime) {
        Ok(registry) => registry,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"message": error.to_string()})),
            )
                .into_response();
        }
    };
    let permissions = permissions_for_role(principal.role);
    let tools = registry
        .model_tools()
        .filter(|descriptor| descriptor.required_permissions.is_subset(&permissions))
        .map(|descriptor| {
            let read_only = descriptor.effect == AgentToolEffect::ReadOnly;
            json!({
                "name": descriptor.name,
                "description": descriptor.description,
                "effect": descriptor.effect,
                "risk": descriptor.risk,
                "readOnly": read_only,
                "humanReviewRequired": !read_only,
            })
        })
        .collect::<Vec<_>>();

    Json(json!({
        "enabled": state.mcp.enabled,
        "endpoint": "/mcp",
        "transport": "streamable_http",
        "protocolVersion": MCP_PROTOCOL_VERSION,
        "serverVersion": env!("CARGO_PKG_VERSION"),
        "authentication": {
            "required": state.api_auth.is_enabled(),
            "scheme": if state.api_auth.is_enabled() { "bearer" } else { "local_development" },
            "principal": principal.subject,
            "role": principal.role,
        },
        "scope": {
            "projects": state.mcp.allowed_projects,
            "edges": state.mcp.allowed_edges,
            "projectHeader": "x-velaedge-project-id",
            "edgeHeader": "x-velaedge-edge-id",
        },
        "controls": {
            "originValidation": !state.mcp.allowed_origins.is_empty(),
            "allowedOriginCount": state.mcp.allowed_origins.len(),
            "rateLimitPerMinute": state.mcp.rate_limit_per_minute,
            "auditEnabled": true,
            "executionToolsExposed": false,
            "draftOnlyMutations": true,
        },
        "tools": tools,
    }))
    .into_response()
}

pub async fn mcp_post(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiPrincipal>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Response {
    if let Some(response) = validate_transport(&state.mcp, &headers) {
        return response;
    }
    if !state.mcp.permits_request(&principal.subject) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "MCP request rate limit exceeded",
        )
            .into_response();
    }
    let request: JsonRpcRequest = match serde_json::from_value(request) {
        Ok(request) => request,
        Err(error) => {
            return rpc_error(
                Value::Null,
                -32600,
                "Invalid Request",
                Some(json!({
                    "detail": error.to_string()
                })),
            )
        }
    };
    if request.jsonrpc != JSON_RPC_VERSION || request.method.trim().is_empty() {
        return rpc_error(
            request.id.unwrap_or(Value::Null),
            -32600,
            "Invalid Request",
            None,
        );
    }

    let Some(id) = request.id else {
        return StatusCode::ACCEPTED.into_response();
    };

    match dispatch(
        &state,
        &principal,
        &headers,
        &request.method,
        request.params,
    )
    .await
    {
        Ok(result) => rpc_result(id, result),
        Err(McpDispatchError::MethodNotFound) => rpc_error(id, -32601, "Method not found", None),
        Err(McpDispatchError::InvalidParams(message)) => rpc_error(
            id,
            -32602,
            "Invalid params",
            Some(json!({"detail": message})),
        ),
        Err(McpDispatchError::Internal(message)) => rpc_error(
            id,
            -32603,
            "Internal error",
            Some(json!({"detail": message})),
        ),
    }
}

fn validate_transport(config: &McpConfig, headers: &HeaderMap) -> Option<Response> {
    if !config.enabled {
        return Some((StatusCode::NOT_FOUND, "MCP endpoint is disabled").into_response());
    }
    if !config.permits_origin(headers.get(header::ORIGIN)) {
        return Some((StatusCode::FORBIDDEN, "MCP Origin is not allowed").into_response());
    }
    None
}

#[derive(Debug)]
enum McpDispatchError {
    MethodNotFound,
    InvalidParams(String),
    Internal(String),
}

async fn dispatch(
    state: &AppState,
    principal: &ApiPrincipal,
    headers: &HeaderMap,
    method: &str,
    params: Value,
) -> std::result::Result<Value, McpDispatchError> {
    match method {
        "initialize" => initialize(params),
        "ping" => Ok(json!({})),
        "tools/list" => list_tools(state, principal, params),
        "tools/call" => call_tool(state, principal, headers, params).await,
        _ => Err(McpDispatchError::MethodNotFound),
    }
}

fn initialize(params: Value) -> std::result::Result<Value, McpDispatchError> {
    let requested = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .ok_or_else(|| McpDispatchError::InvalidParams("protocolVersion is required".to_owned()))?;
    let protocol_version = match requested {
        "2025-03-26" | "2025-06-18" | MCP_PROTOCOL_VERSION => requested,
        _ => MCP_PROTOCOL_VERSION,
    };
    Ok(json!({
        "protocolVersion": protocol_version,
        "capabilities": {
            "tools": {"listChanged": false}
        },
        "serverInfo": {
            "name": "velaedge",
            "title": "VelaEdge Industrial Edge MCP Gateway",
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": "Read industrial state or create reviewable drafts. Configuration apply and device dispatch are never exposed through MCP."
    }))
}

fn list_tools(
    state: &AppState,
    principal: &ApiPrincipal,
    params: Value,
) -> std::result::Result<Value, McpDispatchError> {
    let params: ToolListParams = serde_json::from_value(normalize_params(params))
        .map_err(|error| McpDispatchError::InvalidParams(error.to_string()))?;
    let offset = params
        .cursor
        .as_deref()
        .map(|cursor| {
            cursor
                .parse::<usize>()
                .map_err(|_| McpDispatchError::InvalidParams("cursor is invalid".to_owned()))
        })
        .transpose()?
        .unwrap_or_default();
    let runtime = CloudAgentToolRuntime::new(state.store.clone());
    let registry = tool_registry(&runtime).map_err(internal)?;
    let permissions = permissions_for_role(principal.role);
    let visible = registry
        .model_tools()
        .filter(|descriptor| descriptor.required_permissions.is_subset(&permissions))
        .collect::<Vec<_>>();
    let tools = visible
        .iter()
        .skip(offset)
        .take(TOOL_PAGE_SIZE)
        .map(|descriptor| mcp_tool(descriptor))
        .collect::<Vec<_>>();
    let next_cursor =
        (offset + tools.len() < visible.len()).then(|| (offset + tools.len()).to_string());
    let mut result = json!({"tools": tools});
    if let Some(next_cursor) = next_cursor {
        result["nextCursor"] = Value::String(next_cursor);
    }
    Ok(result)
}

async fn call_tool(
    state: &AppState,
    principal: &ApiPrincipal,
    headers: &HeaderMap,
    params: Value,
) -> std::result::Result<Value, McpDispatchError> {
    let params: ToolCallParams = serde_json::from_value(params)
        .map_err(|error| McpDispatchError::InvalidParams(error.to_string()))?;
    if !params.arguments.is_object() {
        return Err(McpDispatchError::InvalidParams(
            "tool arguments must be a JSON object".to_owned(),
        ));
    }
    let context = match tool_context(&state.mcp, headers, principal) {
        Ok(context) => context,
        Err(message) => {
            let denied_context = json!({
                "scope": {"projectId": null, "edgeId": null},
                "principal": {
                    "subject": principal.subject,
                    "role": principal.role,
                    "source": "mcp"
                }
            });
            record_tool_audit(
                state,
                principal,
                &params.name,
                &denied_context,
                "scope_denied",
            )
            .await
            .map_err(internal)?;
            return Ok(tool_error("scope_denied", message));
        }
    };
    let runtime = CloudAgentToolRuntime::new(state.store.clone());
    let registry = tool_registry(&runtime).map_err(internal)?;
    let decision = registry
        .authorize(
            &params.name,
            &AgentAuthorizationRequest {
                caller: AgentToolCaller::Model,
                permissions: permissions_for_role(principal.role),
                confirmation: None,
            },
        )
        .map_err(|error| McpDispatchError::InvalidParams(error.to_string()))?;
    if decision != AgentToolDecision::Allowed {
        record_tool_audit(state, principal, &params.name, &context, "denied")
            .await
            .map_err(internal)?;
        return Ok(tool_error(
            "permission_denied",
            format!(
                "tool `{}` is not available to this principal: {decision:?}",
                params.name
            ),
        ));
    }
    let call = AgentToolCall {
        call_id: uuid::Uuid::new_v4().to_string(),
        name: params.name.clone(),
        arguments: params.arguments,
    };
    let mut output = match runtime.execute(&call, &context) {
        Ok(output) => output,
        Err(error) => {
            record_tool_audit(state, principal, &params.name, &context, "failed")
                .await
                .map_err(internal)?;
            return Ok(tool_error("tool_execution_failed", format!("{error:#}")));
        }
    };
    if let Err(error) = persist_draft_result(state, principal, &params.name, &mut output).await {
        record_tool_audit(state, principal, &params.name, &context, "draft_failed")
            .await
            .map_err(internal)?;
        return Ok(tool_error("draft_persistence_failed", format!("{error:#}")));
    }
    record_tool_audit(state, principal, &params.name, &context, "succeeded")
        .await
        .map_err(internal)?;
    Ok(json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&output).unwrap_or_else(|_| output.to_string())
        }],
        "structuredContent": output,
        "isError": false
    }))
}

async fn persist_draft_result(
    state: &AppState,
    principal: &ApiPrincipal,
    tool_name: &str,
    output: &mut Value,
) -> Result<()> {
    match tool_name {
        "configuration.change_set.draft" => {
            let mut change_set: AgentChangeSet = serde_json::from_value(
                output
                    .get("changeSet")
                    .cloned()
                    .context("MCP tool omitted changeSet")?,
            )
            .context("decode MCP ChangeSet")?;
            change_set.created_by = principal.subject.clone();
            let report = validate_agent_change_set_candidate(state, &change_set);
            change_set.record_validation(report)?;
            let audit = AuditRecord::by_actor(
                AuditAction::CreateAgentChangeSet,
                format!("mcp-change-set:{}", change_set.change_set_id),
                principal.subject.clone(),
            );
            persist_agent_change_set_transition(state, change_set.clone(), audit)
                .await
                .map_err(|(_, body)| anyhow::anyhow!(body.0.message))?;
            output["changeSet"] = serde_json::to_value(change_set)?;
            output["nextAction"] = json!("human_review_required");
        }
        "device.command.draft" if output.get("duplicate") != Some(&Value::Bool(true)) => {
            let mut candidate: AgentCommandCandidate = serde_json::from_value(
                output
                    .get("commandCandidate")
                    .cloned()
                    .context("MCP tool omitted commandCandidate")?,
            )
            .context("decode MCP command candidate")?;
            candidate.created_by = principal.subject.clone();
            let report = validate_agent_command_candidate(state, &candidate);
            candidate.record_validation(report)?;
            let audit = AuditRecord::by_actor(
                AuditAction::CreateAgentCommandCandidate,
                format!("mcp-command:{}", candidate.candidate_id),
                principal.subject.clone(),
            );
            persist_agent_command_candidate_transition(state, candidate.clone(), audit)
                .await
                .map_err(|(_, body)| anyhow::anyhow!(body.0.message))?;
            output["commandCandidate"] = serde_json::to_value(candidate)?;
            output["nextAction"] = json!("human_review_required");
        }
        _ => {}
    }
    Ok(())
}

fn normalize_params(params: Value) -> Value {
    if params.is_null() {
        empty_object()
    } else {
        params
    }
}

fn tool_context(
    config: &McpConfig,
    headers: &HeaderMap,
    principal: &ApiPrincipal,
) -> std::result::Result<Value, String> {
    let header = |name: &'static str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let project_id = resolve_scope_value(
        "project",
        header("x-velaedge-project-id"),
        &config.allowed_projects,
    )?;
    let edge_id = resolve_scope_value("edge", header("x-velaedge-edge-id"), &config.allowed_edges)?;
    Ok(json!({
        "scope": {
            "projectId": project_id,
            "edgeId": edge_id
        },
        "principal": {
            "subject": principal.subject,
            "role": principal.role,
            "source": "mcp"
        }
    }))
}

fn resolve_scope_value(
    kind: &str,
    requested: Option<&str>,
    allowed: &BTreeSet<String>,
) -> std::result::Result<Option<String>, String> {
    if allowed.is_empty() {
        return Ok(requested.map(str::to_owned));
    }
    match requested {
        Some(value) if allowed.contains(value) => Ok(Some(value.to_owned())),
        Some(_) => Err(format!("requested {kind} is outside the MCP service scope")),
        None if allowed.len() == 1 => Ok(allowed.first().cloned()),
        None => Err(format!(
            "x-velaedge-{kind}-id is required because this credential can access multiple {kind}s"
        )),
    }
}

async fn record_tool_audit(
    state: &AppState,
    principal: &ApiPrincipal,
    tool_name: &str,
    context: &Value,
    outcome: &str,
) -> Result<()> {
    let project = context
        .pointer("/scope/projectId")
        .and_then(Value::as_str)
        .unwrap_or("all");
    let edge = context
        .pointer("/scope/edgeId")
        .and_then(Value::as_str)
        .unwrap_or("all");
    state
        .persist_audit_record(AuditRecord::by_actor(
            AuditAction::InvokeAgentTool,
            format!("mcp-tool:{tool_name}:{outcome}:project={project}:edge={edge}"),
            principal.subject.clone(),
        ))
        .await
}

fn tool_registry(runtime: &dyn AgentToolRuntime) -> Result<AgentToolRegistry> {
    let mut registry = AgentToolRegistry::default();
    for descriptor in runtime.descriptors() {
        registry.register(descriptor)?;
    }
    Ok(registry)
}

fn permissions_for_role(role: ApiRole) -> BTreeSet<AgentPermission> {
    let mut permissions = [
        AgentPermission::ProjectRead,
        AgentPermission::ProductRead,
        AgentPermission::ConfigurationRead,
        AgentPermission::RuntimeRead,
        AgentPermission::MqttRead,
        AgentPermission::AuditRead,
        AgentPermission::KnowledgeRead,
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    if role >= ApiRole::Operator {
        permissions.extend([
            AgentPermission::ConfigurationPropose,
            AgentPermission::CommandPropose,
        ]);
    }
    permissions
}

fn mcp_tool(descriptor: &AgentToolDescriptor) -> Value {
    let read_only = descriptor.effect == AgentToolEffect::ReadOnly;
    json!({
        "name": descriptor.name,
        "title": descriptor.name,
        "description": descriptor.description,
        "inputSchema": descriptor.input_schema,
        "outputSchema": descriptor.output_schema,
        "annotations": {
            "readOnlyHint": read_only,
            "destructiveHint": false,
            "idempotentHint": read_only,
            "openWorldHint": false
        },
        "_meta": {
            "io.velaedge/risk": descriptor.risk,
            "io.velaedge/effect": descriptor.effect,
            "io.velaedge/humanReviewRequired": !read_only
        }
    })
}

fn tool_error(code: &str, message: String) -> Value {
    json!({
        "content": [{"type": "text", "text": message}],
        "structuredContent": {"error": {"code": code, "message": message}},
        "isError": true
    })
}

fn internal(error: impl std::fmt::Display) -> McpDispatchError {
    McpDispatchError::Internal(error.to_string())
}

fn rpc_result(id: Value, result: Value) -> Response {
    Json(JsonRpcResponse {
        jsonrpc: JSON_RPC_VERSION,
        id,
        result: Some(result),
        error: None,
    })
    .into_response()
}

fn rpc_error(id: Value, code: i64, message: &str, data: Option<Value>) -> Response {
    Json(JsonRpcResponse {
        jsonrpc: JSON_RPC_VERSION,
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.to_owned(),
            data,
        }),
    })
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_origins_follow_the_configured_local_http_port() {
        let origins = default_allowed_origins(Some("127.0.0.1:8082"));
        assert!(origins.contains("http://127.0.0.1:8082"));
        assert!(origins.contains("http://localhost:8082"));
        assert!(origins.contains("http://127.0.0.1:8080"));
    }

    #[test]
    fn malformed_http_addresses_do_not_widen_the_origin_allowlist() {
        let origins = default_allowed_origins(Some("not-an-address"));
        assert_eq!(origins.len(), 4);
        assert!(!origins
            .iter()
            .any(|origin| origin.contains("not-an-address")));
    }

    #[test]
    fn viewer_permissions_never_include_proposal_or_execution() {
        let permissions = permissions_for_role(ApiRole::Viewer);
        assert!(permissions.contains(&AgentPermission::RuntimeRead));
        assert!(!permissions.contains(&AgentPermission::ConfigurationPropose));
        assert!(!permissions.contains(&AgentPermission::ConfigurationApply));
        assert!(!permissions.contains(&AgentPermission::CommandDispatch));
    }

    #[test]
    fn operator_can_only_add_non_executing_proposal_permissions() {
        let permissions = permissions_for_role(ApiRole::Operator);
        assert!(permissions.contains(&AgentPermission::ConfigurationPropose));
        assert!(permissions.contains(&AgentPermission::CommandPropose));
        assert!(!permissions.contains(&AgentPermission::ConfigurationApply));
        assert!(!permissions.contains(&AgentPermission::CommandDispatch));
    }
}
