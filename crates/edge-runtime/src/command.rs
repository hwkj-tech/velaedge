use std::collections::{BTreeMap, BTreeSet, VecDeque};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use edge_core::{
    CommandFlowConfig, CommandGraphNode, CommandGraphNodeKind, EdgeConfigPackage,
    TelemetryPointMapping, TelemetryType, TelemetryValue,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{MqttPublishMessage, ProtocolWriteResult};

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedPointWrite {
    pub node_id: String,
    pub mapping: TelemetryPointMapping,
    pub value: TelemetryValue,
    pub verification: CommandWriteVerification,
    pub readback_tolerance: f64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandWriteVerification {
    #[default]
    Response,
    Readback,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandExecutionPlan {
    pub flow: CommandFlowConfig,
    pub command_id: String,
    pub command_source: Option<String>,
    pub writes: Vec<PlannedPointWrite>,
    pub reply_node_ids: Vec<String>,
    pub safety_gates: Vec<PlannedCommandSafetyGate>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedCommandSafetyGate {
    pub node_id: String,
    pub source_path: String,
    pub source: Option<String>,
    pub allowed_sources: Vec<String>,
    pub max_commands: Option<u32>,
    pub window_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandExecutionStatus {
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandWriteRecord {
    pub node_id: String,
    pub point_id: String,
    pub device_id: String,
    pub value: TelemetryValue,
    pub verified: bool,
    #[serde(default)]
    pub verification: CommandWriteVerification,
    #[serde(default)]
    pub readback_value: Option<TelemetryValue>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandExecutionReport {
    pub flow_id: String,
    pub command_id: String,
    #[serde(default)]
    pub source: Option<String>,
    pub status: CommandExecutionStatus,
    pub writes: Vec<CommandWriteRecord>,
    pub error: Option<String>,
    pub completed_at: DateTime<Utc>,
    #[serde(default)]
    pub duplicate: bool,
    #[serde(default)]
    pub replies: Vec<MqttPublishMessage>,
}

impl CommandExecutionReport {
    pub fn new(flow_id: impl Into<String>, command_id: impl Into<String>) -> Self {
        Self {
            flow_id: flow_id.into(),
            command_id: command_id.into(),
            source: None,
            status: CommandExecutionStatus::Succeeded,
            writes: Vec::new(),
            error: None,
            completed_at: Utc::now(),
            duplicate: false,
            replies: Vec::new(),
        }
    }

    pub fn record_write(&mut self, write: &PlannedPointWrite, result: ProtocolWriteResult) {
        self.writes.push(CommandWriteRecord {
            node_id: write.node_id.clone(),
            point_id: result.point_id,
            device_id: write.mapping.device_id.clone(),
            value: result.value,
            verified: result.verified,
            verification: write.verification,
            readback_value: result.readback_value,
        });
    }

    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = CommandExecutionStatus::Failed;
        self.error = Some(error.into());
        self.completed_at = Utc::now();
    }
}

pub fn plan_command_execution(
    package: &EdgeConfigPackage,
    flow_id: &str,
    payload: &[u8],
) -> Result<CommandExecutionPlan> {
    let flow = package
        .command_flows
        .iter()
        .find(|flow| flow.flow_id == flow_id)
        .cloned()
        .with_context(|| format!("command flow not found: {flow_id}"))?;
    if !flow.enabled {
        bail!("command flow {flow_id} is disabled");
    }
    let document: Value =
        serde_json::from_slice(payload).context("command payload must be valid JSON")?;
    let command_id = string_at(&document, "commandId")
        .or_else(|| string_at(&document, "command_id"))
        .filter(|value| !value.trim().is_empty())
        .context("command payload requires commandId")?
        .to_string();
    validate_expiry(&document)?;

    let nodes = flow
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mappings = package
        .point_mappings
        .iter()
        .map(|mapping| (mapping.point_id.as_str(), mapping))
        .collect::<BTreeMap<_, _>>();
    let mut queue = flow
        .nodes
        .iter()
        .filter(|node| node.kind == CommandGraphNodeKind::MqttInput)
        .map(|node| node.node_id.clone())
        .collect::<VecDeque<_>>();
    let mut visited = BTreeSet::new();
    let mut writes = Vec::new();
    let mut reply_node_ids = Vec::new();
    let mut safety_gates = Vec::new();

    while let Some(node_id) = queue.pop_front() {
        if !visited.insert(node_id.clone()) {
            continue;
        }
        let node = nodes
            .get(node_id.as_str())
            .copied()
            .with_context(|| format!("command graph node not found: {node_id}"))?;

        let selected_port = match node.kind {
            CommandGraphNodeKind::MqttInput | CommandGraphNodeKind::JsonParse => None,
            CommandGraphNodeKind::Condition => Some(if evaluate_condition(node, &document)? {
                "true"
            } else {
                "false"
            }),
            CommandGraphNodeKind::SafetyGate => {
                safety_gates.push(plan_safety_gate(node, &document)?);
                None
            }
            CommandGraphNodeKind::PointWrite => {
                if let Some(write) = plan_point_write(node, &document, &mappings)? {
                    writes.push(write);
                    None
                } else {
                    continue;
                }
            }
            CommandGraphNodeKind::MqttReply => {
                reply_node_ids.push(node.node_id.clone());
                continue;
            }
        };

        for edge in flow.edges.iter().filter(|edge| edge.from == node_id) {
            if edge.from_port.as_deref().is_none()
                || selected_port.is_none()
                || edge.from_port.as_deref() == selected_port
            {
                queue.push_back(edge.to.clone());
            }
        }
    }

    if writes.is_empty() {
        bail!("command flow {flow_id} did not select a writable point");
    }
    if reply_node_ids.is_empty() {
        bail!("command flow {flow_id} did not reach a reply node");
    }

    let command_source = safety_gates
        .iter()
        .find_map(|gate| gate.source.clone())
        .or_else(|| string_at(&document, "requestedBy").map(ToString::to_string));

    Ok(CommandExecutionPlan {
        flow,
        command_id,
        command_source,
        writes,
        reply_node_ids,
        safety_gates,
    })
}

pub fn build_command_reply_messages(
    package: &EdgeConfigPackage,
    plan: &CommandExecutionPlan,
    report: &CommandExecutionReport,
) -> Result<Vec<MqttPublishMessage>> {
    let uplink = package
        .mqtt_uplinks
        .iter()
        .find(|uplink| uplink.sink_id == plan.flow.mqtt_connection_id)
        .with_context(|| {
            format!(
                "command flow {} MQTT connection not found: {}",
                plan.flow.flow_id, plan.flow.mqtt_connection_id
            )
        })?;
    let payload = serde_json::to_vec(&json!({
        "flowId": report.flow_id,
        "commandId": report.command_id,
        "source": report.source,
        "status": report.status,
        "writes": report.writes,
        "error": report.error,
        "completedAt": report.completed_at,
    }))?;
    let point_id = report
        .writes
        .first()
        .map(|write| write.point_id.as_str())
        .unwrap_or("unknown");
    let device_id = report
        .writes
        .first()
        .map(|write| write.device_id.as_str())
        .unwrap_or("unknown");
    let topic = plan
        .flow
        .reply_topic_template
        .replace("{edge_id}", &package.edge_id)
        .replace("{command_id}", &report.command_id)
        .replace("{device_id}", device_id)
        .replace("{point_id}", point_id);

    Ok(plan
        .reply_node_ids
        .iter()
        .map(|_| MqttPublishMessage {
            sink_id: uplink.sink_id.clone(),
            broker: uplink.broker.clone(),
            client_id: uplink.client_id.clone(),
            topic: topic.clone(),
            qos: plan.flow.qos,
            payload: payload.clone(),
        })
        .collect())
}

fn plan_point_write(
    node: &CommandGraphNode,
    document: &Value,
    mappings: &BTreeMap<&str, &TelemetryPointMapping>,
) -> Result<Option<PlannedPointWrite>> {
    let point_id = node.ref_id.as_deref().unwrap_or_default();
    let mapping = mappings
        .get(point_id)
        .copied()
        .with_context(|| format!("write node {} point not found: {point_id}", node.node_id))?;
    if !mapping.access.is_writable() {
        bail!("point {point_id} is not writable");
    }
    if let Some(requested_point) = string_at(document, "pointId") {
        if requested_point != point_id {
            return Ok(None);
        }
    }
    if let Some(requested_device) = string_at(document, "deviceId") {
        if requested_device != mapping.device_id {
            bail!(
                "command device {} does not match point {} device {}",
                requested_device,
                point_id,
                mapping.device_id
            );
        }
    }

    let value_path = node
        .params
        .get("value_path")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("values.{point_id}"));
    let raw_value = value_at(document, &value_path)
        .or_else(|| value_at(document, "value"))
        .with_context(|| {
            format!(
                "command payload has no value for point {point_id}; expected `{value_path}` or `value`"
            )
        })?;
    let value = telemetry_value(raw_value, mapping.value_type)
        .with_context(|| format!("invalid command value for point {point_id}"))?;
    if let (Some(range), Some(numeric)) = (mapping.range, value.as_f64()) {
        if !range.contains(numeric) {
            bail!(
                "command value {numeric} for point {point_id} is outside [{}, {}]",
                range.min,
                range.max
            );
        }
    }
    let verification = node
        .params
        .get("verification")
        .or_else(|| node.params.get("verify_mode"))
        .and_then(Value::as_str)
        .map(|value| match value {
            "response" => Ok(CommandWriteVerification::Response),
            "readback" => Ok(CommandWriteVerification::Readback),
            unsupported => bail!(
                "write node {} verification mode is unsupported: {unsupported}",
                node.node_id
            ),
        })
        .transpose()?
        .unwrap_or_default();
    let default_tolerance = if mapping.value_type == TelemetryType::Float {
        1.0e-6
    } else {
        0.0
    };
    let readback_tolerance = node
        .params
        .get("readback_tolerance")
        .map(|value| {
            value.as_f64().with_context(|| {
                format!(
                    "write node {} readback_tolerance must be a number",
                    node.node_id
                )
            })
        })
        .transpose()?
        .unwrap_or(default_tolerance);
    if !readback_tolerance.is_finite() || readback_tolerance < 0.0 {
        bail!(
            "write node {} readback_tolerance must be finite and non-negative",
            node.node_id
        );
    }
    Ok(Some(PlannedPointWrite {
        node_id: node.node_id.clone(),
        mapping: mapping.clone(),
        value,
        verification,
        readback_tolerance,
    }))
}

pub fn command_values_match(
    expected: &TelemetryValue,
    actual: &TelemetryValue,
    tolerance: f64,
) -> bool {
    match (expected.as_f64(), actual.as_f64()) {
        (Some(expected), Some(actual)) => (expected - actual).abs() <= tolerance,
        _ => expected == actual,
    }
}

fn evaluate_condition(node: &CommandGraphNode, document: &Value) -> Result<bool> {
    let path = node
        .params
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or("value");
    let operator = node
        .params
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or("eq");
    let actual = value_at(document, path)
        .with_context(|| format!("condition node {} path not found: {path}", node.node_id))?;
    let expected = node.params.get("value").unwrap_or(&Value::Bool(true));
    match operator {
        "eq" => Ok(actual == expected),
        "ne" => Ok(actual != expected),
        "gt" | "gte" | "lt" | "lte" => {
            let actual = actual
                .as_f64()
                .context("numeric condition actual value must be a number")?;
            let expected = expected
                .as_f64()
                .context("numeric condition expected value must be a number")?;
            Ok(match operator {
                "gt" => actual > expected,
                "gte" => actual >= expected,
                "lt" => actual < expected,
                "lte" => actual <= expected,
                _ => unreachable!(),
            })
        }
        _ => bail!(
            "condition node {} operator is unsupported: {operator}",
            node.node_id
        ),
    }
}

fn plan_safety_gate(node: &CommandGraphNode, document: &Value) -> Result<PlannedCommandSafetyGate> {
    validate_expiry(document)?;
    if node
        .params
        .get("require_confirmation")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && string_at(document, "confirmationToken")
            .filter(|token| !token.trim().is_empty())
            .is_none()
    {
        bail!("safety gate {} requires confirmationToken", node.node_id);
    }
    let source_path = node
        .params
        .get("source_path")
        .and_then(Value::as_str)
        .unwrap_or("requestedBy")
        .to_string();
    let source = string_at(document, &source_path).map(ToString::to_string);
    let allowed_sources = node
        .params
        .get("allowed_sources")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();
    let max_commands = node
        .params
        .get("max_commands")
        .and_then(Value::as_u64)
        .map(u32::try_from)
        .transpose()
        .with_context(|| format!("safety gate {} max_commands is too large", node.node_id))?;
    let window_ms = node.params.get("window_ms").and_then(Value::as_u64);
    Ok(PlannedCommandSafetyGate {
        node_id: node.node_id.clone(),
        source_path,
        source,
        allowed_sources,
        max_commands,
        window_ms,
    })
}

fn validate_expiry(document: &Value) -> Result<()> {
    let Some(expires_at) = string_at(document, "expiresAt") else {
        return Ok(());
    };
    let expires_at = DateTime::parse_from_rfc3339(expires_at)
        .context("command expiresAt must use RFC3339")?
        .with_timezone(&Utc);
    if expires_at <= Utc::now() {
        bail!("command has expired");
    }
    Ok(())
}

fn telemetry_value(value: &Value, value_type: TelemetryType) -> Result<TelemetryValue> {
    Ok(match value_type {
        TelemetryType::Float => TelemetryValue::Float(
            value
                .as_f64()
                .context("float point requires a JSON number")?,
        ),
        TelemetryType::Integer => TelemetryValue::Integer(
            value
                .as_i64()
                .context("integer point requires a JSON integer")?,
        ),
        TelemetryType::Boolean => TelemetryValue::Boolean(
            value
                .as_bool()
                .context("boolean point requires true or false")?,
        ),
        TelemetryType::Text => TelemetryValue::Text(
            value
                .as_str()
                .context("text point requires a JSON string")?
                .to_string(),
        ),
    })
}

fn string_at<'a>(document: &'a Value, path: &str) -> Option<&'a str> {
    value_at(document, path).and_then(Value::as_str)
}

fn value_at<'a>(document: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .filter(|segment| !segment.is_empty())
        .try_fold(document, |value, segment| value.get(segment))
}
