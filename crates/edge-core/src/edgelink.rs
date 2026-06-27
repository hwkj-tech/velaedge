use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    CommandCandidate, EdgeConfigPackage, EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot,
    TelemetrySample,
};

pub const EDGELINK_SCHEMA_VERSION: &str = "1.0";
pub const EDGELINK_MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
const EDGELINK_FRAME_HEADER_BYTES: usize = 4;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EdgeLinkMessage {
    pub message_id: Uuid,
    pub edge_id: String,
    pub runtime_id: Option<String>,
    pub schema_version: String,
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub kind: EdgeLinkMessageKind,
    pub payload: EdgeLinkPayload,
}

impl EdgeLinkMessage {
    pub fn hello(
        edge_id: impl Into<String>,
        runtime_id: impl Into<String>,
        runtime_version: impl Into<String>,
        applied_config_version: Option<String>,
        capabilities: Vec<String>,
    ) -> Self {
        let edge_id = edge_id.into();
        let runtime_id = runtime_id.into();
        Self::new(
            edge_id,
            Some(runtime_id.clone()),
            1,
            EdgeLinkPayload::Hello(EdgeLinkHello {
                runtime_id,
                runtime_version: runtime_version.into(),
                applied_config_version,
                capabilities,
            }),
        )
    }

    pub fn ack(
        edge_id: impl Into<String>,
        runtime_id: impl Into<String>,
        ack_message_id: Uuid,
        ack_sequence: u64,
    ) -> Self {
        Self::new(
            edge_id,
            Some(runtime_id.into()),
            ack_sequence,
            EdgeLinkPayload::Ack(EdgeLinkAck {
                ack_message_id,
                ack_sequence,
                accepted: true,
                reason: None,
            }),
        )
    }

    pub fn nack(
        edge_id: impl Into<String>,
        runtime_id: Option<impl Into<String>>,
        ack_message_id: Uuid,
        ack_sequence: u64,
        reason: impl Into<String>,
    ) -> Self {
        Self::new(
            edge_id,
            runtime_id.map(Into::into),
            ack_sequence,
            EdgeLinkPayload::Nack(EdgeLinkAck {
                ack_message_id,
                ack_sequence,
                accepted: false,
                reason: Some(reason.into()),
            }),
        )
    }

    pub fn heartbeat(
        edge_id: impl Into<String>,
        runtime_id: impl Into<String>,
        sequence: u64,
        heartbeat: EdgeLinkHeartbeat,
    ) -> Self {
        Self::new(
            edge_id,
            Some(runtime_id.into()),
            sequence,
            EdgeLinkPayload::Heartbeat(heartbeat),
        )
    }

    pub fn runtime_metrics(
        edge_id: impl Into<String>,
        runtime_id: impl Into<String>,
        sequence: u64,
        snapshot: EdgeRuntimeMetricsSnapshot,
    ) -> Self {
        Self::new(
            edge_id,
            Some(runtime_id.into()),
            sequence,
            EdgeLinkPayload::RuntimeMetrics(snapshot),
        )
    }

    pub fn config_deploy(
        edge_id: impl Into<String>,
        runtime_id: impl Into<String>,
        sequence: u64,
        package: EdgeConfigPackage,
    ) -> Self {
        Self::new(
            edge_id,
            Some(runtime_id.into()),
            sequence,
            EdgeLinkPayload::ConfigDeploy(package),
        )
    }

    pub fn config_report(
        edge_id: impl Into<String>,
        runtime_id: impl Into<String>,
        sequence: u64,
        desired_version: impl Into<String>,
        applied_version: Option<String>,
        accepted: bool,
        reason: Option<String>,
    ) -> Self {
        Self::new(
            edge_id,
            Some(runtime_id.into()),
            sequence,
            EdgeLinkPayload::ConfigReport(EdgeLinkConfigReport {
                desired_version: desired_version.into(),
                applied_version,
                accepted,
                reason,
            }),
        )
    }

    pub fn runtime_event(
        edge_id: impl Into<String>,
        runtime_id: impl Into<String>,
        sequence: u64,
        event: EdgeRuntimeEvent,
    ) -> Self {
        Self::new(
            edge_id,
            Some(runtime_id.into()),
            sequence,
            EdgeLinkPayload::RuntimeEvent(event),
        )
    }

    pub fn kind(&self) -> EdgeLinkMessageKind {
        self.payload.kind()
    }

