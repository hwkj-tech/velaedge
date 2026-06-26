use std::collections::BTreeMap;

use edge_core::{CommandCandidate, TelemetryValue};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentCommandDraft {
    pub agent_id: String,
    pub edge_id: String,
    pub device_id: String,
    pub command: String,
    pub parameters: BTreeMap<String, TelemetryValue>,
    pub rationale: Option<String>,
}

impl AgentCommandDraft {
    pub fn new(
        agent_id: impl Into<String>,
        edge_id: impl Into<String>,
        device_id: impl Into<String>,
        command: impl Into<String>,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            edge_id: edge_id.into(),
            device_id: device_id.into(),
            command: command.into(),
            parameters: BTreeMap::new(),
            rationale: None,
        }
    }

    pub fn with_parameter(mut self, id: impl Into<String>, value: TelemetryValue) -> Self {
        self.parameters.insert(id.into(), value);
        self
    }

    pub fn with_rationale(mut self, rationale: impl Into<String>) -> Self {
        self.rationale = Some(rationale.into());
        self
    }

    pub fn into_candidate(self) -> CommandCandidate {
        let requested_by = format!("agent:{}", self.agent_id);
        CommandCandidate::new(self.edge_id, self.device_id, self.command, self.parameters)
            .requested_by(requested_by)
    }
}
