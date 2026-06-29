use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DeviceSpec {
    pub device_type: String,
    pub version: String,
    pub telemetry: Vec<TelemetryPoint>,
    pub commands: Vec<CommandSpec>,
    pub events: Vec<EventSpec>,
}

impl DeviceSpec {
    pub fn new(device_type: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            device_type: device_type.into(),
            version: version.into(),
            telemetry: Vec::new(),
            commands: Vec::new(),
            events: Vec::new(),
        }
    }

    pub fn with_telemetry(mut self, telemetry: Vec<TelemetryPoint>) -> Self {
        self.telemetry = telemetry;
        self
    }

    pub fn with_commands(mut self, commands: Vec<CommandSpec>) -> Self {
        self.commands = commands;
        self
    }

    pub fn with_events(mut self, events: Vec<EventSpec>) -> Self {
        self.events = events;
        self
    }

    pub fn telemetry(&self, id: &str) -> Option<&TelemetryPoint> {
        self.telemetry.iter().find(|point| point.id == id)
    }

    pub fn command(&self, id: &str) -> Option<&CommandSpec> {
        self.commands.iter().find(|command| command.id == id)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TelemetryPoint {
    pub id: String,
    pub value_type: TelemetryType,
    pub unit: Option<String>,
    pub range: Option<NumberRange>,
    pub description: Option<String>,
}

impl TelemetryPoint {
    pub fn new(id: impl Into<String>, value_type: TelemetryType) -> Self {
        Self {
            id: id.into(),
            value_type,
            unit: None,
            range: None,
            description: None,
        }
    }

    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    pub fn with_range(mut self, range: NumberRange) -> Self {
        self.range = Some(range);
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TelemetryType {
    Float,
    Integer,
    Boolean,
    Text,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct NumberRange {
    pub min: f64,
    pub max: f64,
}

impl NumberRange {
    pub fn new(min: f64, max: f64) -> Self {
        Self { min, max }
    }

    pub fn contains(&self, value: f64) -> bool {
        value >= self.min && value <= self.max
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TelemetrySample {
    pub device_id: String,
    pub telemetry_id: String,
    pub value: TelemetryValue,
    pub quality: DataQuality,
    pub timestamp: DateTime<Utc>,
}

impl TelemetrySample {
    pub fn new(
        device_id: impl Into<String>,
        telemetry_id: impl Into<String>,
        value: TelemetryValue,
        quality: DataQuality,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            device_id: device_id.into(),
            telemetry_id: telemetry_id.into(),
            value,
            quality,
            timestamp,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum TelemetryValue {
    Float(f64),
    Integer(i64),
    Boolean(bool),
    Text(String),
}

impl TelemetryValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(value) => Some(*value),
            Self::Integer(value) => Some(*value as f64),
            Self::Boolean(_) | Self::Text(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DataQuality {
    Good,
    Uncertain,
    Bad,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CommandSpec {
    pub id: String,
    pub risk: CommandRisk,
    pub parameters: Vec<CommandParameter>,
    pub requires_confirmation: bool,
}

impl CommandSpec {
    pub fn new(id: impl Into<String>, risk: CommandRisk) -> Self {
        Self {
            id: id.into(),
            risk,
            parameters: Vec::new(),
            requires_confirmation: matches!(risk, CommandRisk::High | CommandRisk::Critical),
        }
    }

    pub fn with_parameter(mut self, parameter: CommandParameter) -> Self {
        self.parameters.push(parameter);
        self
    }

    pub fn requiring_confirmation(mut self) -> Self {
        self.requires_confirmation = true;
        self
    }

    pub fn parameter(&self, id: &str) -> Option<&CommandParameter> {
        self.parameters.iter().find(|parameter| parameter.id == id)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CommandParameter {
    pub id: String,
    pub value_type: TelemetryType,
    pub range: Option<NumberRange>,
}

impl CommandParameter {
    pub fn new(id: impl Into<String>, value_type: TelemetryType) -> Self {
        Self {
            id: id.into(),
            value_type,
            range: None,
        }
    }

    pub fn with_range(mut self, range: NumberRange) -> Self {
        self.range = Some(range);
        self
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum CommandRisk {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CommandCandidate {
    pub id: Uuid,
    pub edge_id: String,
    pub device_id: String,
    pub command: String,
    pub parameters: BTreeMap<String, TelemetryValue>,
    pub requested_by: String,
    pub confirmation_token: Option<String>,
}

impl CommandCandidate {
    pub fn new(
        edge_id: impl Into<String>,
        device_id: impl Into<String>,
        command: impl Into<String>,
        parameters: BTreeMap<String, TelemetryValue>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            edge_id: edge_id.into(),
            device_id: device_id.into(),
            command: command.into(),
            parameters,
            requested_by: "unknown".to_string(),
            confirmation_token: None,
        }
    }

    pub fn requested_by(mut self, requested_by: impl Into<String>) -> Self {
        self.requested_by = requested_by.into();
        self
    }

    pub fn with_confirmation_token(mut self, token: impl Into<String>) -> Self {
        self.confirmation_token = Some(token.into());
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EventSpec {
    pub id: String,
    pub severity: EventSeverity,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AlgorithmSpec {
    pub id: String,
    pub version: String,
    #[serde(default = "default_algorithm_kind")]
    pub kind: AlgorithmKind,
    #[serde(default)]
    pub dsl: AlgorithmDsl,
    pub runtime: AlgorithmRuntime,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

impl AlgorithmSpec {
    pub fn dsl(
        id: impl Into<String>,
        version: impl Into<String>,
        kind: AlgorithmKind,
        dsl: AlgorithmDsl,
    ) -> Self {
        let inputs = dsl
            .inputs
            .iter()
            .map(|input| input.point_id.clone())
            .collect();
        let outputs = dsl
            .outputs
            .iter()
            .map(|output| output.point_id.clone())
            .collect();
        Self {
            id: id.into(),
            version: version.into(),
            kind,
            dsl,
            runtime: AlgorithmRuntime::Rule,
            inputs,
            outputs,
        }
    }

    pub fn inputs(&self) -> Vec<String> {
        if self.dsl.inputs.is_empty() {
            return self.inputs.clone();
        }
        self.dsl
            .inputs
            .iter()
            .map(|input| input.point_id.clone())
            .collect()
    }

    pub fn outputs(&self) -> Vec<String> {
        if self.dsl.outputs.is_empty() {
            return self.outputs.clone();
        }
        self.dsl
            .outputs
            .iter()
            .map(|output| output.point_id.clone())
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlgorithmRuntime {
    Rule,
    Wasm,
    Onnx,
    Python,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlgorithmKind {
    ChangeReport,
    WindowAggregate,
    ExpressionAggregate,
    ThresholdRule,
    DurationRule,
    Deadband,
    Debounce,
    Statistics,
}

fn default_algorithm_kind() -> AlgorithmKind {
    AlgorithmKind::ChangeReport
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AlgorithmDsl {
    #[serde(default)]
    pub inputs: Vec<AlgorithmInputBinding>,
    pub trigger: AlgorithmTrigger,
    #[serde(default)]
    pub steps: Vec<AlgorithmStep>,
    #[serde(default)]
    pub outputs: Vec<AlgorithmOutput>,
    pub report: AlgorithmReportPolicy,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AlgorithmInputBinding {
    pub alias: String,
    pub point_id: String,
}

impl AlgorithmInputBinding {
    pub fn new(alias: impl Into<String>, point_id: impl Into<String>) -> Self {
        Self {
            alias: alias.into(),
            point_id: point_id.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AlgorithmTrigger {
    OnSample,
    OnAnyInput,
    Window {
        #[serde(rename = "everyMs")]
        every_ms: u64,
    },
}

impl Default for AlgorithmTrigger {
    fn default() -> Self {
        Self::OnSample
    }
}

impl AlgorithmTrigger {
    pub fn on_sample() -> Self {
        Self::OnSample
    }

    pub fn on_any_input() -> Self {
        Self::OnAnyInput
    }

    pub fn window(every_ms: u64) -> Self {
        Self::Window { every_ms }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AlgorithmStep {
    ChangeFilter {
        source: String,
        threshold: f64,
    },
    WindowAggregate {
        source: String,
        functions: Vec<WindowAggregateFunction>,
    },
    Expression {
        output: String,
        expr: String,
    },
    ThresholdRule {
        source: String,
        operator: CompareOperator,
        threshold: f64,
        event: AlgorithmEventOutput,
    },
}

impl AlgorithmStep {
    pub fn change_filter(source: impl Into<String>, threshold: f64) -> Self {
        Self::ChangeFilter {
            source: source.into(),
            threshold,
        }
    }

    pub fn window_aggregate(
        source: impl Into<String>,
        functions: Vec<WindowAggregateFunction>,
    ) -> Self {
        Self::WindowAggregate {
            source: source.into(),
            functions,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "function", rename_all = "camelCase")]
pub enum WindowAggregateFunction {
    Avg { output: String },
    Min { output: String },
    Max { output: String },
    Sum { output: String },
    Count { output: String },
    First { output: String },
    Last { output: String },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompareOperator {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
    Ne,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AlgorithmEventOutput {
    pub code: String,
    pub severity: EventSeverity,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AlgorithmOutput {
    pub name: String,
    pub point_id: String,
}

impl AlgorithmOutput {
    pub fn virtual_point(name: impl Into<String>, point_id: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            point_id: point_id.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AlgorithmReportPolicy {
    pub mode: AlgorithmReportMode,
    pub sink: String,
}

impl Default for AlgorithmReportPolicy {
    fn default() -> Self {
        Self::new(AlgorithmReportMode::OnOutput, "velamq-main")
    }
}

impl AlgorithmReportPolicy {
    pub fn new(mode: AlgorithmReportMode, sink: impl Into<String>) -> Self {
        Self {
            mode,
            sink: sink.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlgorithmReportMode {
    OnOutput,
    OnChange,
    WindowResult,
    EventOnly,
}
