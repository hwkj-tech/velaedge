//! Shared domain contracts for the edge-cloud platform.

pub mod config;
pub mod edgelink;
pub mod message;
pub mod model;
pub mod observability;
pub mod policy;
pub mod protocol_catalog;
pub mod shadow;

pub use config::{
    bacnet_object_templates, bacnet_property_templates, decode_custom_serial_hex,
    dlt645_data_identifier_templates, dlt645_template_by_identifier, parse_bacnet_ip_endpoint,
    parse_bacnet_point_address, parse_dlt645_point_address, parse_iec101_point_address,
    parse_iec104_point_address, parse_omron_fins_endpoint, parse_omron_fins_point_address,
    parse_opc_ua_browse_path, parse_siemens_s7_endpoint, parse_siemens_s7_point_address,
    validate_bacnet_point, validate_command_flow, validate_custom_serial_point_spec,
    validate_data_config_visual_graph, validate_iec101_point, validate_iec104_endpoint,
    validate_iec104_point, validate_modbus_point_options, validate_omron_fins_point,
    validate_opc_ua_node_id, validate_opc_ua_point, validate_point_access,
    validate_siemens_s7_point, BacnetCovSettings, BacnetForeignDeviceSettings,
    BacnetIpConnectionSettings, BacnetObjectTemplate, BacnetPointAddress, BacnetPointOptions,
    BacnetPropertyTemplate, CollectionTask, CommandFlowConfig, CommandGraphEdge, CommandGraphNode,
    CommandGraphNodeKind, CustomSerialChecksum, CustomSerialFrameEncoding, CustomSerialPointSpec,
    CustomSerialValueEncoding, DataConfig, DataConfigCollection, DataConfigGraphEdge,
    DataConfigGraphNode, DataConfigGraphNodeKind, DataConfigPayload, DataConfigPayloadMode,
    DataConfigPoint, DataConfigPublish, DataConfigVisualGraph, DeviceInstance, DiscoveredPoint,
    DiscoveryAddressKind, DiscoveryReport, DiscoveryRequest, Dlt645DataIdentifierTemplate,
    Dlt645PointAddress, EdgeConfigPackage, Iec101ConnectionSettings, Iec101ControlType,
    Iec101PointOptions, Iec104ConnectionSettings, Iec104ControlType, Iec104PointOptions,
    ModbusByteOrder, ModbusPointOptions, ModbusRegisterEncoding, ModbusWordOrder,
    MqttLastWillConfig, MqttProtocolVersion, MqttUplinkConfig, MqttUserProperty, OmronFinsArea,
    OmronFinsConnectionSettings, OmronFinsPointAddress, OmronFinsTransport, OmronFinsWordOrder,
    OpcUaAuthMode, OpcUaBrowsePathAddress, OpcUaBrowsePathElement, OpcUaConnectionSettings,
    OpcUaMessageSecurityMode, OpcUaPointOptions, OpcUaSecurityPolicy, OpcUaWriteDataType,
    PointAccess, PointAddress, PointMappingSuggestion, ProtocolCircuitBreakerConfig,
    ProtocolConnection, ProtocolType, SerialConnectionSettings, SiemensS7Area,
    SiemensS7ConnectionSettings, SiemensS7DataType, SiemensS7PointAddress, TelemetryPointMapping,
    MAX_DATA_CONFIG_RETRY_COUNT, MAX_DATA_CONFIG_TIMEOUT_MS, MAX_DISCOVERY_POINTS,
    MAX_PROTOCOL_CIRCUIT_FAILURE_THRESHOLD, MAX_PROTOCOL_CIRCUIT_HALF_OPEN_SUCCESSES,
    MAX_PROTOCOL_CIRCUIT_OPEN_DURATION_MS, MIN_PROTOCOL_CIRCUIT_OPEN_DURATION_MS,
};
pub use edgelink::{
    decode_edgelink_frame, encode_edgelink_frame, EdgeLinkAck, EdgeLinkCommandResult,
    EdgeLinkConfigReport, EdgeLinkFrameError, EdgeLinkHeartbeat, EdgeLinkHello, EdgeLinkMessage,
    EdgeLinkMessageKind, EdgeLinkPayload, EDGELINK_MAX_FRAME_BYTES, EDGELINK_SCHEMA_VERSION,
};
pub use message::CloudEnvelope;
pub use model::{
    AlgorithmDsl, AlgorithmEventOutput, AlgorithmInputBinding, AlgorithmKind, AlgorithmOutput,
    AlgorithmReportMode, AlgorithmReportPolicy, AlgorithmRuntime, AlgorithmSpec, AlgorithmStep,
    AlgorithmTrigger, CommandCandidate, CommandParameter, CommandRisk, CommandSpec,
    CompareOperator, DataQuality, DataQualityCode, DeviceSpec, EventSeverity, EventSpec,
    NumberRange, TelemetryPoint, TelemetrySample, TelemetryType, TelemetryValue,
    WindowAggregateFunction,
};
pub use observability::{
    AlgorithmRuntimeMetrics, CloudSyncMetrics, CollectionRuntimeMetrics, EdgeHealth,
    EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot, LocalStoreMetrics, MqttRuntimeMetrics,
    MqttSinkRuntimeMetrics, ProtocolCircuitState, ProtocolRuntimeMetrics, RuntimeEventCategory,
    RuntimeEventSeverity, SystemRuntimeMetrics,
};
pub use policy::{PolicyEngine, PolicyViolation};
pub use protocol_catalog::{
    RuntimeProtocolCatalog, RuntimeProtocolDescriptor, RuntimeProtocolMaturity,
    RuntimeProtocolTransport,
};
pub use shadow::DeviceShadow;
