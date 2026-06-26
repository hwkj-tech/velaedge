//! Deterministic edge runtime components.

pub mod protocol;
pub mod runtime;
pub mod storage;

pub use protocol::{ProtocolAdapter, SimulatedProtocolAdapter};
pub use runtime::{CollectionReport, EdgeRuntime};
pub use storage::{JsonlLocalStore, LocalStore};
