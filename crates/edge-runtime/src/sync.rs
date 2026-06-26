use anyhow::{bail, Result};
use async_trait::async_trait;
use edge_core::EdgeConfigPackage;

use crate::{AppliedEdgeConfig, ConfiguredSimulatedRuntime};

#[derive(Clone, Debug, PartialEq)]
pub struct EdgeDesiredConfig {
    pub desired_version: String,
    pub package: EdgeConfigPackage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeConfigSyncReport {
    pub applied_version: String,
    pub samples_collected: usize,
}

#[async_trait]
pub trait EdgeConfigSyncClient {
    async fn fetch_desired_config(&mut self, edge_id: &str) -> Result<EdgeDesiredConfig>;

    async fn report_applied_version(&mut self, edge_id: &str, version: &str) -> Result<()>;
}

pub async fn sync_once<C>(edge_id: &str, client: &mut C) -> Result<EdgeConfigSyncReport>
where
    C: EdgeConfigSyncClient + Send,
{
    let desired = client.fetch_desired_config(edge_id).await?;
    if desired.package.edge_id != edge_id {
        bail!(
            "desired package targets edge {}, but runtime is {}",
            desired.package.edge_id,
            edge_id
        );
    }
    if desired.package.version != desired.desired_version {
        bail!(
            "desired version {} does not match package version {}",
            desired.desired_version,
            desired.package.version
        );
    }

    let applied = AppliedEdgeConfig::apply(desired.package)?;
    let mut runtime = ConfiguredSimulatedRuntime::new(applied);
    let collection = runtime.collect_once().await?;
    let applied_version = runtime.reported_version().to_string();

    client
        .report_applied_version(edge_id, &applied_version)
        .await?;

    Ok(EdgeConfigSyncReport {
        applied_version,
        samples_collected: collection.samples_collected,
    })
}
