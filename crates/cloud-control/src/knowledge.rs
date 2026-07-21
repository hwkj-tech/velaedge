use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocument {
    pub document_id: Uuid,
    pub project_id: Option<String>,
    pub title: String,
    pub source_uri: Option<String>,
    pub content: String,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl KnowledgeDocument {
    pub fn new(
        project_id: Option<String>,
        title: impl Into<String>,
        content: impl Into<String>,
        created_by: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            document_id: Uuid::new_v4(),
            project_id,
            title: title.into(),
            source_uri: None,
            content: content.into(),
            tags: Vec::new(),
            enabled: true,
            created_by: created_by.into(),
            created_at: now,
            updated_at: now,
        }
    }
}
