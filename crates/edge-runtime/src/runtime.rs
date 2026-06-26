use anyhow::Result;
use edge_core::DeviceShadow;

use crate::{LocalStore, ProtocolAdapter};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollectionReport {
    pub samples_collected: usize,
}

pub struct EdgeRuntime<A, S> {
    adapter: A,
    store: S,
    shadow: DeviceShadow,
}

impl<A, S> EdgeRuntime<A, S>
where
    A: ProtocolAdapter,
    S: LocalStore,
{
    pub fn new(
        edge_id: impl Into<String>,
        device_id: impl Into<String>,
        adapter: A,
        store: S,
    ) -> Self {
        let edge_id = edge_id.into();
        let device_id = device_id.into();

        Self {
            adapter,
            store,
            shadow: DeviceShadow::new(edge_id, device_id),
        }
    }

    pub async fn collect_once(&mut self) -> Result<CollectionReport> {
        let samples = self.adapter.read_telemetry().await?;
        let samples_collected = samples.len();

        for sample in samples {
            self.store.append_sample(&sample).await?;
            self.shadow.update(sample);
        }

        Ok(CollectionReport { samples_collected })
    }

    pub fn shadow(&self) -> &DeviceShadow {
        &self.shadow
    }
}