    fn new(
        edge_id: impl Into<String>,
        runtime_id: Option<String>,
        sequence: u64,
        payload: EdgeLinkPayload,
    ) -> Self {
        Self {
            message_id: Uuid::new_v4(),
            edge_id: edge_id.into(),
            runtime_id,
            schema_version: EDGELINK_SCHEMA_VERSION.to_string(),
            sequence,
            timestamp: Utc::now(),
            kind: payload.kind(),
            payload,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeLinkMessageKind {
    Hello,
    Heartbeat,
    Ack,
    Nack,
    ConfigDeploy,
    ConfigReport,
    CommandRequest,
    CommandResult,
    TelemetryBatch,
    RuntimeMetrics,
    RuntimeEvent,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum EdgeLinkPayload {
    Hello(EdgeLinkHello),
    Heartbeat(EdgeLinkHeartbeat),
    Ack(EdgeLinkAck),
    Nack(EdgeLinkAck),
    ConfigDeploy(EdgeConfigPackage),
    ConfigReport(EdgeLinkConfigReport),
    CommandRequest(CommandCandidate),
    CommandResult(EdgeLinkCommandResult),
    TelemetryBatch(Vec<TelemetrySample>),
    RuntimeMetrics(EdgeRuntimeMetricsSnapshot),
    RuntimeEvent(EdgeRuntimeEvent),
}

impl EdgeLinkPayload {
    pub fn kind(&self) -> EdgeLinkMessageKind {
        match self {
            Self::Hello(_) => EdgeLinkMessageKind::Hello,
            Self::Heartbeat(_) => EdgeLinkMessageKind::Heartbeat,
            Self::Ack(_) => EdgeLinkMessageKind::Ack,
            Self::Nack(_) => EdgeLinkMessageKind::Nack,
            Self::ConfigDeploy(_) => EdgeLinkMessageKind::ConfigDeploy,
            Self::ConfigReport(_) => EdgeLinkMessageKind::ConfigReport,
            Self::CommandRequest(_) => EdgeLinkMessageKind::CommandRequest,
            Self::CommandResult(_) => EdgeLinkMessageKind::CommandResult,
            Self::TelemetryBatch(_) => EdgeLinkMessageKind::TelemetryBatch,
            Self::RuntimeMetrics(_) => EdgeLinkMessageKind::RuntimeMetrics,
            Self::RuntimeEvent(_) => EdgeLinkMessageKind::RuntimeEvent,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgeLinkHello {
    pub runtime_id: String,
    pub runtime_version: String,
    pub applied_config_version: Option<String>,
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgeLinkHeartbeat {
    pub uptime_seconds: u64,
    pub pending_uploads: u64,
    pub active_config_version: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgeLinkAck {
    pub ack_message_id: Uuid,
    pub ack_sequence: u64,
    pub accepted: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgeLinkConfigReport {
    pub desired_version: String,
    pub applied_version: Option<String>,
    pub accepted: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EdgeLinkCommandResult {
    pub command_id: String,
    pub accepted: bool,
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum EdgeLinkFrameError {
    #[error("incomplete EdgeLink frame header: expected 4 bytes, got {actual}")]
    IncompleteHeader { actual: usize },
    #[error("incomplete EdgeLink frame: expected {expected} payload bytes, got {actual}")]
    IncompleteFrame { expected: usize, actual: usize },
    #[error("EdgeLink frame too large: {size} bytes exceeds {max} bytes")]
    FrameTooLarge { size: usize, max: usize },
    #[error("invalid EdgeLink frame JSON: {0}")]
    InvalidJson(serde_json::Error),
}

pub fn encode_edgelink_frame(message: &EdgeLinkMessage) -> Result<Vec<u8>, EdgeLinkFrameError> {
    let payload = serde_json::to_vec(message).map_err(EdgeLinkFrameError::InvalidJson)?;
    if payload.len() > EDGELINK_MAX_FRAME_BYTES {
        return Err(EdgeLinkFrameError::FrameTooLarge {
            size: payload.len(),
            max: EDGELINK_MAX_FRAME_BYTES,
        });
    }

    let mut frame = Vec::with_capacity(EDGELINK_FRAME_HEADER_BYTES + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub fn decode_edgelink_frame(frame: &[u8]) -> Result<EdgeLinkMessage, EdgeLinkFrameError> {
    if frame.len() < EDGELINK_FRAME_HEADER_BYTES {
        return Err(EdgeLinkFrameError::IncompleteHeader {
            actual: frame.len(),
        });
    }

    let payload_len = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
    if payload_len > EDGELINK_MAX_FRAME_BYTES {
        return Err(EdgeLinkFrameError::FrameTooLarge {
            size: payload_len,
            max: EDGELINK_MAX_FRAME_BYTES,
        });
    }

    let actual = frame.len() - EDGELINK_FRAME_HEADER_BYTES;
    if actual < payload_len {
        return Err(EdgeLinkFrameError::IncompleteFrame {
            expected: payload_len,
            actual,
        });
    }

    serde_json::from_slice(
        &frame[EDGELINK_FRAME_HEADER_BYTES..EDGELINK_FRAME_HEADER_BYTES + payload_len],
    )
    .map_err(EdgeLinkFrameError::InvalidJson)
}
