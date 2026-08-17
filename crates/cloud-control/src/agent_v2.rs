use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentPermission {
    ProjectRead,
    ProductRead,
    ConfigurationRead,
    RuntimeRead,
    MqttRead,
    AuditRead,
    KnowledgeRead,
    ConfigurationPropose,
    ConfigurationApply,
    CommandPropose,
    CommandDispatch,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolCategory {
    Project,
    Product,
    Configuration,
    Runtime,
    Mqtt,
    Audit,
    Knowledge,
    Command,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolEffect {
    ReadOnly,
    DraftChangeSet,
    ApplyChangeSet,
    DraftDeviceCommand,
    DispatchDeviceCommand,
}

impl AgentToolEffect {
    pub fn mutates_state(self) -> bool {
        matches!(self, Self::ApplyChangeSet | Self::DispatchDeviceCommand)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolExposure {
    ModelCallable,
    ControlPlaneOnly,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentConfirmationPolicy {
    None,
    HumanUser,
    PrivilegedReviewer,
    TwoPerson,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolDescriptor {
    pub name: String,
    pub description: String,
    pub category: AgentToolCategory,
    pub effect: AgentToolEffect,
    pub exposure: AgentToolExposure,
    pub risk: AgentRiskLevel,
    pub confirmation: AgentConfirmationPolicy,
    pub required_permissions: BTreeSet<AgentPermission>,
    pub input_schema: Value,
    pub output_schema: Value,
}

impl AgentToolDescriptor {
    fn validate(&self) -> Result<(), AgentToolRegistryError> {
        if self.name.trim().is_empty() {
            return Err(AgentToolRegistryError::EmptyToolName);
        }
        if !self.input_schema.is_object() || !self.output_schema.is_object() {
            return Err(AgentToolRegistryError::InvalidSchema(self.name.clone()));
        }
        if self.exposure == AgentToolExposure::ModelCallable && self.effect.mutates_state() {
            return Err(AgentToolRegistryError::ModelMutationForbidden(
                self.name.clone(),
            ));
        }
        if self.effect.mutates_state() && self.confirmation == AgentConfirmationPolicy::None {
            return Err(AgentToolRegistryError::MutationRequiresConfirmation(
                self.name.clone(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolRegistry {
    tools: BTreeMap<String, AgentToolDescriptor>,
}

impl AgentToolRegistry {
    pub fn register(
        &mut self,
        descriptor: AgentToolDescriptor,
    ) -> Result<(), AgentToolRegistryError> {
        descriptor.validate()?;
        if self.tools.contains_key(&descriptor.name) {
            return Err(AgentToolRegistryError::DuplicateTool(descriptor.name));
        }
        self.tools.insert(descriptor.name.clone(), descriptor);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&AgentToolDescriptor> {
        self.tools.get(name)
    }

    pub fn descriptors(&self) -> impl Iterator<Item = &AgentToolDescriptor> {
        self.tools.values()
    }

    pub fn model_tools(&self) -> impl Iterator<Item = &AgentToolDescriptor> {
        self.tools
            .values()
            .filter(|tool| tool.exposure == AgentToolExposure::ModelCallable)
    }

    pub fn authorize(
        &self,
        tool_name: &str,
        request: &AgentAuthorizationRequest,
    ) -> Result<AgentToolDecision, AgentToolRegistryError> {
        let tool = self
            .get(tool_name)
            .ok_or_else(|| AgentToolRegistryError::UnknownTool(tool_name.to_owned()))?;

        if request.caller == AgentToolCaller::Model
            && tool.exposure == AgentToolExposure::ControlPlaneOnly
        {
            return Ok(AgentToolDecision::Denied {
                reason: "tool is reserved for the deterministic control plane".to_owned(),
                missing_permissions: BTreeSet::new(),
            });
        }

        let missing_permissions = tool
            .required_permissions
            .difference(&request.permissions)
            .copied()
            .collect::<BTreeSet<_>>();
        if !missing_permissions.is_empty() {
            return Ok(AgentToolDecision::Denied {
                reason: "caller does not have the required permissions".to_owned(),
                missing_permissions,
            });
        }

        if tool.confirmation != AgentConfirmationPolicy::None {
            let Some(confirmation) = request.confirmation.as_ref() else {
                return Ok(AgentToolDecision::ConfirmationRequired {
                    policy: tool.confirmation,
                });
            };
            if !confirmation.is_human() {
                return Ok(AgentToolDecision::Denied {
                    reason: "confirmation must be provided by a human operator".to_owned(),
                    missing_permissions: BTreeSet::new(),
                });
            }
            if tool.confirmation == AgentConfirmationPolicy::TwoPerson
                && confirmation.distinct_approvers() < 2
            {
                return Ok(AgentToolDecision::ConfirmationRequired {
                    policy: AgentConfirmationPolicy::TwoPerson,
                });
            }
        }

        Ok(AgentToolDecision::Allowed)
    }

    pub fn velaedge_v2() -> Self {
        let mut registry = Self::default();
        for tool in default_tool_descriptors() {
            registry
                .register(tool)
                .expect("built-in Agent v2 tool descriptors must be valid");
        }
        registry
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolCaller {
    Model,
    ControlPlane,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfirmationEvidence {
    pub confirmed_by: String,
    pub co_approver: Option<String>,
    pub note: Option<String>,
    pub confirmed_at: DateTime<Utc>,
}

impl AgentConfirmationEvidence {
    pub fn is_human(&self) -> bool {
        let actor = self.confirmed_by.trim();
        !actor.is_empty() && !actor.starts_with("agent:") && !actor.starts_with("model:")
    }

    pub fn distinct_approvers(&self) -> usize {
        let mut approvers = BTreeSet::new();
        if self.is_human() {
            approvers.insert(self.confirmed_by.trim());
        }
        if let Some(co_approver) = self.co_approver.as_deref() {
            let co_approver = co_approver.trim();
            if !co_approver.is_empty()
                && !co_approver.starts_with("agent:")
                && !co_approver.starts_with("model:")
            {
                approvers.insert(co_approver);
            }
        }
        approvers.len()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAuthorizationRequest {
    pub caller: AgentToolCaller,
    pub permissions: BTreeSet<AgentPermission>,
    pub confirmation: Option<AgentConfirmationEvidence>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolDecision {
    Allowed,
    ConfirmationRequired {
        policy: AgentConfirmationPolicy,
    },
    Denied {
        reason: String,
        missing_permissions: BTreeSet<AgentPermission>,
    },
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum AgentToolRegistryError {
    #[error("agent tool name must not be empty")]
    EmptyToolName,
    #[error("agent tool `{0}` is already registered")]
    DuplicateTool(String),
    #[error("agent tool `{0}` must define object input and output schemas")]
    InvalidSchema(String),
    #[error("model-callable tool `{0}` cannot mutate state")]
    ModelMutationForbidden(String),
    #[error("mutating tool `{0}` must require human confirmation")]
    MutationRequiresConfirmation(String),
    #[error("unknown agent tool `{0}`")]
    UnknownTool(String),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentResourceKind {
    Project,
    Product,
    ProductVersion,
    PointSet,
    ProtocolConnection,
    CollectionFlow,
    CommandFlow,
    MqttSink,
    EdgeBinding,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentChangeOperationKind {
    Create,
    Update,
    Delete,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentChangeOperation {
    pub operation_id: String,
    pub kind: AgentChangeOperationKind,
    pub resource_kind: AgentResourceKind,
    pub resource_id: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

impl AgentChangeOperation {
    pub fn create(
        resource_kind: AgentResourceKind,
        resource_id: impl Into<String>,
        after: Value,
    ) -> Self {
        Self::new(
            AgentChangeOperationKind::Create,
            resource_kind,
            resource_id,
            None,
            Some(after),
        )
    }

    pub fn update(
        resource_kind: AgentResourceKind,
        resource_id: impl Into<String>,
        before: Value,
        after: Value,
    ) -> Self {
        Self::new(
            AgentChangeOperationKind::Update,
            resource_kind,
            resource_id,
            Some(before),
            Some(after),
        )
    }

    pub fn delete(
        resource_kind: AgentResourceKind,
        resource_id: impl Into<String>,
        before: Value,
    ) -> Self {
        Self::new(
            AgentChangeOperationKind::Delete,
            resource_kind,
            resource_id,
            Some(before),
            None,
        )
    }

    fn new(
        kind: AgentChangeOperationKind,
        resource_kind: AgentResourceKind,
        resource_id: impl Into<String>,
        before: Option<Value>,
        after: Option<Value>,
    ) -> Self {
        Self {
            operation_id: Uuid::new_v4().to_string(),
            kind,
            resource_kind,
            resource_id: resource_id.into(),
            before,
            after,
            depends_on: Vec::new(),
        }
    }

    pub fn validate_shape(&self) -> Result<(), AgentChangeSetError> {
        let valid = match self.kind {
            AgentChangeOperationKind::Create => self.before.is_none() && self.after.is_some(),
            AgentChangeOperationKind::Update => self.before.is_some() && self.after.is_some(),
            AgentChangeOperationKind::Delete => self.before.is_some() && self.after.is_none(),
        };
        if !valid || self.resource_id.trim().is_empty() {
            return Err(AgentChangeSetError::InvalidOperation(
                self.operation_id.clone(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentChangeTarget {
    pub project_id: String,
    pub product_id: Option<String>,
    pub product_version: Option<String>,
    #[serde(default)]
    pub edge_ids: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentValidationSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentValidationIssue {
    pub severity: AgentValidationSeverity,
    pub code: String,
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentImpactSummary {
    pub affected_resources: usize,
    pub affected_edges: BTreeSet<String>,
    pub requires_runtime_sync: bool,
    pub command_path_changed: bool,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentValidationReport {
    pub valid: bool,
    pub checked_at: DateTime<Utc>,
    pub issues: Vec<AgentValidationIssue>,
    pub impact: AgentImpactSummary,
}

impl AgentValidationReport {
    pub fn new(issues: Vec<AgentValidationIssue>, impact: AgentImpactSummary) -> Self {
        let valid = !issues
            .iter()
            .any(|issue| issue.severity == AgentValidationSeverity::Error);
        Self {
            valid,
            checked_at: Utc::now(),
            issues,
            impact,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentChangeSetStatus {
    Draft,
    AwaitingConfirmation,
    Confirmed,
    Applying,
    Applied,
    Rejected,
    Failed,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentChangeSet {
    pub change_set_id: String,
    pub title: String,
    pub rationale: String,
    pub target: AgentChangeTarget,
    pub base_revision: String,
    pub operations: Vec<AgentChangeOperation>,
    pub risk: AgentRiskLevel,
    pub status: AgentChangeSetStatus,
    pub validation: Option<AgentValidationReport>,
    pub confirmation: Option<AgentConfirmationEvidence>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub result: Option<Value>,
}

impl AgentChangeSet {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        title: impl Into<String>,
        rationale: impl Into<String>,
        target: AgentChangeTarget,
        base_revision: impl Into<String>,
        operations: Vec<AgentChangeOperation>,
        risk: AgentRiskLevel,
        created_by: impl Into<String>,
    ) -> Result<Self, AgentChangeSetError> {
        if operations.is_empty() {
            return Err(AgentChangeSetError::EmptyChangeSet);
        }
        for operation in &operations {
            operation.validate_shape()?;
        }
        let now = Utc::now();
        Ok(Self {
            change_set_id: Uuid::new_v4().to_string(),
            title: title.into(),
            rationale: rationale.into(),
            target,
            base_revision: base_revision.into(),
            operations,
            risk,
            status: AgentChangeSetStatus::Draft,
            validation: None,
            confirmation: None,
            created_by: created_by.into(),
            created_at: now,
            updated_at: now,
            result: None,
        })
    }

    pub fn record_validation(
        &mut self,
        report: AgentValidationReport,
    ) -> Result<(), AgentChangeSetError> {
        if matches!(
            self.status,
            AgentChangeSetStatus::Applying
                | AgentChangeSetStatus::Applied
                | AgentChangeSetStatus::Rejected
                | AgentChangeSetStatus::Failed
        ) {
            return Err(AgentChangeSetError::InvalidTransition {
                from: self.status,
                action: "record_validation",
            });
        }
        self.status = if report.valid {
            AgentChangeSetStatus::AwaitingConfirmation
        } else {
            AgentChangeSetStatus::Draft
        };
        self.validation = Some(report);
        self.confirmation = None;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn confirm(
        &mut self,
        evidence: AgentConfirmationEvidence,
    ) -> Result<(), AgentChangeSetError> {
        if self.status != AgentChangeSetStatus::AwaitingConfirmation {
            return Err(AgentChangeSetError::InvalidTransition {
                from: self.status,
                action: "confirm",
            });
        }
        if !self.validation.as_ref().is_some_and(|report| report.valid) {
            return Err(AgentChangeSetError::ValidationRequired);
        }
        if !evidence.is_human() {
            return Err(AgentChangeSetError::HumanConfirmationRequired);
        }
        if matches!(self.risk, AgentRiskLevel::High | AgentRiskLevel::Critical)
            && evidence
                .note
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
        {
            return Err(AgentChangeSetError::ReviewNoteRequired);
        }
        self.confirmation = Some(evidence);
        self.status = AgentChangeSetStatus::Confirmed;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn start_apply(&mut self) -> Result<(), AgentChangeSetError> {
        if self.status != AgentChangeSetStatus::Confirmed {
            return Err(AgentChangeSetError::InvalidTransition {
                from: self.status,
                action: "start_apply",
            });
        }
        self.status = AgentChangeSetStatus::Applying;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn reject(
        &mut self,
        rejected_by: impl Into<String>,
        note: impl Into<String>,
    ) -> Result<(), AgentChangeSetError> {
        if matches!(
            self.status,
            AgentChangeSetStatus::Applying
                | AgentChangeSetStatus::Applied
                | AgentChangeSetStatus::Rejected
                | AgentChangeSetStatus::Failed
        ) {
            return Err(AgentChangeSetError::InvalidTransition {
                from: self.status,
                action: "reject",
            });
        }
        let rejected_by = rejected_by.into();
        if rejected_by.trim().is_empty()
            || rejected_by.starts_with("agent:")
            || rejected_by.starts_with("model:")
        {
            return Err(AgentChangeSetError::HumanConfirmationRequired);
        }
        let note = note.into();
        if note.trim().is_empty() {
            return Err(AgentChangeSetError::ReviewNoteRequired);
        }
        self.status = AgentChangeSetStatus::Rejected;
        self.confirmation = None;
        self.result = Some(json!({
            "rejectedBy": rejected_by,
            "note": note,
            "rejectedAt": Utc::now(),
        }));
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn mark_applied(&mut self, result: Value) -> Result<(), AgentChangeSetError> {
        self.finish_apply(AgentChangeSetStatus::Applied, result)
    }

    pub fn mark_failed(&mut self, result: Value) -> Result<(), AgentChangeSetError> {
        self.finish_apply(AgentChangeSetStatus::Failed, result)
    }

    fn finish_apply(
        &mut self,
        status: AgentChangeSetStatus,
        result: Value,
    ) -> Result<(), AgentChangeSetError> {
        if self.status != AgentChangeSetStatus::Applying {
            return Err(AgentChangeSetError::InvalidTransition {
                from: self.status,
                action: "finish_apply",
            });
        }
        self.status = status;
        self.result = Some(result);
        self.updated_at = Utc::now();
        Ok(())
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum AgentChangeSetError {
    #[error("change set must contain at least one operation")]
    EmptyChangeSet,
    #[error("change operation `{0}` does not match its operation kind")]
    InvalidOperation(String),
    #[error("change set validation must pass before confirmation")]
    ValidationRequired,
    #[error("change set confirmation must be provided by a human operator")]
    HumanConfirmationRequired,
    #[error("high-risk and critical change sets require a reviewer note")]
    ReviewNoteRequired,
    #[error("cannot perform `{action}` while change set is in `{from:?}` state")]
    InvalidTransition {
        from: AgentChangeSetStatus,
        action: &'static str,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCommandTarget {
    pub project_id: String,
    pub edge_id: String,
    pub product_id: Option<String>,
    pub product_version: Option<String>,
    pub flow_id: String,
    pub protocol_connection_id: String,
    pub device_id: String,
    pub point_id: String,
}

impl AgentCommandTarget {
    fn validate(&self) -> Result<(), AgentCommandCandidateError> {
        for (field, value) in [
            ("projectId", self.project_id.as_str()),
            ("edgeId", self.edge_id.as_str()),
            ("flowId", self.flow_id.as_str()),
            ("protocolConnectionId", self.protocol_connection_id.as_str()),
            ("deviceId", self.device_id.as_str()),
            ("pointId", self.point_id.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(AgentCommandCandidateError::MissingTargetField(field));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentCommandCandidateStatus {
    Draft,
    AwaitingConfirmation,
    Confirmed,
    Dispatching,
    Dispatched,
    Rejected,
    Failed,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCommandCandidate {
    pub candidate_id: String,
    pub title: String,
    pub rationale: String,
    pub target: AgentCommandTarget,
    pub value: Value,
    pub idempotency_key: String,
    pub risk: AgentRiskLevel,
    pub status: AgentCommandCandidateStatus,
    pub validation: Option<AgentValidationReport>,
    pub confirmation: Option<AgentConfirmationEvidence>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub result: Option<Value>,
}

impl AgentCommandCandidate {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        title: impl Into<String>,
        rationale: impl Into<String>,
        target: AgentCommandTarget,
        value: Value,
        idempotency_key: impl Into<String>,
        risk: AgentRiskLevel,
        created_by: impl Into<String>,
    ) -> Result<Self, AgentCommandCandidateError> {
        target.validate()?;
        let idempotency_key = idempotency_key.into();
        if idempotency_key.trim().is_empty() {
            return Err(AgentCommandCandidateError::IdempotencyKeyRequired);
        }
        let now = Utc::now();
        Ok(Self {
            candidate_id: Uuid::new_v4().to_string(),
            title: title.into(),
            rationale: rationale.into(),
            target,
            value,
            idempotency_key,
            risk,
            status: AgentCommandCandidateStatus::Draft,
            validation: None,
            confirmation: None,
            created_by: created_by.into(),
            created_at: now,
            updated_at: now,
            result: None,
        })
    }

    pub fn record_validation(
        &mut self,
        report: AgentValidationReport,
    ) -> Result<(), AgentCommandCandidateError> {
        if matches!(
            self.status,
            AgentCommandCandidateStatus::Dispatching
                | AgentCommandCandidateStatus::Dispatched
                | AgentCommandCandidateStatus::Rejected
                | AgentCommandCandidateStatus::Failed
        ) {
            return Err(AgentCommandCandidateError::InvalidTransition {
                from: self.status,
                action: "record_validation",
            });
        }
        self.status = if report.valid {
            AgentCommandCandidateStatus::AwaitingConfirmation
        } else {
            AgentCommandCandidateStatus::Draft
        };
        self.validation = Some(report);
        self.confirmation = None;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn confirm(
        &mut self,
        evidence: AgentConfirmationEvidence,
    ) -> Result<(), AgentCommandCandidateError> {
        if self.status != AgentCommandCandidateStatus::AwaitingConfirmation {
            return Err(AgentCommandCandidateError::InvalidTransition {
                from: self.status,
                action: "confirm",
            });
        }
        if !self.validation.as_ref().is_some_and(|report| report.valid) {
            return Err(AgentCommandCandidateError::ValidationRequired);
        }
        if !evidence.is_human() {
            return Err(AgentCommandCandidateError::HumanConfirmationRequired);
        }
        if evidence
            .note
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
        {
            return Err(AgentCommandCandidateError::ReviewNoteRequired);
        }
        if self.risk == AgentRiskLevel::Critical && evidence.distinct_approvers() < 2 {
            return Err(AgentCommandCandidateError::TwoPersonConfirmationRequired);
        }
        self.confirmation = Some(evidence);
        self.status = AgentCommandCandidateStatus::Confirmed;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn reject(
        &mut self,
        rejected_by: impl Into<String>,
        note: impl Into<String>,
    ) -> Result<(), AgentCommandCandidateError> {
        if matches!(
            self.status,
            AgentCommandCandidateStatus::Dispatching
                | AgentCommandCandidateStatus::Dispatched
                | AgentCommandCandidateStatus::Rejected
                | AgentCommandCandidateStatus::Failed
        ) {
            return Err(AgentCommandCandidateError::InvalidTransition {
                from: self.status,
                action: "reject",
            });
        }
        let rejected_by = rejected_by.into();
        if rejected_by.trim().is_empty()
            || rejected_by.starts_with("agent:")
            || rejected_by.starts_with("model:")
        {
            return Err(AgentCommandCandidateError::HumanConfirmationRequired);
        }
        let note = note.into();
        if note.trim().is_empty() {
            return Err(AgentCommandCandidateError::ReviewNoteRequired);
        }
        self.status = AgentCommandCandidateStatus::Rejected;
        self.confirmation = None;
        self.result = Some(json!({
            "rejectedBy": rejected_by,
            "note": note,
            "rejectedAt": Utc::now(),
        }));
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn start_dispatch(&mut self) -> Result<(), AgentCommandCandidateError> {
        if self.status != AgentCommandCandidateStatus::Confirmed {
            return Err(AgentCommandCandidateError::InvalidTransition {
                from: self.status,
                action: "start_dispatch",
            });
        }
        self.status = AgentCommandCandidateStatus::Dispatching;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn mark_dispatched(&mut self, result: Value) -> Result<(), AgentCommandCandidateError> {
        self.finish_dispatch(AgentCommandCandidateStatus::Dispatched, result)
    }

    pub fn mark_failed(&mut self, result: Value) -> Result<(), AgentCommandCandidateError> {
        self.finish_dispatch(AgentCommandCandidateStatus::Failed, result)
    }

    fn finish_dispatch(
        &mut self,
        status: AgentCommandCandidateStatus,
        result: Value,
    ) -> Result<(), AgentCommandCandidateError> {
        if self.status != AgentCommandCandidateStatus::Dispatching {
            return Err(AgentCommandCandidateError::InvalidTransition {
                from: self.status,
                action: "finish_dispatch",
            });
        }
        self.status = status;
        self.result = Some(result);
        self.updated_at = Utc::now();
        Ok(())
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum AgentCommandCandidateError {
    #[error("command target field `{0}` is required")]
    MissingTargetField(&'static str),
    #[error("command candidate requires an idempotency key")]
    IdempotencyKeyRequired,
    #[error("command candidate validation must pass before confirmation")]
    ValidationRequired,
    #[error("command confirmation must be provided by a human operator")]
    HumanConfirmationRequired,
    #[error("device commands require a reviewer note")]
    ReviewNoteRequired,
    #[error("critical device commands require two distinct human approvers")]
    TwoPersonConfirmationRequired,
    #[error("cannot perform `{action}` while command candidate is in `{from:?}` state")]
    InvalidTransition {
        from: AgentCommandCandidateStatus,
        action: &'static str,
    },
}

fn default_tool_descriptors() -> Vec<AgentToolDescriptor> {
    let descriptor = |name: &str,
                      description: &str,
                      category,
                      effect,
                      exposure,
                      risk,
                      confirmation,
                      permissions: &[AgentPermission]| AgentToolDescriptor {
        name: name.to_owned(),
        description: description.to_owned(),
        category,
        effect,
        exposure,
        risk,
        confirmation,
        required_permissions: permissions.iter().copied().collect(),
        input_schema: json!({ "type": "object", "additionalProperties": true }),
        output_schema: json!({ "type": "object", "additionalProperties": true }),
    };

    vec![
        descriptor(
            "project.list",
            "List projects available in the active operator scope.",
            AgentToolCategory::Project,
            AgentToolEffect::ReadOnly,
            AgentToolExposure::ModelCallable,
            AgentRiskLevel::Low,
            AgentConfirmationPolicy::None,
            &[AgentPermission::ProjectRead],
        ),
        descriptor(
            "product.inspect",
            "Inspect a product, protocol bindings and runtime compatibility.",
            AgentToolCategory::Product,
            AgentToolEffect::ReadOnly,
            AgentToolExposure::ModelCallable,
            AgentRiskLevel::Low,
            AgentConfirmationPolicy::None,
            &[AgentPermission::ProductRead],
        ),
        descriptor(
            "configuration.inspect",
            "Inspect point sets, protocol connections and data or command flows.",
            AgentToolCategory::Configuration,
            AgentToolEffect::ReadOnly,
            AgentToolExposure::ModelCallable,
            AgentRiskLevel::Low,
            AgentConfirmationPolicy::None,
            &[AgentPermission::ConfigurationRead],
        ),
        descriptor(
            "protocol.inspect",
            "Inspect scoped industrial protocol connections and their runtime bindings.",
            AgentToolCategory::Configuration,
            AgentToolEffect::ReadOnly,
            AgentToolExposure::ModelCallable,
            AgentRiskLevel::Low,
            AgentConfirmationPolicy::None,
            &[AgentPermission::ConfigurationRead],
        ),
        descriptor(
            "point_set.inspect",
            "Inspect reusable point sets, point addresses, access and sampling intervals.",
            AgentToolCategory::Configuration,
            AgentToolEffect::ReadOnly,
            AgentToolExposure::ModelCallable,
            AgentRiskLevel::Low,
            AgentConfirmationPolicy::None,
            &[AgentPermission::ConfigurationRead],
        ),
        descriptor(
            "collection_flow.inspect",
            "Inspect upstream collection graphs, calculations and MQTT outputs.",
            AgentToolCategory::Configuration,
            AgentToolEffect::ReadOnly,
            AgentToolExposure::ModelCallable,
            AgentRiskLevel::Low,
            AgentConfirmationPolicy::None,
            &[AgentPermission::ConfigurationRead],
        ),
        descriptor(
            "command_flow.inspect",
            "Inspect governed downstream command graphs and writable-point targets.",
            AgentToolCategory::Command,
            AgentToolEffect::ReadOnly,
            AgentToolExposure::ModelCallable,
            AgentRiskLevel::Low,
            AgentConfirmationPolicy::None,
            &[AgentPermission::ConfigurationRead],
        ),
        descriptor(
            "runtime.metrics",
            "Read runtime health, collection, processing and delivery metrics.",
            AgentToolCategory::Runtime,
            AgentToolEffect::ReadOnly,
            AgentToolExposure::ModelCallable,
            AgentRiskLevel::Low,
            AgentConfirmationPolicy::None,
            &[AgentPermission::RuntimeRead],
        ),
        descriptor(
            "mqtt.status",
            "Read MQTT connection, session, delivery and error status.",
            AgentToolCategory::Mqtt,
            AgentToolEffect::ReadOnly,
            AgentToolExposure::ModelCallable,
            AgentRiskLevel::Low,
            AgentConfirmationPolicy::None,
            &[AgentPermission::MqttRead],
        ),
        descriptor(
            "operations.diagnose",
            "Correlate configuration, Runtime, industrial protocol, MQTT, local buffer and event evidence into deterministic operational findings.",
            AgentToolCategory::Runtime,
            AgentToolEffect::ReadOnly,
            AgentToolExposure::ModelCallable,
            AgentRiskLevel::Low,
            AgentConfirmationPolicy::None,
            &[
                AgentPermission::RuntimeRead,
                AgentPermission::ConfigurationRead,
                AgentPermission::MqttRead,
            ],
        ),
        descriptor(
            "audit.search",
            "Search scoped configuration, command and runtime audit events.",
            AgentToolCategory::Audit,
            AgentToolEffect::ReadOnly,
            AgentToolExposure::ModelCallable,
            AgentRiskLevel::Low,
            AgentConfirmationPolicy::None,
            &[AgentPermission::AuditRead],
        ),
        descriptor(
            "knowledge.search",
            "Retrieve protocol manuals and runbooks with citations.",
            AgentToolCategory::Knowledge,
            AgentToolEffect::ReadOnly,
            AgentToolExposure::ModelCallable,
            AgentRiskLevel::Low,
            AgentConfirmationPolicy::None,
            &[AgentPermission::KnowledgeRead],
        ),
        descriptor(
            "configuration.change_set.draft",
            "Create a non-executing configuration ChangeSet with diff and impact metadata.",
            AgentToolCategory::Configuration,
            AgentToolEffect::DraftChangeSet,
            AgentToolExposure::ModelCallable,
            AgentRiskLevel::Medium,
            AgentConfirmationPolicy::None,
            &[AgentPermission::ConfigurationPropose],
        ),
        descriptor(
            "configuration.change_set.apply",
            "Apply a validated and confirmed ChangeSet through configuration APIs.",
            AgentToolCategory::Configuration,
            AgentToolEffect::ApplyChangeSet,
            AgentToolExposure::ControlPlaneOnly,
            AgentRiskLevel::High,
            AgentConfirmationPolicy::HumanUser,
            &[AgentPermission::ConfigurationApply],
        ),
        descriptor(
            "device.command.draft",
            "Create a non-executing command candidate for a writable point.",
            AgentToolCategory::Command,
            AgentToolEffect::DraftDeviceCommand,
            AgentToolExposure::ModelCallable,
            AgentRiskLevel::High,
            AgentConfirmationPolicy::None,
            &[AgentPermission::CommandPropose],
        ),
        descriptor(
            "device.command.dispatch",
            "Dispatch a validated command through policy and the device adapter.",
            AgentToolCategory::Command,
            AgentToolEffect::DispatchDeviceCommand,
            AgentToolExposure::ControlPlaneOnly,
            AgentRiskLevel::Critical,
            AgentConfirmationPolicy::PrivilegedReviewer,
            &[AgentPermission::CommandDispatch],
        ),
    ]
}
