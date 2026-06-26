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
        Self {
            audit_id: Uuid::new_v4(),
            action,
            target: target.into(),
            actor: "system".to_string(),
            created_at: Utc::now(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditAction {
    CreateRelease,
    ApplyRelease,
}
