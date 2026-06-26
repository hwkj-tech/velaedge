use anyhow::Result;
use async_trait::async_trait;
use edge_core::TelemetrySample;

#[async_trait]
pub trait ProtocolAdapter: Send {
    async fn read_telemetry(&mut self) -> Result<Vec<TelemetrySample>>;
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
