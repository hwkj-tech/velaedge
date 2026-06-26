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
    pub runtime: AlgorithmRuntime,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlgorithmRuntime {
    Rule,
    Wasm,
    Onnx,
    Python,
}
