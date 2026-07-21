use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditRecord {
    pub audit_id: Uuid,
    pub action: AuditAction,
    pub target: String,
    pub actor: String,
    pub created_at: DateTime<Utc>,
}

impl AuditRecord {
    pub fn system(action: AuditAction, target: impl Into<String>) -> Self {
        Self::by_actor(action, target, "system")
    }

    pub fn by_actor(
        action: AuditAction,
        target: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        Self {
            audit_id: Uuid::new_v4(),
            action,
            target: target.into(),
            actor: actor.into(),
            created_at: Utc::now(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditAction {
    CreateRelease,
    ApplyRelease,
    UpdateConfig,
    CreateAgentProposal,
    ApproveAgentProposal,
    RejectAgentProposal,
    CreateKnowledgeDocument,
    UpdateKnowledgeDocument,
    DeleteKnowledgeDocument,
    CreateAgentConversation,
    DeleteAgentConversation,
}
