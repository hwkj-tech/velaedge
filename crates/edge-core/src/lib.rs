//! Shared domain contracts for the edge-cloud platform.

pub mod config;
pub mod edgelink;
pub mod message;
pub mod model;
pub mod observability;
pub mod policy;
pub mod shadow;

pub use config::{
    CollectionTask, DataConfig, DataConfigCollection, DataConfigGraphEdge, DataConfigGraphNode,
    DataConfigGraphNodeKind, DataConfigPayload, DataConfigPayloadMode, DataConfigPoint,
    DataConfigPublish, DataConfigVisualGraph, DeviceInstance, DiscoveredPoint, DiscoveryReport,
    EdgeConfigPackage, MqttUplinkConfig, PointAddress, PointMappingSuggestion, ProtocolConnection,
    ProtocolType, SerialConnectionSettings, TelemetryPointMapping,
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
    CompareOperator, DataQuality, DeviceSpec, EventSeverity, EventSpec, NumberRange,
    TelemetryPoint, TelemetrySample, TelemetryType, TelemetryValue, WindowAggregateFunction,
};
pub use observability::{
    AlgorithmRuntimeMetrics, CloudSyncMetrics, CollectionRuntimeMetrics, EdgeHealth,
    EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot, LocalStoreMetrics, ProtocolRuntimeMetrics,
    RuntimeEventCategory, RuntimeEventSeverity, SystemRuntimeMetrics,
};
pub use policy::{PolicyEngine, PolicyViolation};
pub use shadow::DeviceShadow;
