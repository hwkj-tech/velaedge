//! Deterministic edge runtime components.

pub mod config;
pub mod edgelink_client;
pub mod metrics;
pub mod protocol;
pub mod reporting;
pub mod runtime;
pub mod storage;
pub mod sync;

pub use config::{AppliedEdgeConfig, ConfiguredSimulatedRuntime};
pub use edgelink_client::{
    connect_edgelink_once, connect_edgelink_tls_once, EdgeLinkClientTlsConfig,
    EdgeLinkConnectReport,
};
pub use metrics::SimulatedRuntimeMetricsCollector;
pub use protocol::{ProtocolAdapter, SimulatedProtocolAdapter};
pub use reporting::{report_runtime_status_once, HttpRuntimeStatusReporter, RuntimeStatusReporter};
pub use runtime::{CollectionReport, EdgeRuntime};
pub use storage::{JsonlLocalStore, LocalStore};
pub use sync::{
    sync_and_report_once, sync_once, EdgeConfigSyncClient, EdgeConfigSyncReport, EdgeDesiredConfig,
    HttpEdgeConfigSyncClient,
};
