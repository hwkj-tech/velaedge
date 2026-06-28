//! Deterministic edge runtime components.

pub mod capability;
pub mod config;
pub mod discovery;
pub mod edgelink_client;
pub mod local_db;
pub mod metrics;
pub mod mqtt_uplink;
pub mod protocol;
pub mod reporting;
pub mod runtime;
pub mod storage;
pub mod sync;

pub use capability::RuntimeCapabilityConfig;
pub use config::{AppliedEdgeConfig, ConfiguredMqttCollectionReport, ConfiguredSimulatedRuntime};
pub use discovery::SimulatedSerialDiscovery;
pub use edgelink_client::{
    connect_edgelink_once, connect_edgelink_once_with_capabilities, connect_edgelink_tls_once,
    publish_edgelink_runtime_status_once,
    publish_edgelink_runtime_status_with_store_and_capabilities_once,
    publish_edgelink_runtime_status_with_store_once, EdgeLinkClientTlsConfig,
    EdgeLinkConnectReport, EdgeLinkPublishReport,
};
pub use local_db::RocksEdgeRuntimeStore;
pub use metrics::SimulatedRuntimeMetricsCollector;
pub use mqtt_uplink::{
    build_mqtt_publish_messages, parse_mqtt_broker_target, publish_mqtt_samples, MqttBrokerTarget,
    MqttPublishMessage, MqttPublisher, RecordingMqttPublisher, RumqttcMqttPublisher,
};
pub use protocol::{ProtocolAdapter, SimulatedProtocolAdapter};
pub use reporting::{report_runtime_status_once, HttpRuntimeStatusReporter, RuntimeStatusReporter};
pub use runtime::{CollectionReport, EdgeRuntime};
pub use storage::{JsonlLocalStore, LocalStore};
pub use sync::{
    sync_and_report_mqtt_uplink_once, sync_and_report_once,
    sync_and_report_with_mqtt_publisher_once, sync_once, EdgeConfigMqttSyncReport,
    EdgeConfigSyncClient, EdgeConfigSyncReport, EdgeDesiredConfig, HttpEdgeConfigSyncClient,
};
