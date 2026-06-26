//! Shared domain contracts for the edge-cloud platform.

pub mod config;
pub mod message;
pub mod model;
pub mod policy;
pub mod shadow;

pub use config::{
    CollectionTask, DeviceInstance, EdgeConfigPackage, PointAddress, ProtocolConnection,
    ProtocolType, TelemetryPointMapping,
};
pub use message::CloudEnvelope;
pub use model::{
    AlgorithmRuntime, AlgorithmSpec, CommandCandidate, CommandParameter, CommandRisk, CommandSpec,
    DataQuality, DeviceSpec, EventSpec, NumberRange, TelemetryPoint, TelemetrySample,
    TelemetryType, TelemetryValue,
};
pub use policy::{PolicyEngine, PolicyViolation};
pub use shadow::DeviceShadow;
