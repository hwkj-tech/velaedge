//! Deterministic edge runtime components.

pub mod algorithm;
pub mod capability;
pub mod config;
pub mod configured_runtime;
pub mod custom_serial;
pub mod discovery;
pub mod dlt645;
pub mod edgelink_client;
pub mod iec101;
pub mod local_db;
pub mod metrics;
pub mod modbus_rtu;
pub mod modbus_tcp;
pub mod mqtt_acceptance;
pub mod mqtt_uplink;
pub mod protocol;
pub mod reporting;
pub mod runtime;
pub mod scheduler;
pub mod serial;
pub mod storage;
pub mod sync;

pub use algorithm::{AlgorithmEngine, AlgorithmExecutionReport};
pub use capability::RuntimeCapabilityConfig;
pub use config::{AppliedEdgeConfig, ConfiguredMqttCollectionReport, ConfiguredSimulatedRuntime};
pub use configured_runtime::{
    ConfiguredEdgeRuntime, ResilientScheduledCollectionReport,
    ResilientScheduledDataConfigPublishReport, ScheduledCollectionFailure,
    ScheduledCollectionReport, ScheduledDataConfigFailure, ScheduledDataConfigPublishReport,
};
pub use custom_serial::{append_custom_serial_checksum, CustomSerialAdapter};
pub use discovery::{run_modbus_discovery_request, ModbusRtuDiscovery, SimulatedSerialDiscovery};
pub use dlt645::{append_dlt645_checksum, Dlt645Adapter};
pub use edgelink_client::{
    connect_edgelink_once, connect_edgelink_once_with_capabilities, connect_edgelink_tls_once,
    handle_edgelink_discovery_requests_with_factory, publish_edgelink_runtime_daemon_session,
    publish_edgelink_runtime_status_authenticated_once, publish_edgelink_runtime_status_once,
    publish_edgelink_runtime_status_tls_once,
    publish_edgelink_runtime_status_with_mqtt_publisher_once,
    publish_edgelink_runtime_status_with_mqtt_uplink_once,
    publish_edgelink_runtime_status_with_store_and_capabilities_once,
    publish_edgelink_runtime_status_with_store_once, EdgeLinkClientTlsConfig,
    EdgeLinkConnectReport, EdgeLinkPublishReport,
};
pub use iec101::{append_iec101_checksum, Iec101Adapter};
pub use local_db::{
    MqttOutboxEntry, MqttOutboxStats, MqttPublishAcknowledgement, RocksEdgeRuntimeStore,
};
pub use metrics::{CollectionRunStats, HostSystemMetricsSampler, RuntimeMetricsCollector};
pub use modbus_rtu::{append_modbus_rtu_crc, ModbusRtuAdapter};
pub use modbus_tcp::{
    DynamicFloatPoint, ModbusTcpAdapter, ModbusTcpSimulator, ModbusTcpSimulatorOptions,
};
pub use mqtt_acceptance::{run_mqtt_acceptance, MqttAcceptanceOptions, MqttAcceptanceReport};
pub use mqtt_uplink::{
    build_data_config_mqtt_publish_messages, build_mqtt_publish_messages, flush_mqtt_outbox,
    parse_mqtt_broker_target, publish_data_config_mqtt_samples,
    publish_data_config_mqtt_samples_with_outbox, publish_mqtt_samples,
    publish_mqtt_samples_with_outbox, MqttBrokerTarget, MqttPublishMessage, MqttPublisher,
    MultiBrokerMqttPublisher, RecordingMqttPublisher, RumqttcMqttPublisher,
};
pub use protocol::{ProtocolAdapter, SimulatedProtocolAdapter};
pub use reporting::{
    report_runtime_status_once, report_runtime_status_with_store_once, HttpRuntimeStatusReporter,
    RuntimeStatusReporter,
};
pub use runtime::{CollectionReport, EdgeRuntime};
pub use scheduler::{CollectionSchedule, DataConfigSchedule};
pub use serial::{
    require_serial_endpoint, ScriptedSerialBus, ScriptedSerialBusFactory, SerialBus,
    SerialBusFactory, TokioSerialBus, TokioSerialBusFactory,
};
pub use storage::{JsonlLocalStore, LocalStore};
pub use sync::{
    sync_and_report_mqtt_uplink_once, sync_and_report_mqtt_uplink_with_store_once,
    sync_and_report_once, sync_and_report_with_mqtt_publisher_and_store_once,
    sync_and_report_with_mqtt_publisher_once, sync_once, EdgeConfigMqttSyncReport,
    EdgeConfigSyncClient, EdgeConfigSyncReport, EdgeDesiredConfig, HttpEdgeConfigSyncClient,
};
