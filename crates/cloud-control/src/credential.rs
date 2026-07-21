use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EdgeAccessCredential {
    pub credential_id: Uuid,
    pub edge_id: String,
    pub token_hash: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

impl EdgeAccessCredential {
    pub fn new(edge_id: impl Into<String>, token_hash: impl Into<String>) -> Self {
        Self {
            credential_id: Uuid::new_v4(),
            edge_id: edge_id.into(),
            token_hash: token_hash.into(),
            active: true,
            created_at: Utc::now(),
        }
    }
}
