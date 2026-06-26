use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CloudEnvelope<T> {
    pub message_id: Uuid,
    pub edge_id: String,
    pub schema_version: String,
    pub timestamp: DateTime<Utc>,
    pub payload: T,
}

impl<T> CloudEnvelope<T> {
    pub fn new(edge_id: impl Into<String>, payload: T) -> Self {
        Self {
            message_id: Uuid::new_v4(),
            edge_id: edge_id.into(),
            schema_version: "1.0".to_string(),
            timestamp: Utc::now(),
            payload,
        }
    }
}
