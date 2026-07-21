use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const MAX_CONVERSATION_MESSAGES: usize = 200;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentConversation {
    pub conversation_id: Uuid,
    pub project_id: Option<String>,
    pub edge_id: Option<String>,
    pub operator_id: String,
    pub title: String,
    pub messages: Vec<AgentConversationMessage>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AgentConversation {
    pub fn new(
        project_id: Option<String>,
        edge_id: Option<String>,
        operator_id: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            conversation_id: Uuid::new_v4(),
            project_id,
            edge_id,
            operator_id: operator_id.into(),
            title: title.into(),
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn push_message(&mut self, message: AgentConversationMessage) {
        self.updated_at = message.created_at;
        self.messages.push(message);
        if self.messages.len() > MAX_CONVERSATION_MESSAGES {
            self.messages
                .drain(..self.messages.len() - MAX_CONVERSATION_MESSAGES);
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentConversationMessage {
    pub message_id: Uuid,
    pub role: AgentConversationRole,
    pub content: String,
    pub citations: Vec<AgentConversationCitation>,
    pub created_at: DateTime<Utc>,
}

impl AgentConversationMessage {
    pub fn new(role: AgentConversationRole, content: impl Into<String>) -> Self {
        Self {
            message_id: Uuid::new_v4(),
            role,
            content: content.into(),
            citations: Vec::new(),
            created_at: Utc::now(),
        }
    }

    pub fn with_citations(mut self, citations: Vec<AgentConversationCitation>) -> Self {
        self.citations = citations;
        self
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentConversationRole {
    User,
    Assistant,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentConversationCitation {
    pub document_id: String,
    pub title: String,
    pub source_uri: Option<String>,
    pub excerpt: String,
}
