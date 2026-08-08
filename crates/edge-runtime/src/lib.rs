//! Deterministic edge runtime components.

pub mod algorithm;
pub mod bacnet_ip;
pub mod capability;
mod circuit_breaker;
pub mod command;
pub mod command_service;
pub mod config;
pub mod configured_runtime;
pub mod custom_serial;
pub mod discovery;
pub mod dlt645;
pub mod edgelink_client;
pub mod field_campaign_artifact;
pub mod field_campaign_plan;
pub mod field_campaign_site;
pub mod field_endurance;
pub mod iec101;
pub mod iec104;
pub mod interoperability_acceptance;
pub mod local_db;
pub mod metrics;
mod modbus_batch;
mod modbus_codec;
pub mod modbus_rtu;
pub mod modbus_tcp;
pub mod modbus_tcp_endurance;
pub mod mqtt_acceptance;
pub mod mqtt_field_receipt;
pub mod mqtt_uplink;
pub mod omron_fins;
pub mod opcua;
pub mod protocol;
pub mod protocol_catalog;
pub mod reporting;
pub mod runtime;
pub mod runtime_health;
pub mod scheduler;
pub mod serial;
pub mod siemens_s7;
pub mod storage;
pub mod sync;

pub use algorithm::{AlgorithmEngine, AlgorithmExecutionReport};
pub use bacnet_ip::{BacnetCovRuntimeMetrics, BacnetIpAdapter};
pub use capability::RuntimeCapabilityConfig;
pub use circuit_breaker::ProtocolCircuitBreakerRegistry;
pub use command::{
    build_command_reply_messages, command_values_match, plan_command_execution,
    CommandExecutionPlan, CommandExecutionReport, CommandExecutionStatus, CommandWriteRecord,
    CommandWriteVerification, PlannedCommandSafetyGate, PlannedPointWrite,
};
pub use command_service::CommandRuntimeService;
pub use config::{AppliedEdgeConfig, ConfiguredMqttCollectionReport, ConfiguredSimulatedRuntime};
pub use configured_runtime::{
    ConfiguredEdgeRuntime, ResilientScheduledCollectionReport,
    ResilientScheduledDataConfigPublishReport, ScheduledCollectionFailure,
    ScheduledCollectionReport, ScheduledDataConfigFailure, ScheduledDataConfigPublishReport,
};
pub use custom_serial::{
    append_custom_serial_checksum, decode_custom_serial_frame, encode_custom_serial_frame,
    CustomSerialAdapter,
};
pub use discovery::{
    run_modbus_discovery_request, run_protocol_discovery_request, ModbusRtuDiscovery,
    SimulatedSerialDiscovery,
};
pub use dlt645::{append_dlt645_checksum, Dlt645Adapter, Dlt645ReadFailure};
pub use edgelink_client::{
    connect_edgelink_once, connect_edgelink_once_with_capabilities, connect_edgelink_tls_once,
    handle_edgelink_discovery_requests_with_factory, publish_edgelink_runtime_daemon_session,
    publish_edgelink_runtime_daemon_session_with_persistent_mqtt,
    publish_edgelink_runtime_status_authenticated_once, publish_edgelink_runtime_status_once,
    publish_edgelink_runtime_status_tls_once,
    publish_edgelink_runtime_status_with_mqtt_publisher_once,
    publish_edgelink_runtime_status_with_mqtt_uplink_once,
    publish_edgelink_runtime_status_with_store_and_capabilities_once,
    publish_edgelink_runtime_status_with_store_once, EdgeLinkClientTlsConfig,
    EdgeLinkConnectReport, EdgeLinkPublishReport, PersistentCollectionRuntime,
};
pub use field_campaign_artifact::{
    read_field_campaign_evidence, read_field_campaign_manifest,
    read_field_interoperability_artifacts, FieldCampaignArtifact, FieldCampaignManifest,
};
pub use field_campaign_plan::{
    evaluate_field_campaign_plan, evaluate_field_campaign_plan_for_site_status,
    FieldCampaignDeploymentPlan, FieldCampaignPlanDevice, FieldCampaignPlanEntry,
    FieldCampaignPlanEntryReport, FieldCampaignPlanMqttRoute, FieldCampaignPlanReport,
    FieldCampaignPlanStatus, FieldCampaignPlanSummary, FieldCampaignProtocolCoverage,
};
pub use field_campaign_site::{
    evaluate_field_campaign_site_status, FieldCampaignExecutionReport,
    FieldCampaignExecutionStatus, FieldCampaignSiteReport, FieldCampaignSiteStatus,
    FieldCampaignSiteSummary,
};
pub use field_endurance::{
    run_field_endurance_acceptance, validate_field_endurance_options, FieldAcceptanceCriteria,
    FieldCycleEvidence, FieldDeviceIdentity, FieldEnduranceOptions, FieldEnduranceReport,
    FieldEnduranceStatus, FieldLatencyEvidence, FieldMqttEvidence, FieldMqttSinkAcceptanceEvidence,
    FieldPointEvidence, FieldProtocolAcceptanceEvidence,
};
pub use iec101::{append_iec101_checksum, Iec101Adapter};
pub use iec104::Iec104Adapter;
pub use interoperability_acceptance::{
    evaluate_field_interoperability, field_protocol_name, validate_field_interoperability_policy,
    AcceptedInteroperabilityRun, BrokerConsumerReceipt, BrokerConsumerRouteReceipt,
    FieldInteroperabilityEvidence, FieldInteroperabilityPolicy, FieldInteroperabilityPolicyReport,
    FieldInteroperabilityReport, FieldInteroperabilityStatus, FieldInteroperabilitySummary,
    NativeBrokerAudit, ProtocolInteroperabilityEvidence, RejectedInteroperabilityEvidence,
};
pub use local_db::{
    CommandAuditRecord, CommandAuditState, CommandClaim, MqttOutboxEntry, MqttOutboxStats,
    MqttPublishAcknowledgement, RocksEdgeRuntimeStore,
};
pub use metrics::{CollectionRunStats, HostSystemMetricsSampler, RuntimeMetricsCollector};
pub use modbus_rtu::{append_modbus_rtu_crc, ModbusRtuAdapter};
pub use modbus_tcp::{
    DynamicFloatPoint, ModbusTcpAdapter, ModbusTcpSimulator, ModbusTcpSimulatorMetrics,
    ModbusTcpSimulatorOptions,
};
pub use modbus_tcp_endurance::{
    run_modbus_tcp_endurance_acceptance, ModbusTcpAcceptanceCriteria, ModbusTcpCycleEvidence,
    ModbusTcpEnduranceOptions, ModbusTcpEnduranceReport, ModbusTcpEnduranceStatus,
    ModbusTcpLatencyEvidence, ModbusTcpMqttEvidence, ModbusTcpPointEvidence,
    ModbusTcpProtocolEvidence,
};
pub use mqtt_acceptance::{run_mqtt_acceptance, MqttAcceptanceOptions, MqttAcceptanceReport};
pub use mqtt_field_receipt::{
    capture_mqtt_field_receipt, start_mqtt_field_receipt_session, MqttFieldReceiptOptions,
    MqttFieldReceiptSession,
};
pub use mqtt_uplink::{
    build_data_config_mqtt_publish_messages, build_mqtt_publish_messages,
    configured_data_mqtt_output_routes, flush_mqtt_outbox, mqtt_topic_matches,
    parse_mqtt_broker_target, publish_data_config_mqtt_samples,
    publish_data_config_mqtt_samples_with_outbox, publish_mqtt_samples,
    publish_mqtt_samples_with_outbox, validate_mqtt_uplink_config,
    validate_mqtt_uplink_runtime_environment, ConfiguredMqttOutputRoute, MqttBrokerTarget,
    MqttCommandMessage, MqttCommandSubscriber, MqttPublishMessage, MqttPublisher,
    MqttSinkRuntimeStatus, MultiBrokerMqttPublisher, PersistentMqttPublisher, PersistentMqttStatus,
    RecordingMqttPublisher, RumqttcMqttPublisher,
};
pub use omron_fins::OmronFinsAdapter;
pub use opcua::OpcUaAdapter;
pub use protocol::{
    ProtocolAdapter, ProtocolCommandAdapter, ProtocolPointWrite, ProtocolWriteResult,
    SimulatedProtocolAdapter,
};
pub use protocol_catalog::{
    RuntimeProtocolCatalog, RuntimeProtocolDescriptor, RuntimeProtocolMaturity,
    RuntimeProtocolTransport,
};
pub use reporting::{
    report_runtime_status_once, report_runtime_status_with_store_once, HttpRuntimeStatusReporter,
    RuntimeStatusReporter,
};
pub use runtime::{CollectionReport, EdgeRuntime};
pub use runtime_health::{
    serve_runtime_health, RuntimeCollectionActivity, RuntimeHealthDocument, RuntimeHealthState,
    RuntimeTrendSample, RUNTIME_HEALTH_PAGE,
};
pub use scheduler::{CollectionSchedule, DataConfigSchedule};
pub use serial::{
    require_serial_endpoint, ScriptedSerialBus, ScriptedSerialBusFactory, SerialBus,
    SerialBusFactory, TokioSerialBus, TokioSerialBusFactory,
};
pub use siemens_s7::SiemensS7Adapter;
pub use storage::{JsonlLocalStore, LocalStore};
pub use sync::{
    sync_and_report_mqtt_uplink_once, sync_and_report_mqtt_uplink_with_store_once,
    sync_and_report_once, sync_and_report_with_mqtt_publisher_and_store_once,
    sync_and_report_with_mqtt_publisher_once, sync_once, EdgeConfigMqttSyncReport,
    EdgeConfigSyncClient, EdgeConfigSyncReport, EdgeDesiredConfig, HttpEdgeConfigSyncClient,
};
