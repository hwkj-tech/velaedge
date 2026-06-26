//! Cloud control-plane primitives for fleet and configuration governance.

pub mod agent;
pub mod config;
pub mod fleet;

pub use agent::AgentCommandDraft;
pub use config::ConfigPackage;
pub use fleet::{EdgeNode, FleetRegistry};
