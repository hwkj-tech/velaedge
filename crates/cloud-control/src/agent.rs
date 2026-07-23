use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use edge_core::{CommandCandidate, TelemetryValue};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentProposalKind {
    ConfigSuggestion,
    PointMapping,
    RolloutPlan,
    CommandCandidate,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentProposalRisk {
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentProposalStatus {
    PendingReview,
    Approved,
    Rejected,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentProposal {
    pub proposal_id: Uuid,
    pub agent_id: String,
    pub kind: AgentProposalKind,
    pub project_id: Option<String>,
    pub edge_id: Option<String>,
    pub title: String,
    pub summary: String,
    pub payload: serde_json::Value,
    pub risk: AgentProposalRisk,
    pub status: AgentProposalStatus,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub review_note: Option<String>,
}

impl AgentProposal {
    pub fn new(
        agent_id: impl Into<String>,
        kind: AgentProposalKind,
        title: impl Into<String>,
        summary: impl Into<String>,
        created_by: impl Into<String>,
    ) -> Self {
        Self {
            proposal_id: Uuid::new_v4(),
            agent_id: agent_id.into(),
            kind,
            project_id: None,
            edge_id: None,
            title: title.into(),
            summary: summary.into(),
            payload: serde_json::json!({}),
            risk: AgentProposalRisk::Low,
            status: AgentProposalStatus::PendingReview,
            created_by: created_by.into(),
            created_at: Utc::now(),
            reviewed_by: None,
            reviewed_at: None,
            review_note: None,
        }
    }

    pub fn review(
        &mut self,
        decision: AgentProposalStatus,
        reviewer: impl Into<String>,
        note: Option<String>,
    ) -> Result<(), AgentProposalReviewError> {
        if self.status != AgentProposalStatus::PendingReview {
            return Err(AgentProposalReviewError::AlreadyReviewed);
        }
        if !matches!(
            decision,
            AgentProposalStatus::Approved | AgentProposalStatus::Rejected
        ) {
            return Err(AgentProposalReviewError::InvalidDecision);
        }

        let reviewer = reviewer.into();
        let reviewer = reviewer.trim();
        if reviewer.is_empty() {
            return Err(AgentProposalReviewError::MissingReviewer);
        }
        if reviewer == self.created_by.trim() {
            return Err(AgentProposalReviewError::SelfReview);
        }

        let note = note.and_then(|value| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        });
        if note
            .as_ref()
            .is_some_and(|value| value.chars().count() > 2_000)
        {
            return Err(AgentProposalReviewError::ReviewNoteTooLong);
        }
        if decision == AgentProposalStatus::Approved
            && self.risk == AgentProposalRisk::High
            && note.is_none()
        {
            return Err(AgentProposalReviewError::ApprovalNoteRequired);
        }

        self.status = decision;
        self.reviewed_by = Some(reviewer.to_string());
        self.reviewed_at = Some(Utc::now());
        self.review_note = note;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum AgentProposalReviewError {
    #[error("agent proposal has already been reviewed")]
    AlreadyReviewed,
    #[error("agent proposal review decision must approve or reject")]
    InvalidDecision,
    #[error("agent proposal reviewer must not be empty")]
    MissingReviewer,
    #[error("agent proposal creator cannot review their own proposal")]
    SelfReview,
    #[error("high-risk agent proposal approval requires a review note")]
    ApprovalNoteRequired,
    #[error("agent proposal review note must not exceed 2000 characters")]
    ReviewNoteTooLong,
}

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
