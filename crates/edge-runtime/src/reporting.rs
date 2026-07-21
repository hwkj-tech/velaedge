use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use edge_core::{EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot};
use reqwest::Url;

use crate::{AppliedEdgeConfig, RocksEdgeRuntimeStore, SimulatedRuntimeMetricsCollector};

#[async_trait]
pub trait RuntimeStatusReporter {
    async fn report_metrics(&mut self, snapshot: EdgeRuntimeMetricsSnapshot) -> Result<()>;

    async fn report_event(&mut self, event: EdgeRuntimeEvent) -> Result<()>;
}

pub async fn report_runtime_status_once<R>(
    runtime_id: &str,
    applied: AppliedEdgeConfig,
    reporter: &mut R,
) -> Result<EdgeRuntimeMetricsSnapshot>
where
    R: RuntimeStatusReporter + Send,
{
    let snapshot = SimulatedRuntimeMetricsCollector::new(runtime_id, applied).snapshot();
    reporter.report_metrics(snapshot.clone()).await?;
    Ok(snapshot)
}

pub async fn report_runtime_status_with_store_once<R>(
    runtime_id: &str,
    applied: AppliedEdgeConfig,
    store: &RocksEdgeRuntimeStore,
    reporter: &mut R,
) -> Result<EdgeRuntimeMetricsSnapshot>
where
    R: RuntimeStatusReporter + Send,
{
    let snapshot = SimulatedRuntimeMetricsCollector::new(runtime_id, applied)
        .with_mqtt_outbox_stats(store.mqtt_outbox_stats()?)
        .snapshot();
    reporter.report_metrics(snapshot.clone()).await?;
    Ok(snapshot)
}

#[derive(Clone)]
pub struct HttpRuntimeStatusReporter {
    base_url: Url,
    client: reqwest::Client,
}

impl HttpRuntimeStatusReporter {
    pub fn new(base_url: &str) -> Result<Self> {
        Ok(Self {
            base_url: Url::parse(base_url).context("cloud API base URL is invalid")?,
            client: reqwest::Client::new(),
        })
    }

    fn endpoint(&self, edge_id: &str, leaf: &str) -> Result<Url> {
        let mut url = self.base_url.clone();
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| anyhow!("cloud API base URL cannot be a base"))?;
            segments.pop_if_empty();
            segments.extend(["api", "edges", edge_id, leaf]);
        }
        Ok(url)
    }
}

#[async_trait]
impl RuntimeStatusReporter for HttpRuntimeStatusReporter {
    async fn report_metrics(&mut self, snapshot: EdgeRuntimeMetricsSnapshot) -> Result<()> {
        let endpoint = self.endpoint(&snapshot.edge_id, "runtime-metrics")?;
        self.client
            .post(endpoint)
            .json(&snapshot)
            .send()
            .await
            .context("failed to send runtime metrics")?
            .error_for_status()
            .context("cloud API rejected runtime metrics")?;
        Ok(())
    }

    async fn report_event(&mut self, event: EdgeRuntimeEvent) -> Result<()> {
        let endpoint = self.endpoint(&event.edge_id, "runtime-events")?;
        self.client
            .post(endpoint)
            .json(&event)
            .send()
            .await
            .context("failed to send runtime event")?
            .error_for_status()
            .context("cloud API rejected runtime event")?;
        Ok(())
    }
}
