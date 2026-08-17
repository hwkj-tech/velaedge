use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use cloud_control::{
    AgentChangeOperation, AgentChangeSet, AgentChangeTarget, AgentCommandCandidate,
    AgentCommandTarget, AgentConfirmationPolicy, AgentPermission, AgentResourceKind,
    AgentRiskLevel, AgentToolCategory, AgentToolDescriptor, AgentToolEffect, AgentToolExposure,
    CloudControlStore, EdgeNode, Product, ProductVersion, ProductVersionStatus,
};
use edge_core::{
    validate_command_flow, CommandGraphNodeKind, EdgeConfigPackage, MqttUplinkConfig, NumberRange,
    ProtocolCircuitState, RuntimeEventSeverity, TelemetryType,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::agent_knowledge::search_agent_knowledge;
use crate::agent_service::{AgentToolCall, AgentToolRuntime};

const TOOL_SCHEMA_VERSION: &str = "velaedge.agent.tool/v1";
const DEFAULT_RESULT_LIMIT: usize = 20;
const MAX_RESULT_LIMIT: usize = 100;

#[derive(Clone)]
pub struct CloudAgentToolRuntime {
    store: Arc<Mutex<CloudControlStore>>,
}

impl CloudAgentToolRuntime {
    pub fn new(store: Arc<Mutex<CloudControlStore>>) -> Self {
        Self { store }
    }

    fn execute_locked(&self, call: &AgentToolCall, context: &Value) -> Result<Value> {
        let scope = AgentToolScope::from_context(context)?;
        let store = self
            .store
            .lock()
            .map_err(|_| anyhow!("cloud control store mutex poisoned"))?;

        match call.name.as_str() {
            "project.list" => project_list(&store, &scope, &call.arguments),
            "product.inspect" => product_inspect(&store, &scope, &call.arguments),
            "configuration.inspect" => configuration_inspect(&store, &scope, &call.arguments),
            "protocol.inspect" => protocol_inspect(&store, &scope, &call.arguments),
            "point_set.inspect" => point_set_inspect(&store, &scope, &call.arguments),
            "collection_flow.inspect" => collection_flow_inspect(&store, &scope, &call.arguments),
            "command_flow.inspect" => command_flow_inspect(&store, &scope, &call.arguments),
            "runtime.metrics" => runtime_metrics(&store, &scope, &call.arguments),
            "mqtt.status" => mqtt_status(&store, &scope, &call.arguments),
            "operations.diagnose" => operations_diagnose(&store, &scope, &call.arguments),
            "audit.search" => audit_search(&store, &scope, &call.arguments),
            "knowledge.search" => knowledge_search(&store, &scope, &call.arguments),
            "configuration.change_set.draft" => {
                draft_product_version_change_set(&store, &scope, &call.arguments)
            }
            "device.command.draft" => draft_device_command(&store, &scope, &call.arguments),
            _ => bail!("unsupported Agent tool `{}`", call.name),
        }
    }
}

impl AgentToolRuntime for CloudAgentToolRuntime {
    fn descriptors(&self) -> Vec<AgentToolDescriptor> {
        vec![
            read_descriptor(
                "project.list",
                "List projects and scoped resource counts.",
                AgentToolCategory::Project,
                AgentPermission::ProjectRead,
                json!({
                    "type": "object",
                    "properties": {"projectId": {"type": "string"}, "limit": limit_schema()},
                    "additionalProperties": false
                }),
            ),
            read_descriptor(
                "product.inspect",
                "Inspect products and their versioned protocol, point and flow capabilities.",
                AgentToolCategory::Product,
                AgentPermission::ProductRead,
                product_input_schema(),
            ),
            read_descriptor(
                "configuration.inspect",
                "Inspect the complete scoped product or edge configuration.",
                AgentToolCategory::Configuration,
                AgentPermission::ConfigurationRead,
                configuration_input_schema(),
            ),
            read_descriptor(
                "protocol.inspect",
                "Inspect industrial protocol connection parameters and references.",
                AgentToolCategory::Configuration,
                AgentPermission::ConfigurationRead,
                configuration_input_schema(),
            ),
            read_descriptor(
                "point_set.inspect",
                "Inspect reusable point sets, addresses, access and sampling intervals.",
                AgentToolCategory::Configuration,
                AgentPermission::ConfigurationRead,
                json!({
                    "type": "object",
                    "properties": {
                        "projectId": {"type": "string"},
                        "pointSetId": {"type": "string"},
                        "limit": limit_schema()
                    },
                    "additionalProperties": false
                }),
            ),
            read_descriptor(
                "collection_flow.inspect",
                "Inspect collection graphs, calculation nodes and MQTT outputs.",
                AgentToolCategory::Configuration,
                AgentPermission::ConfigurationRead,
                configuration_input_schema(),
            ),
            read_descriptor(
                "command_flow.inspect",
                "Inspect downstream MQTT command graphs and writable-point targets.",
                AgentToolCategory::Command,
                AgentPermission::ConfigurationRead,
                configuration_input_schema(),
            ),
            read_descriptor(
                "runtime.metrics",
                "Read scoped Runtime, protocol, collection, storage and sync metrics.",
                AgentToolCategory::Runtime,
                AgentPermission::RuntimeRead,
                edge_input_schema(),
            ),
            read_descriptor(
                "mqtt.status",
                "Read sanitized MQTT configuration and Runtime delivery metrics.",
                AgentToolCategory::Mqtt,
                AgentPermission::MqttRead,
                edge_input_schema(),
            ),
            diagnosis_descriptor(),
            read_descriptor(
                "audit.search",
                "Search scoped audit records using bounded filters.",
                AgentToolCategory::Audit,
                AgentPermission::AuditRead,
                json!({
                    "type": "object",
                    "properties": {
                        "projectId": {"type": "string"},
                        "edgeId": {"type": "string"},
                        "targetContains": {"type": "string"},
                        "actor": {"type": "string"},
                        "limit": limit_schema()
                    },
                    "additionalProperties": false
                }),
            ),
            read_descriptor(
                "knowledge.search",
                "Search scoped protocol manuals, runbooks and configuration contracts with chunk citations.",
                AgentToolCategory::Knowledge,
                AgentPermission::KnowledgeRead,
                json!({
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": {"type": "string", "minLength": 1, "maxLength": 1000},
                        "projectId": {"type": "string"},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 12}
                    },
                    "additionalProperties": false
                }),
            ),
            change_set_draft_descriptor(),
            command_draft_descriptor(),
        ]
    }

    fn execute(&self, call: &AgentToolCall, context: &Value) -> Result<Value> {
        self.execute_locked(call, context)
            .map_err(|error| anyhow!("Agent tool `{}` failed: {error:#}", call.name))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DraftDeviceCommandInput {
    project_id: Option<String>,
    edge_id: String,
    flow_id: String,
    point_id: String,
    device_id: Option<String>,
    value: Value,
    title: String,
    rationale: String,
    idempotency_key: String,
}

fn draft_device_command(
    store: &CloudControlStore,
    scope: &AgentToolScope,
    arguments: &Value,
) -> Result<Value> {
    let input: DraftDeviceCommandInput =
        serde_json::from_value(arguments.clone()).context("decode device command draft")?;
    let project_id = scope
        .resolve_project(arguments)?
        .ok_or_else(|| anyhow!("projectId is required to draft a device command"))?;
    if input
        .project_id
        .as_deref()
        .is_some_and(|value| value != project_id)
    {
        bail!("projectId is outside the active Agent scope");
    }
    let edge_id = scope
        .resolve_edge(arguments)?
        .ok_or_else(|| anyhow!("edgeId is required to draft a device command"))?;
    if input.edge_id != edge_id {
        bail!("edgeId is outside the active Agent scope");
    }
    for (label, value) in [
        ("flowId", input.flow_id.as_str()),
        ("pointId", input.point_id.as_str()),
        ("title", input.title.as_str()),
        ("rationale", input.rationale.as_str()),
        ("idempotencyKey", input.idempotency_key.as_str()),
    ] {
        ensure_non_empty(label, value)?;
    }
    let edge = ensure_edge_scope(store, Some(&project_id), &edge_id)?;
    if let Some(existing) =
        store.agent_command_candidate_by_idempotency(&project_id, &edge_id, &input.idempotency_key)
    {
        return Ok(json!({
            "schemaVersion": TOOL_SCHEMA_VERSION,
            "tool": "device.command.draft",
            "commandCandidate": existing,
            "duplicate": true,
            "nextAction": "review_existing_candidate",
            "dispatched": false,
        }));
    }
    let package = store
        .latest_config_package_for_edge(&edge_id)
        .ok_or_else(|| anyhow!("edge `{edge_id}` has no configuration package"))?;
    let flow = package
        .command_flows
        .iter()
        .find(|flow| flow.flow_id == input.flow_id)
        .ok_or_else(|| anyhow!("command flow `{}` does not exist", input.flow_id))?;
    if !flow.enabled {
        bail!("command flow `{}` is disabled", input.flow_id);
    }
    validate_command_flow(flow, &package.point_mappings)
        .map_err(|error| anyhow!("command flow is invalid: {error}"))?;
    let write_node = flow
        .nodes
        .iter()
        .find(|node| {
            node.kind == CommandGraphNodeKind::PointWrite
                && node.ref_id.as_deref() == Some(input.point_id.as_str())
        })
        .ok_or_else(|| {
            anyhow!(
                "command flow `{}` does not expose writable point `{}`",
                input.flow_id,
                input.point_id
            )
        })?;
    let mapping = package
        .point_mappings
        .iter()
        .find(|mapping| mapping.point_id == input.point_id)
        .ok_or_else(|| anyhow!("point `{}` does not exist", input.point_id))?;
    if !mapping.access.is_writable() {
        bail!("point `{}` is read-only", input.point_id);
    }
    if mapping.protocol_connection_id != flow.protocol_connection_id {
        bail!(
            "point `{}` uses protocol connection `{}`, but flow `{}` uses `{}`",
            input.point_id,
            mapping.protocol_connection_id,
            flow.flow_id,
            flow.protocol_connection_id
        );
    }
    if input
        .device_id
        .as_deref()
        .is_some_and(|device_id| device_id != mapping.device_id)
    {
        bail!("deviceId does not match the writable point mapping");
    }
    validate_command_value(&input.value, mapping.value_type, mapping.range)?;

    let candidate = AgentCommandCandidate::new(
        input.title.trim(),
        input.rationale.trim(),
        AgentCommandTarget {
            project_id,
            edge_id,
            product_id: edge.product_id.clone(),
            product_version: Some(package.version.clone()),
            flow_id: flow.flow_id.clone(),
            protocol_connection_id: flow.protocol_connection_id.clone(),
            device_id: mapping.device_id.clone(),
            point_id: mapping.point_id.clone(),
        },
        input.value,
        input.idempotency_key.trim(),
        AgentRiskLevel::High,
        "agent:model-gateway",
    )?;

    Ok(json!({
        "schemaVersion": TOOL_SCHEMA_VERSION,
        "tool": "device.command.draft",
        "commandCandidate": candidate,
        "duplicate": false,
        "writablePoint": mapping,
        "commandFlow": {
            "flowId": flow.flow_id,
            "subscribeTopic": flow.subscribe_topic,
            "mqttConnectionId": flow.mqtt_connection_id,
            "writeNodeId": write_node.node_id,
            "valuePath": write_node.params.get("value_path").and_then(Value::as_str)
                .unwrap_or("value"),
        },
        "nextAction": "validate_and_request_human_confirmation",
        "dispatched": false,
    }))
}

fn validate_command_value(
    value: &Value,
    value_type: TelemetryType,
    range: Option<NumberRange>,
) -> Result<()> {
    let numeric = match value_type {
        TelemetryType::Float => Some(
            value
                .as_f64()
                .ok_or_else(|| anyhow!("float point requires a JSON number"))?,
        ),
        TelemetryType::Integer => Some(
            value
                .as_i64()
                .ok_or_else(|| anyhow!("integer point requires a JSON integer"))?
                as f64,
        ),
        TelemetryType::Boolean => {
            if !value.is_boolean() {
                bail!("boolean point requires true or false");
            }
            None
        }
        TelemetryType::Text => {
            if !value.is_string() {
                bail!("text point requires a JSON string");
            }
            None
        }
    };
    if let (Some(range), Some(numeric)) = (range, numeric) {
        if !range.contains(numeric) {
            bail!(
                "command value {numeric} is outside [{}, {}]",
                range.min,
                range.max
            );
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DraftProductVersionChangeSetInput {
    project_id: Option<String>,
    product_id: String,
    base_version: Option<String>,
    target_version: String,
    title: String,
    rationale: String,
    patch: Value,
    risk: Option<AgentRiskLevel>,
}

fn draft_product_version_change_set(
    store: &CloudControlStore,
    scope: &AgentToolScope,
    arguments: &Value,
) -> Result<Value> {
    let input: DraftProductVersionChangeSetInput = serde_json::from_value(arguments.clone())
        .context("decode product version ChangeSet draft")?;
    let project_id = scope
        .resolve_project(arguments)?
        .ok_or_else(|| anyhow!("projectId is required to draft a configuration ChangeSet"))?;
    if input
        .project_id
        .as_deref()
        .is_some_and(|value| value != project_id)
    {
        bail!("projectId is outside the active Agent scope");
    }
    ensure_non_empty("productId", &input.product_id)?;
    ensure_non_empty("targetVersion", &input.target_version)?;
    ensure_non_empty("title", &input.title)?;
    ensure_non_empty("rationale", &input.rationale)?;
    let product = store
        .product(&input.product_id)
        .ok_or_else(|| anyhow!("product `{}` does not exist", input.product_id))?;
    if product.project_id != project_id {
        bail!(
            "product `{}` is outside project `{project_id}`",
            input.product_id
        );
    }
    if store
        .product_version(&input.product_id, &input.target_version)
        .is_some()
    {
        bail!(
            "target product version `{}` already exists",
            input.target_version
        );
    }

    let base_version = input
        .base_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| product.latest_version.clone());
    if let (Some(requested), Some(current)) =
        (base_version.as_deref(), product.latest_version.as_deref())
    {
        if requested != current {
            bail!("baseVersion `{requested}` is stale; the active product version is `{current}`");
        }
    }

    let before = base_version
        .as_deref()
        .map(|version| {
            store
                .product_version(&input.product_id, version)
                .cloned()
                .ok_or_else(|| anyhow!("base product version `{version}` does not exist"))
        })
        .transpose()?;
    let template = before
        .clone()
        .unwrap_or_else(|| ProductVersion::draft(&input.product_id, &input.target_version));
    let changed_fields = patch_product_version(&template, &input.patch, &input.target_version)?;
    let after = changed_fields.0;
    let risk = input
        .risk
        .unwrap_or(AgentRiskLevel::Low)
        .max(risk_for_product_patch(&changed_fields.1));
    let operation = match before.as_ref() {
        Some(before) => AgentChangeOperation::update(
            AgentResourceKind::ProductVersion,
            format!("{}:{}", input.product_id, input.target_version),
            serde_json::to_value(before).context("encode base product version")?,
            serde_json::to_value(&after).context("encode target product version")?,
        ),
        None => AgentChangeOperation::create(
            AgentResourceKind::ProductVersion,
            format!("{}:{}", input.product_id, input.target_version),
            serde_json::to_value(&after).context("encode target product version")?,
        ),
    };
    let edge_ids = store
        .edge_nodes()
        .filter(|edge| edge.product_id.as_deref() == Some(input.product_id.as_str()))
        .map(|edge| edge.edge_id.clone())
        .collect();
    let change_set = AgentChangeSet::new(
        input.title.trim(),
        input.rationale.trim(),
        AgentChangeTarget {
            project_id,
            product_id: Some(input.product_id),
            product_version: Some(input.target_version),
            edge_ids,
        },
        base_version.unwrap_or_else(|| "none".to_owned()),
        vec![operation],
        risk,
        "agent:model-gateway",
    )?;

    Ok(json!({
        "schemaVersion": TOOL_SCHEMA_VERSION,
        "tool": "configuration.change_set.draft",
        "changeSet": change_set,
        "changedFields": changed_fields.1,
        "nextAction": "validate_and_request_human_confirmation",
        "applied": false,
    }))
}

fn patch_product_version(
    base: &ProductVersion,
    patch: &Value,
    target_version: &str,
) -> Result<(ProductVersion, Vec<String>)> {
    let patch = patch
        .as_object()
        .ok_or_else(|| anyhow!("patch must be a JSON object"))?;
    if patch.is_empty() {
        bail!("patch must change at least one product configuration field");
    }
    let allowed = [
        "pointSetIds",
        "deviceModels",
        "devices",
        "protocolConnections",
        "collectionTasks",
        "algorithms",
        "dataConfigs",
        "commandFlows",
        "mqttUplinks",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let mut value = serde_json::to_value(base).context("encode base product version")?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| anyhow!("product version must encode as an object"))?;
    let mut changed_fields = Vec::with_capacity(patch.len());
    for (key, value) in patch {
        if !allowed.contains(key.as_str()) {
            bail!("unsupported product version patch field `{key}`");
        }
        object.insert(key.clone(), value.clone());
        changed_fields.push(key.clone());
    }
    object.insert(
        "productId".to_owned(),
        Value::String(base.product_id.clone()),
    );
    object.insert(
        "version".to_owned(),
        Value::String(target_version.to_owned()),
    );
    object.insert("status".to_owned(), json!(ProductVersionStatus::Draft));
    object.insert("createdAt".to_owned(), json!(Utc::now()));
    let version =
        serde_json::from_value(value).context("validate patched product version shape")?;
    Ok((version, changed_fields))
}

fn risk_for_product_patch(changed_fields: &[String]) -> AgentRiskLevel {
    if changed_fields.iter().any(|field| {
        matches!(
            field.as_str(),
            "commandFlows" | "protocolConnections" | "mqttUplinks"
        )
    }) {
        AgentRiskLevel::High
    } else {
        AgentRiskLevel::Medium
    }
}

fn ensure_non_empty(label: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{label} is required");
    }
    Ok(())
}

#[derive(Clone, Debug, Default)]
struct AgentToolScope {
    project_id: Option<String>,
    edge_id: Option<String>,
}

impl AgentToolScope {
    fn from_context(context: &Value) -> Result<Self> {
        let scope = context.get("scope").unwrap_or(&Value::Null);
        Ok(Self {
            project_id: optional_string(scope, "projectId")?,
            edge_id: optional_string(scope, "edgeId")?,
        })
    }

    fn resolve_project(&self, arguments: &Value) -> Result<Option<String>> {
        restrict_scope(
            "projectId",
            self.project_id.as_deref(),
            optional_string(arguments, "projectId")?.as_deref(),
        )
    }

    fn resolve_edge(&self, arguments: &Value) -> Result<Option<String>> {
        restrict_scope(
            "edgeId",
            self.edge_id.as_deref(),
            optional_string(arguments, "edgeId")?.as_deref(),
        )
    }

    fn as_value(&self) -> Value {
        json!({"projectId": self.project_id, "edgeId": self.edge_id})
    }
}

#[derive(Clone, Debug)]
struct ConfigurationView {
    source_kind: &'static str,
    source_id: String,
    project_id: Option<String>,
    product_id: Option<String>,
    edge_id: Option<String>,
    version: String,
    protocol_connections: Value,
    point_mappings: Value,
    point_set_ids: Value,
    collection_tasks: Value,
    algorithms: Value,
    data_configs: Value,
    command_flows: Value,
    mqtt_uplinks: Value,
}

impl ConfigurationView {
    fn from_edge(package: &EdgeConfigPackage, edge: Option<&EdgeNode>) -> Result<Self> {
        Ok(Self {
            source_kind: "edge_config_package",
            source_id: package.edge_id.clone(),
            project_id: edge.and_then(|edge| edge.project_id.clone()),
            product_id: edge.and_then(|edge| edge.product_id.clone()),
            edge_id: Some(package.edge_id.clone()),
            version: package.version.clone(),
            protocol_connections: serialize_sanitized(&package.protocol_connections)?,
            point_mappings: serialize_sanitized(&package.point_mappings)?,
            point_set_ids: json!([]),
            collection_tasks: serialize_sanitized(&package.collection_tasks)?,
            algorithms: serialize_sanitized(&package.algorithms)?,
            data_configs: serialize_sanitized(&package.data_configs)?,
            command_flows: serialize_sanitized(&package.command_flows)?,
            mqtt_uplinks: serialize_sanitized(&package.mqtt_uplinks)?,
        })
    }

    fn from_product(product: &Product, version: &ProductVersion) -> Result<Self> {
        Ok(Self {
            source_kind: "product_version",
            source_id: product.product_id.clone(),
            project_id: Some(product.project_id.clone()),
            product_id: Some(product.product_id.clone()),
            edge_id: None,
            version: version.version.clone(),
            protocol_connections: serialize_sanitized(&version.protocol_connections)?,
            point_mappings: json!([]),
            point_set_ids: serialize_sanitized(&version.point_set_ids)?,
            collection_tasks: serialize_sanitized(&version.collection_tasks)?,
            algorithms: serialize_sanitized(&version.algorithms)?,
            data_configs: serialize_sanitized(&version.data_configs)?,
            command_flows: serialize_sanitized(&version.command_flows)?,
            mqtt_uplinks: serialize_sanitized(&version.mqtt_uplinks)?,
        })
    }

    fn full_value(&self) -> Value {
        json!({
            "sourceKind": self.source_kind,
            "sourceId": self.source_id,
            "projectId": self.project_id,
            "productId": self.product_id,
            "edgeId": self.edge_id,
            "version": self.version,
            "protocolConnections": self.protocol_connections,
            "pointMappings": self.point_mappings,
            "pointSetIds": self.point_set_ids,
            "collectionTasks": self.collection_tasks,
            "algorithms": self.algorithms,
            "collectionFlows": self.data_configs,
            "commandFlows": self.command_flows,
            "mqttUplinks": self.mqtt_uplinks,
        })
    }

    fn slice_value(&self, key: &str, value: &Value) -> Value {
        let mut object = Map::from_iter([
            ("sourceKind".to_owned(), json!(self.source_kind)),
            ("sourceId".to_owned(), json!(self.source_id)),
            ("projectId".to_owned(), json!(self.project_id)),
            ("productId".to_owned(), json!(self.product_id)),
            ("edgeId".to_owned(), json!(self.edge_id)),
            ("version".to_owned(), json!(self.version)),
        ]);
        object.insert(key.to_owned(), value.clone());
        Value::Object(object)
    }
}

fn project_list(
    store: &CloudControlStore,
    scope: &AgentToolScope,
    arguments: &Value,
) -> Result<Value> {
    let project_id = scope.resolve_project(arguments)?;
    let limit = result_limit(arguments)?;
    let projects = store
        .projects()
        .filter(|project| project_id.as_deref().is_none_or(|id| project.project_id == id))
        .take(limit)
        .map(|project| {
            json!({
                "project": project,
                "productCount": store.products().filter(|item| item.project_id == project.project_id).count(),
                "pointSetCount": store.point_sets().filter(|item| item.project_id == project.project_id).count(),
                "edgeCount": store.edge_nodes().filter(|item| item.project_id.as_deref() == Some(project.project_id.as_str())).count(),
            })
        })
        .collect::<Vec<_>>();
    Ok(response("project.list", scope, projects, Vec::new()))
}

fn product_inspect(
    store: &CloudControlStore,
    scope: &AgentToolScope,
    arguments: &Value,
) -> Result<Value> {
    let project_id = scope.resolve_project(arguments)?;
    let product_id = optional_string(arguments, "productId")?;
    let requested_version = optional_string(arguments, "version")?;
    let limit = result_limit(arguments)?;
    let products = store
        .products()
        .filter(|product| {
            project_id
                .as_deref()
                .is_none_or(|id| product.project_id == id)
                && product_id
                    .as_deref()
                    .is_none_or(|id| product.product_id == id)
        })
        .take(limit)
        .map(|product| {
            let versions = store
                .product_versions()
                .filter(|version| {
                    version.product_id == product.product_id
                        && requested_version
                            .as_deref()
                            .is_none_or(|value| version.version == value)
                })
                .map(|version| {
                    json!({
                        "version": version.version,
                        "status": version.status,
                        "pointSetIds": version.point_set_ids,
                        "protocolConnectionCount": version.protocol_connections.len(),
                        "collectionFlowCount": version.data_configs.len(),
                        "commandFlowCount": version.command_flows.len(),
                        "mqttSinkCount": version.mqtt_uplinks.len(),
                        "createdAt": version.created_at,
                    })
                })
                .collect::<Vec<_>>();
            json!({"product": product, "versions": versions})
        })
        .collect::<Vec<_>>();
    if product_id.is_some() && products.is_empty() {
        bail!("product is outside the active Agent scope or does not exist");
    }
    Ok(response("product.inspect", scope, products, Vec::new()))
}

fn configuration_inspect(
    store: &CloudControlStore,
    scope: &AgentToolScope,
    arguments: &Value,
) -> Result<Value> {
    let views = configuration_views(store, scope, arguments)?;
    Ok(response(
        "configuration.inspect",
        scope,
        views.iter().map(ConfigurationView::full_value).collect(),
        Vec::new(),
    ))
}

fn protocol_inspect(
    store: &CloudControlStore,
    scope: &AgentToolScope,
    arguments: &Value,
) -> Result<Value> {
    let views = configuration_views(store, scope, arguments)?;
    Ok(response(
        "protocol.inspect",
        scope,
        views
            .iter()
            .map(|view| view.slice_value("protocolConnections", &view.protocol_connections))
            .collect(),
        Vec::new(),
    ))
}

fn point_set_inspect(
    store: &CloudControlStore,
    scope: &AgentToolScope,
    arguments: &Value,
) -> Result<Value> {
    let project_id = scope.resolve_project(arguments)?;
    let point_set_id = optional_string(arguments, "pointSetId")?;
    let limit = result_limit(arguments)?;
    let items = store
        .point_sets()
        .filter(|point_set| {
            project_id
                .as_deref()
                .is_none_or(|id| point_set.project_id == id)
                && point_set_id
                    .as_deref()
                    .is_none_or(|id| point_set.point_set_id == id)
        })
        .take(limit)
        .map(serialize_sanitized)
        .collect::<Result<Vec<_>>>()?;
    if point_set_id.is_some() && items.is_empty() {
        bail!("point set is outside the active Agent scope or does not exist");
    }
    Ok(response("point_set.inspect", scope, items, Vec::new()))
}

fn collection_flow_inspect(
    store: &CloudControlStore,
    scope: &AgentToolScope,
    arguments: &Value,
) -> Result<Value> {
    let views = configuration_views(store, scope, arguments)?;
    Ok(response(
        "collection_flow.inspect",
        scope,
        views
            .iter()
            .map(|view| {
                json!({
                    "sourceKind": view.source_kind,
                    "sourceId": view.source_id,
                    "projectId": view.project_id,
                    "productId": view.product_id,
                    "edgeId": view.edge_id,
                    "version": view.version,
                    "collectionTasks": view.collection_tasks,
                    "algorithms": view.algorithms,
                    "collectionFlows": view.data_configs,
                })
            })
            .collect(),
        Vec::new(),
    ))
}

fn command_flow_inspect(
    store: &CloudControlStore,
    scope: &AgentToolScope,
    arguments: &Value,
) -> Result<Value> {
    let views = configuration_views(store, scope, arguments)?;
    Ok(response(
        "command_flow.inspect",
        scope,
        views
            .iter()
            .map(|view| view.slice_value("commandFlows", &view.command_flows))
            .collect(),
        Vec::new(),
    ))
}

fn runtime_metrics(
    store: &CloudControlStore,
    scope: &AgentToolScope,
    arguments: &Value,
) -> Result<Value> {
    let edge_id = scope.resolve_edge(arguments)?;
    let project_id = scope.resolve_project(arguments)?;
    let limit = result_limit(arguments)?;
    let edges = scoped_edges(store, project_id.as_deref(), edge_id.as_deref());
    if edge_id.is_some() && edges.is_empty() {
        bail!("edge is outside the active Agent scope or does not exist");
    }
    let edge_ids = edges
        .iter()
        .map(|edge| edge.edge_id.as_str())
        .collect::<BTreeSet<_>>();
    let items = edges
        .iter()
        .take(limit)
        .map(|edge| {
            let events = store
                .runtime_events()
                .iter()
                .rev()
                .filter(|event| event.edge_id == edge.edge_id)
                .take(20)
                .collect::<Vec<_>>();
            json!({
                "edge": edge,
                "metrics": store.runtime_metrics(&edge.edge_id),
                "recentEvents": events,
            })
        })
        .collect::<Vec<_>>();
    let ignored_event_count = store
        .runtime_events()
        .iter()
        .filter(|event| !edge_ids.contains(event.edge_id.as_str()))
        .count();
    let warnings = if ignored_event_count > 0 && edge_id.is_some() {
        vec![format!(
            "{ignored_event_count} out-of-scope runtime events were excluded"
        )]
    } else {
        Vec::new()
    };
    Ok(response("runtime.metrics", scope, items, warnings))
}

fn mqtt_status(
    store: &CloudControlStore,
    scope: &AgentToolScope,
    arguments: &Value,
) -> Result<Value> {
    let edge_id = scope.resolve_edge(arguments)?;
    let project_id = scope.resolve_project(arguments)?;
    let limit = result_limit(arguments)?;
    let edges = scoped_edges(store, project_id.as_deref(), edge_id.as_deref());
    if edge_id.is_some() && edges.is_empty() {
        bail!("edge is outside the active Agent scope or does not exist");
    }
    let items = edges
        .iter()
        .take(limit)
        .map(|edge| {
            let configured = store
                .mqtt_uplink(&edge.edge_id)
                .map(sanitized_mqtt_config)
                .transpose()?;
            let metrics = store
                .runtime_metrics(&edge.edge_id)
                .map(|snapshot| &snapshot.mqtt);
            Ok(json!({
                "edgeId": edge.edge_id,
                "configured": configured,
                "runtime": metrics,
            }))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(response("mqtt.status", scope, items, Vec::new()))
}

fn operations_diagnose(
    store: &CloudControlStore,
    scope: &AgentToolScope,
    arguments: &Value,
) -> Result<Value> {
    let edge_id = scope.resolve_edge(arguments)?;
    let project_id = scope.resolve_project(arguments)?;
    let limit = result_limit(arguments)?;
    let event_limit = arguments
        .get("eventLimit")
        .map(|value| {
            value
                .as_u64()
                .ok_or_else(|| anyhow!("`eventLimit` must be a positive integer"))
                .map(|value| (value as usize).clamp(1, 100))
        })
        .transpose()?
        .unwrap_or(20);
    let edges = scoped_edges(store, project_id.as_deref(), edge_id.as_deref());
    if edge_id.is_some() && edges.is_empty() {
        bail!("edge is outside the active Agent scope or does not exist");
    }

    let items = edges
        .into_iter()
        .take(limit)
        .map(|edge| diagnose_edge(store, edge, event_limit))
        .collect::<Vec<_>>();
    Ok(response("operations.diagnose", scope, items, Vec::new()))
}

fn diagnose_edge(store: &CloudControlStore, edge: &EdgeNode, event_limit: usize) -> Value {
    let mut findings = Vec::new();
    let package = store.latest_config_package_for_edge(&edge.edge_id);
    let metrics = store.runtime_metrics(&edge.edge_id);
    let events = store
        .runtime_events()
        .iter()
        .rev()
        .filter(|event| event.edge_id == edge.edge_id)
        .take(event_limit)
        .collect::<Vec<_>>();

    let configured_protocols = package
        .map(|package| {
            package
                .protocol_connections
                .iter()
                .map(|connection| connection.connection_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let configured_mqtt_sinks = package
        .map(|package| package.mqtt_uplinks.len())
        .unwrap_or_else(|| usize::from(store.mqtt_uplink(&edge.edge_id).is_some()));

    let mut metrics_age_seconds = None;
    if let Some(snapshot) = metrics {
        let age_seconds = Utc::now()
            .signed_duration_since(snapshot.timestamp)
            .num_seconds()
            .max(0) as u64;
        metrics_age_seconds = Some(age_seconds);
        if age_seconds > 120 {
            add_diagnostic_finding(
                &mut findings,
                "critical",
                "runtime",
                "runtime_metrics_stale",
                "Runtime 指标已过期",
                format!("最近一次 Runtime 指标距今 {age_seconds} 秒，当前状态不能视为实时。"),
                vec![json!({
                    "source": "runtime.metrics",
                    "path": "metrics.timestamp",
                    "value": snapshot.timestamp,
                })],
                vec![
                    "EdgeLink 会话中断".to_owned(),
                    "Runtime 进程阻塞或退出".to_owned(),
                ],
                vec!["检查 EdgeLink 会话和 Runtime 进程".to_owned()],
            );
        } else if age_seconds > 45 {
            add_diagnostic_finding(
                &mut findings,
                "warning",
                "runtime",
                "runtime_metrics_delayed",
                "Runtime 指标上报延迟",
                format!("最近一次 Runtime 指标距今 {age_seconds} 秒。"),
                vec![json!({
                    "source": "runtime.metrics",
                    "path": "metrics.timestamp",
                    "value": snapshot.timestamp,
                })],
                vec!["控制链路抖动".to_owned(), "Runtime 负载升高".to_owned()],
                vec!["观察下一次心跳并检查 CPU、内存和网络".to_owned()],
            );
        }

        if snapshot.cloud_sync.desired_version != snapshot.cloud_sync.reported_version
            || edge.desired_product_version.as_deref() != edge.reported_product_version.as_deref()
        {
            add_diagnostic_finding(
                &mut findings,
                "warning",
                "configuration",
                "configuration_revision_drift",
                "Runtime 配置版本未同步",
                format!(
                    "期望版本为 `{}`，Runtime 已上报版本为 `{}`。",
                    snapshot.cloud_sync.desired_version, snapshot.cloud_sync.reported_version
                ),
                vec![json!({
                    "source": "runtime.metrics",
                    "path": "metrics.cloud_sync",
                    "value": snapshot.cloud_sync,
                })],
                vec![
                    "配置正在应用".to_owned(),
                    "配置校验失败或 EdgeLink 中断".to_owned(),
                ],
                vec!["检查最近配置应用结果和 Runtime 事件".to_owned()],
            );
        }

        if !snapshot.cloud_sync.connected {
            add_diagnostic_finding(
                &mut findings,
                "critical",
                "runtime",
                "cloud_sync_disconnected",
                "Runtime 控制链路已断开",
                "Runtime 指标显示 EdgeLink 控制链路未连接。",
                vec![json!({
                    "source": "runtime.metrics",
                    "path": "metrics.cloud_sync.connected",
                    "value": false,
                })],
                vec!["网络不可达".to_owned(), "TLS 或接入凭证校验失败".to_owned()],
                vec!["检查 Runtime EdgeLink 日志、证书和接入令牌".to_owned()],
            );
        }

        diagnose_resource_pressure(snapshot, &mut findings);
        diagnose_collection(snapshot, &mut findings);

        let observed_protocols = snapshot
            .protocols
            .iter()
            .map(|protocol| protocol.connection_id.as_str())
            .collect::<BTreeSet<_>>();
        for connection_id in configured_protocols.difference(&observed_protocols) {
            add_diagnostic_finding(
                &mut findings,
                "warning",
                "protocol",
                "protocol_metrics_missing",
                "协议连接缺少运行指标",
                format!("配置连接 `{connection_id}` 尚未出现在 Runtime 协议指标中。"),
                vec![json!({
                    "source": "edge.config",
                    "path": "protocolConnections",
                    "value": connection_id,
                })],
                vec!["配置尚未应用".to_owned(), "协议适配器初始化失败".to_owned()],
                vec!["核对 Runtime 已应用版本并查看协议初始化事件".to_owned()],
            );
        }
        for protocol in &snapshot.protocols {
            diagnose_protocol(
                protocol,
                configured_protocols.contains(protocol.connection_id.as_str()),
                &mut findings,
            );
        }
        diagnose_mqtt(snapshot, configured_mqtt_sinks, &mut findings);
        diagnose_algorithms(snapshot, &mut findings);
        diagnose_storage(snapshot, &mut findings);
    } else {
        add_diagnostic_finding(
            &mut findings,
            "critical",
            "runtime",
            "runtime_metrics_missing",
            "没有 Runtime 运行证据",
            "控制面尚未收到该边端的 Runtime 指标，无法证明采集与 MQTT 上报正在运行。",
            vec![json!({
                "source": "cloud-control-store",
                "path": format!("runtimeMetrics.{}", edge.edge_id),
                "value": null,
            })],
            vec![
                "Runtime 未启动或未接入".to_owned(),
                "指标上报链路未配置".to_owned(),
            ],
            vec!["检查 Runtime 健康页和 EdgeLink 接入状态".to_owned()],
        );
    }

    diagnose_runtime_events(&events, &mut findings);
    let overall = diagnostic_overall(&findings);
    let confidence = match (metrics, metrics_age_seconds, package) {
        (Some(_), Some(age), Some(_)) if age <= 45 => "high",
        (Some(_), _, _) => "medium",
        _ => "low",
    };
    let summary = match overall {
        "critical" => "发现会中断采集、控制链路或 MQTT 交付的严重问题",
        "warning" => "发现需要关注的运行偏差或退化",
        _ => "当前实时证据未发现明显异常",
    };

    json!({
        "edgeId": edge.edge_id,
        "runtimeId": metrics.map(|snapshot| snapshot.runtime_id.as_str()),
        "configVersion": metrics.map(|snapshot| snapshot.config_version.as_str()),
        "metricsObservedAt": metrics.map(|snapshot| snapshot.timestamp),
        "metricsAgeSeconds": metrics_age_seconds,
        "overall": overall,
        "confidence": confidence,
        "summary": summary,
        "findingCount": findings.len(),
        "findings": findings,
        "evidenceCoverage": {
            "configuration": package.is_some(),
            "runtimeMetrics": metrics.is_some(),
            "runtimeEvents": events.len(),
            "configuredProtocols": configured_protocols.len(),
            "observedProtocols": metrics.map(|snapshot| snapshot.protocols.len()).unwrap_or_default(),
            "configuredMqttSinks": configured_mqtt_sinks,
        },
    })
}

fn diagnose_resource_pressure(
    snapshot: &edge_core::EdgeRuntimeMetricsSnapshot,
    findings: &mut Vec<Value>,
) {
    for (name, label, value) in [
        ("cpu_percent", "CPU", snapshot.system.cpu_percent),
        ("memory_percent", "内存", snapshot.system.memory_percent),
        ("disk_percent", "磁盘", snapshot.system.disk_percent),
    ] {
        if value < 80.0 {
            continue;
        }
        let severity = if value >= 90.0 { "critical" } else { "warning" };
        add_diagnostic_finding(
            findings,
            severity,
            "runtime",
            &format!("system_{}_high", name.trim_end_matches("_percent")),
            format!("Runtime {label} 使用率偏高"),
            format!("{label} 使用率为 {value:.1}%。"),
            vec![json!({
                "source": "runtime.metrics",
                "path": format!("metrics.system.{name}"),
                "value": value,
            })],
            vec!["采集或算法负载过高".to_owned(), "主机资源不足".to_owned()],
            vec![format!("检查 {label} 占用趋势和高负载任务")],
        );
    }
}

fn diagnose_collection(
    snapshot: &edge_core::EdgeRuntimeMetricsSnapshot,
    findings: &mut Vec<Value>,
) {
    let collection = &snapshot.collection;
    if collection.success_rate < 0.99 {
        let severity = if collection.success_rate < 0.95 {
            "critical"
        } else {
            "warning"
        };
        add_diagnostic_finding(
            findings,
            severity,
            "collection",
            "collection_success_rate_low",
            "点位采集成功率下降",
            format!("当前采集成功率为 {:.2}%。", collection.success_rate * 100.0),
            vec![json!({
                "source": "runtime.metrics",
                "path": "metrics.collection.success_rate",
                "value": collection.success_rate,
            })],
            vec![
                "设备通信超时".to_owned(),
                "点位地址或采集周期不合理".to_owned(),
            ],
            vec!["按协议连接查看超时、错误与断路器状态".to_owned()],
        );
    }
    if collection.bad_point_count > 0 {
        add_diagnostic_finding(
            findings,
            if collection.bad_point_count >= 10 {
                "critical"
            } else {
                "warning"
            },
            "collection",
            "bad_point_quality_detected",
            "存在质量异常点位",
            format!("最近指标中有 {} 个坏质量点位。", collection.bad_point_count),
            vec![json!({
                "source": "runtime.metrics",
                "path": "metrics.collection.bad_point_count",
                "value": collection.bad_point_count,
            })],
            vec![
                "设备返回异常码".to_owned(),
                "数据类型或地址配置不匹配".to_owned(),
            ],
            vec!["检查坏质量点位及其协议地址定义".to_owned()],
        );
    }
}

fn diagnose_protocol(
    protocol: &edge_core::ProtocolRuntimeMetrics,
    configured: bool,
    findings: &mut Vec<Value>,
) {
    let identity = format!("{} ({})", protocol.connection_id, protocol.protocol);
    if !configured {
        add_diagnostic_finding(
            findings,
            "warning",
            "protocol",
            "protocol_metric_not_in_config",
            "Runtime 上报了未配置的协议连接",
            format!("协议指标 `{identity}` 不在当前配置版本中。"),
            vec![json!({
                "source": "runtime.metrics",
                "path": "metrics.protocols.connection_id",
                "value": protocol.connection_id,
            })],
            vec![
                "Runtime 配置版本滞后".to_owned(),
                "旧连接尚未释放".to_owned(),
            ],
            vec!["核对期望版本和 Runtime 已应用版本".to_owned()],
        );
    }
    if !protocol.connected {
        add_diagnostic_finding(
            findings,
            "critical",
            "protocol",
            "protocol_disconnected",
            "工业协议连接断开",
            format!("协议连接 `{identity}` 当前未连接。"),
            vec![json!({
                "source": "runtime.metrics",
                "path": format!("metrics.protocols.{}.connected", protocol.connection_id),
                "value": false,
            })],
            vec![
                "设备地址不可达".to_owned(),
                "串口参数、站号或会话参数不匹配".to_owned(),
            ],
            vec![format!(
                "检查 `{}` 的端点、协议参数和设备状态",
                protocol.connection_id
            )],
        );
    }
    if protocol.circuit_state != ProtocolCircuitState::Closed {
        add_diagnostic_finding(
            findings,
            if protocol.circuit_state == ProtocolCircuitState::Open {
                "critical"
            } else {
                "warning"
            },
            "protocol",
            "protocol_circuit_not_closed",
            "协议断路器限制采集",
            format!(
                "协议连接 `{identity}` 的断路器状态为 {:?}。",
                protocol.circuit_state
            ),
            vec![json!({
                "source": "runtime.metrics",
                "path": format!("metrics.protocols.{}.circuit_state", protocol.connection_id),
                "value": protocol.circuit_state,
            })],
            vec!["连续通信失败触发保护".to_owned()],
            vec!["先恢复设备通信，再观察半开探测是否成功".to_owned()],
        );
    }
    if protocol.collection_attempt_count > 0 {
        let success_rate =
            protocol.collection_success_count as f64 / protocol.collection_attempt_count as f64;
        if success_rate < 0.99 {
            add_diagnostic_finding(
                findings,
                if success_rate < 0.95 {
                    "critical"
                } else {
                    "warning"
                },
                "protocol",
                "protocol_collection_degraded",
                "协议采集链路退化",
                format!(
                    "协议连接 `{identity}` 的采集成功率为 {:.2}%。",
                    success_rate * 100.0
                ),
                vec![json!({
                    "source": "runtime.metrics",
                    "path": format!("metrics.protocols.{}.collection_success_count", protocol.connection_id),
                    "value": {
                        "attempts": protocol.collection_attempt_count,
                        "successes": protocol.collection_success_count,
                    },
                })],
                vec!["链路抖动或设备响应超时".to_owned()],
                vec!["结合超时、重连和质量码检查该协议连接".to_owned()],
            );
        }
    }
    if protocol.consecutive_failure_count > 0 {
        add_diagnostic_finding(
            findings,
            if protocol.consecutive_failure_count >= 3 {
                "critical"
            } else {
                "warning"
            },
            "protocol",
            "protocol_consecutive_failures",
            "协议出现连续失败",
            format!(
                "协议连接 `{identity}` 已连续失败 {} 次。",
                protocol.consecutive_failure_count
            ),
            vec![json!({
                "source": "runtime.metrics",
                "path": format!("metrics.protocols.{}.consecutive_failure_count", protocol.connection_id),
                "value": protocol.consecutive_failure_count,
            })],
            vec!["设备离线".to_owned(), "协议参数或地址配置错误".to_owned()],
            vec!["查看该连接最近的 Runtime 协议事件".to_owned()],
        );
    }
}

fn diagnose_mqtt(
    snapshot: &edge_core::EdgeRuntimeMetricsSnapshot,
    configured_sinks: usize,
    findings: &mut Vec<Value>,
) {
    let mqtt = &snapshot.mqtt;
    if configured_sinks > 0 && mqtt.connected_sink_count < configured_sinks {
        add_diagnostic_finding(
            findings,
            if mqtt.connected_sink_count == 0 {
                "critical"
            } else {
                "warning"
            },
            "mqtt",
            "mqtt_sink_disconnected",
            "MQTT 上报连接不完整",
            format!(
                "已配置 {configured_sinks} 个 MQTT 连接，当前仅 {} 个连接在线。",
                mqtt.connected_sink_count
            ),
            vec![json!({
                "source": "runtime.metrics",
                "path": "metrics.mqtt.connected_sink_count",
                "value": mqtt.connected_sink_count,
            })],
            vec![
                "Broker 不可达".to_owned(),
                "TLS、认证或 Client ID 冲突".to_owned(),
            ],
            vec!["检查各 MQTT Sink 的 lastError 和会话状态".to_owned()],
        );
    }
    if mqtt.publish_failure_count > 0 {
        let attempts = mqtt.publish_success_count + mqtt.publish_failure_count;
        let failure_rate = mqtt.publish_failure_count as f64 / attempts.max(1) as f64;
        add_diagnostic_finding(
            findings,
            if failure_rate >= 0.05 {
                "critical"
            } else {
                "warning"
            },
            "mqtt",
            "mqtt_publish_failures",
            "MQTT 发布存在失败",
            format!(
                "累计发布失败 {} 次，失败率 {:.2}%。",
                mqtt.publish_failure_count,
                failure_rate * 100.0
            ),
            vec![json!({
                "source": "runtime.metrics",
                "path": "metrics.mqtt.publish_failure_count",
                "value": mqtt.publish_failure_count,
            })],
            vec![
                "Broker 拒绝发布".to_owned(),
                "会话断开或 QoS 应答超时".to_owned(),
            ],
            vec!["检查 Sink 错误、Topic ACL 和 Broker 指标".to_owned()],
        );
    }
    for sink in &mqtt.sinks {
        if sink.connected && sink.last_error.is_none() {
            continue;
        }
        add_diagnostic_finding(
            findings,
            if sink.connected {
                "warning"
            } else {
                "critical"
            },
            "mqtt",
            "mqtt_sink_error",
            "MQTT Sink 状态异常",
            format!(
                "Sink `{}` 连接状态为 {}，最近错误为 `{}`。",
                sink.sink_id,
                sink.connected,
                sink.last_error.as_deref().unwrap_or("无")
            ),
            vec![json!({
                "source": "runtime.metrics",
                "path": format!("metrics.mqtt.sinks.{}", sink.sink_id),
                "value": {
                    "connected": sink.connected,
                    "lastError": sink.last_error,
                    "lastPublishAt": sink.last_publish_at,
                },
            })],
            vec!["连接参数、认证或网络异常".to_owned()],
            vec![format!(
                "验证 Sink `{}` 的连接参数并查看 Runtime 日志",
                sink.sink_id
            )],
        );
    }

    let collected = snapshot
        .protocols
        .iter()
        .map(|protocol| protocol.collection_success_count)
        .sum::<u64>();
    if configured_sinks > 0 && collected > 0 && mqtt.publish_success_count == 0 {
        add_diagnostic_finding(
            findings,
            "warning",
            "pipeline",
            "collection_delivery_gap",
            "采集成功但未观察到 MQTT 输出",
            format!("协议侧已累计成功采集 {collected} 次，但 MQTT 成功发布计数仍为 0。"),
            vec![
                json!({"source": "runtime.metrics", "path": "metrics.protocols.collection_success_count", "value": collected}),
                json!({"source": "runtime.metrics", "path": "metrics.mqtt.publish_success_count", "value": 0}),
            ],
            vec![
                "采集编排触发条件尚未满足".to_owned(),
                "计算节点过滤了输出".to_owned(),
                "MQTT Sink 未连接".to_owned(),
            ],
            vec!["检查采集编排拓扑、计算节点触发条件和 MQTT Sink".to_owned()],
        );
    }
}

fn diagnose_algorithms(
    snapshot: &edge_core::EdgeRuntimeMetricsSnapshot,
    findings: &mut Vec<Value>,
) {
    for algorithm in &snapshot.algorithms {
        if algorithm.healthy && algorithm.error_count == 0 {
            continue;
        }
        add_diagnostic_finding(
            findings,
            if algorithm.healthy {
                "warning"
            } else {
                "critical"
            },
            "calculation",
            "calculation_node_unhealthy",
            "计算节点运行异常",
            format!(
                "计算 `{}` 健康状态为 {}，累计错误 {} 次。",
                algorithm.algorithm_id, algorithm.healthy, algorithm.error_count
            ),
            vec![json!({
                "source": "runtime.metrics",
                "path": format!("metrics.algorithms.{}", algorithm.algorithm_id),
                "value": algorithm,
            })],
            vec!["输入质量异常".to_owned(), "DSL 参数或表达式错误".to_owned()],
            vec!["检查计算节点输入、参数和最近错误事件".to_owned()],
        );
    }
}

fn diagnose_storage(snapshot: &edge_core::EdgeRuntimeMetricsSnapshot, findings: &mut Vec<Value>) {
    if snapshot.local_store.buffered_records > 0 {
        add_diagnostic_finding(
            findings,
            if snapshot.local_store.buffered_records >= 10_000 {
                "critical"
            } else {
                "warning"
            },
            "storage",
            "local_buffer_backlog",
            "本地待上传数据积压",
            format!(
                "本地缓存 {} 条记录，最老记录已等待 {} 秒。",
                snapshot.local_store.buffered_records,
                snapshot.local_store.oldest_buffer_age_seconds
            ),
            vec![json!({
                "source": "runtime.metrics",
                "path": "metrics.local_store",
                "value": snapshot.local_store,
            })],
            vec![
                "MQTT 上报受阻".to_owned(),
                "上游产生速率高于发送速率".to_owned(),
            ],
            vec!["优先恢复 MQTT，再观察缓存是否持续下降".to_owned()],
        );
    }
}

fn diagnose_runtime_events(events: &[&edge_core::EdgeRuntimeEvent], findings: &mut Vec<Value>) {
    let critical_count = events
        .iter()
        .filter(|event| event.severity == RuntimeEventSeverity::Critical)
        .count();
    let warning_count = events
        .iter()
        .filter(|event| event.severity == RuntimeEventSeverity::Warning)
        .count();
    if critical_count == 0 && warning_count == 0 {
        return;
    }
    let recent = events
        .iter()
        .filter(|event| event.severity != RuntimeEventSeverity::Info)
        .take(10)
        .map(|event| {
            json!({
                "code": event.code,
                "severity": event.severity,
                "category": event.category,
                "message": event.message,
                "timestamp": event.timestamp,
            })
        })
        .collect::<Vec<_>>();
    add_diagnostic_finding(
        findings,
        if critical_count > 0 {
            "critical"
        } else {
            "warning"
        },
        "events",
        "recent_runtime_events",
        "Runtime 最近记录了异常事件",
        format!("最近事件中包含 {critical_count} 条严重事件和 {warning_count} 条警告。"),
        vec![json!({
            "source": "runtime.events",
            "path": "recentEvents",
            "value": recent,
        })],
        vec!["具体原因以事件代码和上下文为准".to_owned()],
        vec!["按时间顺序关联协议、采集、存储和同步事件".to_owned()],
    );
}

#[allow(clippy::too_many_arguments)]
fn add_diagnostic_finding(
    findings: &mut Vec<Value>,
    severity: &str,
    domain: &str,
    code: &str,
    title: impl Into<String>,
    detail: impl Into<String>,
    evidence: Vec<Value>,
    probable_causes: Vec<String>,
    recommended_actions: Vec<String>,
) {
    findings.push(json!({
        "findingId": format!("{domain}:{code}:{}", findings.len() + 1),
        "severity": severity,
        "domain": domain,
        "code": code,
        "title": title.into(),
        "detail": detail.into(),
        "evidence": evidence,
        "probableCauses": probable_causes,
        "recommendedActions": recommended_actions,
    }));
}

fn diagnostic_overall(findings: &[Value]) -> &'static str {
    if findings
        .iter()
        .any(|finding| finding["severity"] == "critical")
    {
        "critical"
    } else if findings
        .iter()
        .any(|finding| finding["severity"] == "warning")
    {
        "warning"
    } else {
        "healthy"
    }
}

fn audit_search(
    store: &CloudControlStore,
    scope: &AgentToolScope,
    arguments: &Value,
) -> Result<Value> {
    let project_id = scope.resolve_project(arguments)?;
    let edge_id = scope.resolve_edge(arguments)?;
    if let Some(edge_id) = edge_id.as_deref() {
        ensure_edge_scope(store, project_id.as_deref(), edge_id)?;
    }
    let target_contains = optional_string(arguments, "targetContains")?;
    let actor = optional_string(arguments, "actor")?;
    let limit = result_limit(arguments)?;
    let product_ids = project_id
        .as_deref()
        .map(|project_id| {
            store
                .products()
                .filter(|product| product.project_id == project_id)
                .map(|product| product.product_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let items = store
        .audit_records()
        .iter()
        .rev()
        .filter(|record| {
            audit_in_scope(
                &record.target,
                project_id.as_deref(),
                edge_id.as_deref(),
                &product_ids,
            ) && target_contains
                .as_deref()
                .is_none_or(|needle| record.target.contains(needle))
                && actor.as_deref().is_none_or(|value| record.actor == value)
        })
        .take(limit)
        .map(|record| json!(record))
        .collect::<Vec<_>>();
    Ok(response("audit.search", scope, items, Vec::new()))
}

fn knowledge_search(
    store: &CloudControlStore,
    scope: &AgentToolScope,
    arguments: &Value,
) -> Result<Value> {
    let project_id = scope.resolve_project(arguments)?;
    let query =
        optional_string(arguments, "query")?.ok_or_else(|| anyhow!("`query` is required"))?;
    if query.chars().count() > 1_000 {
        bail!("`query` exceeds 1000 characters");
    }
    let limit = arguments
        .get("limit")
        .map(|value| {
            value
                .as_u64()
                .ok_or_else(|| anyhow!("`limit` must be a positive integer"))
                .map(|value| (value as usize).clamp(1, 12))
        })
        .transpose()?;
    let items = search_agent_knowledge(store, &query, project_id.as_deref(), limit)
        .into_iter()
        .map(|hit| json!(hit))
        .collect();
    Ok(response("knowledge.search", scope, items, Vec::new()))
}

fn configuration_views(
    store: &CloudControlStore,
    scope: &AgentToolScope,
    arguments: &Value,
) -> Result<Vec<ConfigurationView>> {
    let project_id = scope.resolve_project(arguments)?;
    let edge_id = scope.resolve_edge(arguments)?;
    let product_id = optional_string(arguments, "productId")?;
    let version = optional_string(arguments, "version")?;
    let limit = result_limit(arguments)?;

    if let Some(edge_id) = edge_id.as_deref() {
        let edge = ensure_edge_scope(store, project_id.as_deref(), edge_id)?;
        if let Some(product_id) = product_id.as_deref() {
            if edge.product_id.as_deref() != Some(product_id) {
                bail!("edge is not bound to requested product");
            }
        }
        let package = version
            .as_deref()
            .and_then(|version| store.config_package(edge_id, version))
            .or_else(|| store.latest_config_package_for_edge(edge_id))
            .ok_or_else(|| anyhow!("edge has no configuration package"))?;
        return Ok(vec![ConfigurationView::from_edge(package, Some(edge))?]);
    }

    let products = store
        .products()
        .filter(|product| {
            project_id
                .as_deref()
                .is_none_or(|id| product.project_id == id)
                && product_id
                    .as_deref()
                    .is_none_or(|id| product.product_id == id)
        })
        .take(limit)
        .collect::<Vec<_>>();
    if product_id.is_some() && products.is_empty() {
        bail!("product is outside the active Agent scope or does not exist");
    }
    let views = products
        .into_iter()
        .filter_map(|product| {
            select_product_version(store, product, version.as_deref())
                .map(|version| (product, version))
        })
        .map(|(product, version)| ConfigurationView::from_product(product, version))
        .collect::<Result<Vec<_>>>()?;
    Ok(views)
}

fn select_product_version<'a>(
    store: &'a CloudControlStore,
    product: &Product,
    requested_version: Option<&str>,
) -> Option<&'a ProductVersion> {
    if let Some(version) = requested_version {
        return store.product_version(&product.product_id, version);
    }
    product
        .latest_version
        .as_deref()
        .and_then(|version| store.product_version(&product.product_id, version))
        .or_else(|| {
            store
                .product_versions()
                .filter(|version| version.product_id == product.product_id)
                .max_by(|left, right| left.version.cmp(&right.version))
        })
}

fn scoped_edges<'a>(
    store: &'a CloudControlStore,
    project_id: Option<&str>,
    edge_id: Option<&str>,
) -> Vec<&'a EdgeNode> {
    store
        .edge_nodes()
        .filter(|edge| {
            project_id.is_none_or(|id| edge.project_id.as_deref() == Some(id))
                && edge_id.is_none_or(|id| edge.edge_id == id)
        })
        .collect()
}

fn ensure_edge_scope<'a>(
    store: &'a CloudControlStore,
    project_id: Option<&str>,
    edge_id: &str,
) -> Result<&'a EdgeNode> {
    let edge = store
        .edge_nodes()
        .find(|edge| edge.edge_id == edge_id)
        .ok_or_else(|| anyhow!("edge does not exist"))?;
    if project_id.is_some_and(|project_id| edge.project_id.as_deref() != Some(project_id)) {
        bail!("edge is outside the active Agent project scope");
    }
    Ok(edge)
}

fn audit_in_scope(
    target: &str,
    project_id: Option<&str>,
    edge_id: Option<&str>,
    product_ids: &BTreeSet<&str>,
) -> bool {
    if let Some(edge_id) = edge_id {
        return target.contains(edge_id);
    }
    let Some(project_id) = project_id else {
        return true;
    };
    target.contains(project_id)
        || product_ids
            .iter()
            .any(|product_id| target.contains(product_id))
}

fn response(tool: &str, scope: &AgentToolScope, items: Vec<Value>, warnings: Vec<String>) -> Value {
    json!({
        "schemaVersion": TOOL_SCHEMA_VERSION,
        "tool": tool,
        "observedAt": Utc::now(),
        "scope": scope.as_value(),
        "count": items.len(),
        "items": items,
        "warnings": warnings,
        "evidence": {"source": "cloud-control-store", "live": true},
    })
}

fn read_descriptor(
    name: &str,
    description: &str,
    category: AgentToolCategory,
    permission: AgentPermission,
    input_schema: Value,
) -> AgentToolDescriptor {
    AgentToolDescriptor {
        name: name.to_owned(),
        description: description.to_owned(),
        category,
        effect: AgentToolEffect::ReadOnly,
        exposure: AgentToolExposure::ModelCallable,
        risk: AgentRiskLevel::Low,
        confirmation: AgentConfirmationPolicy::None,
        required_permissions: [permission].into_iter().collect(),
        input_schema,
        output_schema: json!({
            "type": "object",
            "required": ["schemaVersion", "tool", "scope", "count", "items", "evidence"],
            "properties": {
                "schemaVersion": {"type": "string"},
                "tool": {"type": "string"},
                "scope": {"type": "object"},
                "count": {"type": "integer"},
                "items": {"type": "array"},
                "warnings": {"type": "array", "items": {"type": "string"}},
                "evidence": {"type": "object"}
            }
        }),
    }
}

fn diagnosis_descriptor() -> AgentToolDescriptor {
    let mut descriptor = read_descriptor(
        "operations.diagnose",
        "Correlate scoped configuration, Runtime, industrial protocol, MQTT, local buffer and event evidence into deterministic operational findings.",
        AgentToolCategory::Runtime,
        AgentPermission::RuntimeRead,
        json!({
            "type": "object",
            "properties": {
                "projectId": {"type": "string"},
                "edgeId": {"type": "string"},
                "limit": limit_schema(),
                "eventLimit": {"type": "integer", "minimum": 1, "maximum": 100}
            },
            "additionalProperties": false
        }),
    );
    descriptor.required_permissions.extend([
        AgentPermission::ConfigurationRead,
        AgentPermission::MqttRead,
    ]);
    descriptor
}

fn change_set_draft_descriptor() -> AgentToolDescriptor {
    AgentToolDescriptor {
        name: "configuration.change_set.draft".to_owned(),
        description: "Draft a non-executing, versioned product configuration ChangeSet. The control plane validates and persists the draft; a human must confirm it before apply.".to_owned(),
        category: AgentToolCategory::Configuration,
        effect: AgentToolEffect::DraftChangeSet,
        exposure: AgentToolExposure::ModelCallable,
        risk: AgentRiskLevel::Medium,
        confirmation: AgentConfirmationPolicy::None,
        required_permissions: [AgentPermission::ConfigurationPropose]
            .into_iter()
            .collect(),
        input_schema: json!({
            "type": "object",
            "required": ["projectId", "productId", "targetVersion", "title", "rationale", "patch"],
            "properties": {
                "projectId": {"type": "string", "minLength": 1},
                "productId": {"type": "string", "minLength": 1},
                "baseVersion": {"type": "string"},
                "targetVersion": {"type": "string", "minLength": 1},
                "title": {"type": "string", "minLength": 1, "maxLength": 160},
                "rationale": {"type": "string", "minLength": 1, "maxLength": 1000},
                "risk": {"type": "string", "enum": ["low", "medium", "high", "critical"]},
                "patch": {
                    "type": "object",
                    "properties": {
                        "pointSetIds": {"type": "array"},
                        "deviceModels": {"type": "array"},
                        "devices": {"type": "array"},
                        "protocolConnections": {"type": "array"},
                        "collectionTasks": {"type": "array"},
                        "algorithms": {"type": "array"},
                        "dataConfigs": {"type": "array"},
                        "commandFlows": {"type": "array"},
                        "mqttUplinks": {"type": "array"}
                    },
                    "additionalProperties": false,
                    "minProperties": 1
                }
            },
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "required": ["schemaVersion", "tool", "changeSet", "changedFields", "nextAction", "applied"],
            "properties": {
                "schemaVersion": {"type": "string"},
                "tool": {"type": "string"},
                "changeSet": {"type": "object"},
                "changedFields": {"type": "array", "items": {"type": "string"}},
                "nextAction": {"type": "string"},
                "applied": {"type": "boolean", "const": false}
            }
        }),
    }
}

fn command_draft_descriptor() -> AgentToolDescriptor {
    AgentToolDescriptor {
        name: "device.command.draft".to_owned(),
        description: "Draft a non-executing command candidate for one writable point exposed by a validated command flow. The control plane validates and persists it; a human must confirm it before MQTT dispatch.".to_owned(),
        category: AgentToolCategory::Command,
        effect: AgentToolEffect::DraftDeviceCommand,
        exposure: AgentToolExposure::ModelCallable,
        risk: AgentRiskLevel::High,
        confirmation: AgentConfirmationPolicy::None,
        required_permissions: [AgentPermission::CommandPropose].into_iter().collect(),
        input_schema: json!({
            "type": "object",
            "required": [
                "projectId", "edgeId", "flowId", "pointId", "value", "title",
                "rationale", "idempotencyKey"
            ],
            "properties": {
                "projectId": {"type": "string", "minLength": 1},
                "edgeId": {"type": "string", "minLength": 1},
                "flowId": {"type": "string", "minLength": 1},
                "pointId": {"type": "string", "minLength": 1},
                "deviceId": {"type": "string", "minLength": 1},
                "value": {},
                "title": {"type": "string", "minLength": 1, "maxLength": 160},
                "rationale": {"type": "string", "minLength": 1, "maxLength": 1000},
                "idempotencyKey": {"type": "string", "minLength": 1, "maxLength": 200}
            },
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "required": [
                "schemaVersion", "tool", "commandCandidate", "duplicate", "nextAction",
                "dispatched"
            ],
            "properties": {
                "schemaVersion": {"type": "string"},
                "tool": {"type": "string", "const": "device.command.draft"},
                "commandCandidate": {"type": "object"},
                "duplicate": {"type": "boolean"},
                "writablePoint": {"type": "object"},
                "commandFlow": {"type": "object"},
                "nextAction": {"type": "string"},
                "dispatched": {"type": "boolean", "const": false}
            }
        }),
    }
}

fn configuration_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "projectId": {"type": "string"},
            "productId": {"type": "string"},
            "version": {"type": "string"},
            "edgeId": {"type": "string"},
            "limit": limit_schema()
        },
        "additionalProperties": false
    })
}

