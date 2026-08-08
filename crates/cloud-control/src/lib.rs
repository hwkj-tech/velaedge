//! Cloud control-plane primitives for fleet and configuration governance.

pub mod agent;
pub mod audit;
pub mod catalog;
pub mod config;
pub mod conversation;
pub mod credential;
pub mod fleet;
pub mod knowledge;
pub mod release;
pub mod sqlite;
pub mod store;
pub mod templates;
pub mod validation;

pub use agent::{
    AgentCommandDraft, AgentProposal, AgentProposalKind, AgentProposalReviewError,
    AgentProposalRisk, AgentProposalStatus,
};
pub use audit::{AuditAction, AuditRecord};
pub use catalog::{
    PointSet, PointSetPoint, Product, ProductVersion, ProductVersionStatus, Project,
};
pub use config::ConfigPackage;
pub use conversation::{
    AgentConversation, AgentConversationCitation, AgentConversationMessage, AgentConversationRole,
};
pub use credential::EdgeAccessCredential;
pub use fleet::{EdgeNode, FleetRegistry};
pub use knowledge::KnowledgeDocument;
pub use release::{ReleaseRecord, ReleaseService, ReleaseStatus};
pub use sqlite::SqliteCloudStore;
pub use store::CloudControlStore;
pub use templates::{
    manufacturer_product_templates, ProductTemplateBundle, OMRON_FINS_TEMPLATE_ID,
    SIEMENS_S7_TEMPLATE_ID,
};
pub use validation::{ConfigValidator, ValidationError};
