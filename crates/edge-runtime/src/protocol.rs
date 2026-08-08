use anyhow::Result;
use async_trait::async_trait;
use edge_core::{TelemetryPointMapping, TelemetrySample, TelemetryValue};

#[async_trait]
pub trait ProtocolAdapter: Send {
    async fn read_telemetry(&mut self) -> Result<Vec<TelemetrySample>>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProtocolWriteResult {
    pub point_id: String,
    pub value: TelemetryValue,
    pub verified: bool,
    pub readback_value: Option<TelemetryValue>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProtocolPointWrite {
    pub mapping: TelemetryPointMapping,
    pub value: TelemetryValue,
}

impl ProtocolPointWrite {
    pub fn new(mapping: TelemetryPointMapping, value: TelemetryValue) -> Self {
        Self { mapping, value }
    }
}

#[async_trait]
pub trait ProtocolCommandAdapter: Send {
    async fn write_point(
        &mut self,
        mapping: &TelemetryPointMapping,
        value: TelemetryValue,
    ) -> Result<ProtocolWriteResult>;

    async fn write_points(
        &mut self,
        writes: &[ProtocolPointWrite],
    ) -> Result<Vec<ProtocolWriteResult>> {
        let mut results = Vec::with_capacity(writes.len());
        for write in writes {
            results.push(
                self.write_point(&write.mapping, write.value.clone())
                    .await?,
            );
        }
        Ok(results)
    }
}

#[derive(Clone, Debug)]
pub struct SimulatedProtocolAdapter {
    samples: Vec<TelemetrySample>,
}

impl SimulatedProtocolAdapter {
    pub fn new(samples: Vec<TelemetrySample>) -> Self {
        Self { samples }
    }
}

#[async_trait]
impl ProtocolAdapter for SimulatedProtocolAdapter {
    async fn read_telemetry(&mut self) -> Result<Vec<TelemetrySample>> {
        Ok(self.samples.clone())
    }
}