fn product_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "projectId": {"type": "string"},
            "productId": {"type": "string"},
            "version": {"type": "string"},
            "limit": limit_schema()
        },
        "additionalProperties": false
    })
}

fn edge_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "projectId": {"type": "string"},
            "edgeId": {"type": "string"},
            "limit": limit_schema()
        },
        "additionalProperties": false
    })
}

fn limit_schema() -> Value {
    json!({"type": "integer", "minimum": 1, "maximum": MAX_RESULT_LIMIT})
}

fn optional_string(value: &Value, key: &str) -> Result<Option<String>> {
    let Some(raw) = value.get(key) else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    let raw = raw
        .as_str()
        .ok_or_else(|| anyhow!("`{key}` must be a string"))?
        .trim();
    Ok((!raw.is_empty()).then(|| raw.to_owned()))
}

fn result_limit(arguments: &Value) -> Result<usize> {
    let Some(limit) = arguments.get("limit") else {
        return Ok(DEFAULT_RESULT_LIMIT);
    };
    let limit = limit
        .as_u64()
        .ok_or_else(|| anyhow!("`limit` must be a positive integer"))? as usize;
    if limit == 0 {
        bail!("`limit` must be greater than zero");
    }
    Ok(limit.min(MAX_RESULT_LIMIT))
}

