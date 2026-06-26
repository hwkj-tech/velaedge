//! Cloud control-plane primitives for fleet and configuration governance.

pub mod agent;
pub mod audit;
pub mod config;
pub mod fleet;
pub mod release;
pub mod store;
pub mod validation;

pub use agent::AgentCommandDraft;
pub use audit::{AuditAction, AuditRecord};
pub use config::ConfigPackage;
pub use fleet::{EdgeNode, FleetRegistry};
pub use release::{ReleaseRecord, ReleaseService, ReleaseStatus};
pub use store::CloudControlStore;
pub use validation::{ConfigValidator, ValidationError};