fn restrict_scope(
    field: &str,
    allowed: Option<&str>,
    requested: Option<&str>,
) -> Result<Option<String>> {
    if let (Some(allowed), Some(requested)) = (allowed, requested) {
        if allowed != requested {
            bail!("requested `{field}` is outside the active Agent scope");
        }
    }
    Ok(requested.or(allowed).map(str::to_owned))
}

fn serialize_sanitized<T: serde::Serialize>(value: &T) -> Result<Value> {
    let value = serde_json::to_value(value).context("serialize Agent tool result")?;
    Ok(sanitize_value(value))
}

fn sanitized_mqtt_config(config: &MqttUplinkConfig) -> Result<Value> {
    let mut value = serialize_sanitized(config)?;
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "passwordConfigured".to_owned(),
            json!(config.password_env.is_some()),
        );
        object.insert(
            "tlsConfigured".to_owned(),
            json!(config.tls_ca_path.is_some()),
        );
        object.remove("password_env");
        object.remove("passwordEnv");
        object.remove("tls_ca_path");
        object.remove("tlsCaPath");
    }
    Ok(value)
}

fn sanitize_value(value: Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| {
                    let normalized = key.to_ascii_lowercase().replace(['-', '_'], "");
                    let secret_value = matches!(
                        normalized.as_str(),
                        "password" | "secret" | "token" | "apikey" | "privatekey"
                    );
                    let secret_reference = matches!(
                        normalized.as_str(),
                        "passwordenv"
                            | "secretenv"
                            | "tokenenv"
                            | "apikeyenv"
                            | "privatekeypath"
                            | "userprivatekeypath"
                    );
                    if secret_value {
                        (key, Value::String("[REDACTED]".to_owned()))
                    } else if secret_reference && !value.is_null() {
                        (key, Value::String("[CONFIGURED]".to_owned()))
                    } else {
                        (key, sanitize_value(value))
                    }
                })
                .collect::<Map<_, _>>(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(sanitize_value).collect()),
        other => other,
    }
}
