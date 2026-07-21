use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use edge_core::EdgeConfigPackage;
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::{
    report_runtime_status_once, report_runtime_status_with_store_once, AppliedEdgeConfig,
    ConfiguredEdgeRuntime, ConfiguredSimulatedRuntime, MqttPublisher, MultiBrokerMqttPublisher,
    RocksEdgeRuntimeStore, RuntimeStatusReporter, TokioSerialBusFactory,
};

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeConfigMqttSyncReport {
    pub applied_version: String,
    pub samples_collected: usize,
    pub mqtt_messages_published: usize,
}

#[async_trait]
pub trait EdgeConfigSyncClient {
    async fn fetch_desired_config(&mut self, edge_id: &str) -> Result<EdgeDesiredConfig>;

    async fn report_applied_version(&mut self, edge_id: &str, version: &str) -> Result<()>;
}

#[derive(Clone)]
pub struct HttpEdgeConfigSyncClient {
    base_url: Url,
    client: reqwest::Client,
}

impl HttpEdgeConfigSyncClient {
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
impl EdgeConfigSyncClient for HttpEdgeConfigSyncClient {
    async fn fetch_desired_config(&mut self, edge_id: &str) -> Result<EdgeDesiredConfig> {
        let response = self
            .client
            .get(self.endpoint(edge_id, "desired-config")?)
            .send()
            .await
            .context("failed to fetch desired edge config")?
            .error_for_status()
            .context("cloud API rejected desired config request")?
            .json::<EdgeDesiredConfigResponse>()
            .await
            .context("failed to decode desired edge config")?;

        Ok(EdgeDesiredConfig {
            desired_version: response.desired_version,
            package: response.package,
        })
    }

    async fn report_applied_version(&mut self, edge_id: &str, version: &str) -> Result<()> {
        self.client
            .post(self.endpoint(edge_id, "reported-config")?)
            .json(&EdgeReportedConfigRequest {
                reported_version: version.to_string(),
            })
            .send()
            .await
            .context("failed to report applied edge config version")?
            .error_for_status()
            .context("cloud API rejected reported config version")?;

        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EdgeDesiredConfigResponse {
    desired_version: String,
    package: EdgeConfigPackage,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EdgeReportedConfigRequest {
    reported_version: String,
}

pub async fn sync_once<C>(edge_id: &str, client: &mut C) -> Result<EdgeConfigSyncReport>
where
    C: EdgeConfigSyncClient + Send,
{
    let (report, _) = sync_once_inner(edge_id, client).await?;
    Ok(report)
}

pub async fn sync_and_report_once<C, R>(
    edge_id: &str,
    runtime_id: &str,
    client: &mut C,
    reporter: &mut R,
) -> Result<EdgeConfigSyncReport>
where
    C: EdgeConfigSyncClient + Send,
    R: RuntimeStatusReporter + Send,
{
    let (report, applied) = sync_once_inner(edge_id, client).await?;
    report_runtime_status_once(runtime_id, applied, reporter).await?;

    Ok(report)
}

pub async fn sync_and_report_with_mqtt_publisher_once<C, R, P>(
    edge_id: &str,
    runtime_id: &str,
    client: &mut C,
    reporter: &mut R,
    publisher: &mut P,
) -> Result<EdgeConfigMqttSyncReport>
where
    C: EdgeConfigSyncClient + Send,
    R: RuntimeStatusReporter + Send,
    P: MqttPublisher + Send,
{
    let (report, applied) = sync_once_inner_with_mqtt_publisher(edge_id, client, publisher).await?;
    report_runtime_status_once(runtime_id, applied, reporter).await?;

    Ok(report)
}

pub async fn sync_and_report_mqtt_uplink_once<C, R>(
    edge_id: &str,
    runtime_id: &str,
    client: &mut C,
    reporter: &mut R,
) -> Result<EdgeConfigMqttSyncReport>
where
    C: EdgeConfigSyncClient + Send,
    R: RuntimeStatusReporter + Send,
{
    let desired = client.fetch_desired_config(edge_id).await?;
    let applied = apply_desired_config(edge_id, desired)?;
    let mut runtime = ConfiguredEdgeRuntime::new(applied.package().clone(), TokioSerialBusFactory)?;
    let collection = if !applied.package().mqtt_uplinks.is_empty() {
        let mut publisher =
            MultiBrokerMqttPublisher::connect_from_uplinks(&applied.package().mqtt_uplinks)?;
        if applied.package().data_configs.is_empty() {
            runtime
                .collect_once_and_publish_mqtt(&mut publisher)
                .await?
        } else {
            runtime
                .collect_data_configs_once_and_publish_mqtt(&mut publisher)
                .await?
        }
    } else {
        let collection = runtime.collect_once().await?;
        crate::ConfiguredMqttCollectionReport {
            collection,
            mqtt_messages_published: 0,
        }
    };
    let applied_version = runtime.reported_version().to_string();

    client
        .report_applied_version(edge_id, &applied_version)
        .await?;
    report_runtime_status_once(runtime_id, applied, reporter).await?;

    Ok(EdgeConfigMqttSyncReport {
        applied_version,
        samples_collected: collection.collection.samples_collected,
        mqtt_messages_published: collection.mqtt_messages_published,
    })
}

pub async fn sync_and_report_mqtt_uplink_with_store_once<C, R>(
    edge_id: &str,
    runtime_id: &str,
    client: &mut C,
    reporter: &mut R,
    store: &RocksEdgeRuntimeStore,
) -> Result<EdgeConfigMqttSyncReport>
where
    C: EdgeConfigSyncClient + Send,
    R: RuntimeStatusReporter + Send,
{
    let desired = client.fetch_desired_config(edge_id).await?;
    store.put_desired_config(&desired.package)?;
    let applied = apply_desired_config(edge_id, desired)?;
    let mut runtime = ConfiguredEdgeRuntime::new(applied.package().clone(), TokioSerialBusFactory)?;
    let collection = if !applied.package().mqtt_uplinks.is_empty() {
        let mut publisher =
            MultiBrokerMqttPublisher::connect_from_uplinks(&applied.package().mqtt_uplinks)?;
        if applied.package().data_configs.is_empty() {
            runtime
                .collect_once_and_publish_mqtt_with_outbox(store, &mut publisher)
                .await?
        } else {
            runtime
                .collect_data_configs_once_and_publish_mqtt_with_outbox(store, &mut publisher)
                .await?
        }
    } else {
        let collection = runtime.collect_once().await?;
        crate::ConfiguredMqttCollectionReport {
            collection,
            mqtt_messages_published: 0,
        }
    };
    let applied_version = runtime.reported_version().to_string();
    store.promote_active_config(edge_id, &applied_version)?;

    client
        .report_applied_version(edge_id, &applied_version)
        .await?;
    report_runtime_status_with_store_once(runtime_id, applied, store, reporter).await?;

    Ok(EdgeConfigMqttSyncReport {
        applied_version,
        samples_collected: collection.collection.samples_collected,
        mqtt_messages_published: collection.mqtt_messages_published,
    })
}

pub async fn sync_and_report_with_mqtt_publisher_and_store_once<C, R, P>(
    edge_id: &str,
    runtime_id: &str,
    client: &mut C,
    reporter: &mut R,
    store: &RocksEdgeRuntimeStore,
    publisher: &mut P,
) -> Result<EdgeConfigMqttSyncReport>
where
    C: EdgeConfigSyncClient + Send,
    R: RuntimeStatusReporter + Send,
    P: MqttPublisher + Send,
{
    let desired = client.fetch_desired_config(edge_id).await?;
    store.put_desired_config(&desired.package)?;
    let applied = apply_desired_config(edge_id, desired)?;
    let mut runtime = ConfiguredSimulatedRuntime::new(applied.clone());
    let collection = if applied.package().data_configs.is_empty() {
        runtime
            .collect_once_and_publish_mqtt_with_outbox(store, publisher)
            .await?
    } else {
        runtime
            .collect_data_configs_once_and_publish_mqtt_with_outbox(store, publisher)
            .await?
    };
    let applied_version = runtime.reported_version().to_string();
    store.promote_active_config(edge_id, &applied_version)?;

    client
        .report_applied_version(edge_id, &applied_version)
        .await?;
    report_runtime_status_with_store_once(runtime_id, applied, store, reporter).await?;

    Ok(EdgeConfigMqttSyncReport {
        applied_version,
        samples_collected: collection.collection.samples_collected,
        mqtt_messages_published: collection.mqtt_messages_published,
    })
}

async fn sync_once_inner<C>(
    edge_id: &str,
    client: &mut C,
) -> Result<(EdgeConfigSyncReport, AppliedEdgeConfig)>
where
    C: EdgeConfigSyncClient + Send,
{
    let desired = client.fetch_desired_config(edge_id).await?;
    let applied = apply_desired_config(edge_id, desired)?;
    let mut runtime = ConfiguredSimulatedRuntime::new(applied.clone());
    let collection = runtime.collect_once().await?;
    let applied_version = runtime.reported_version().to_string();

    client
        .report_applied_version(edge_id, &applied_version)
        .await?;

    Ok((
        EdgeConfigSyncReport {
            applied_version,
            samples_collected: collection.samples_collected,
        },
        applied,
    ))
}

async fn sync_once_inner_with_mqtt_publisher<C, P>(
    edge_id: &str,
    client: &mut C,
    publisher: &mut P,
) -> Result<(EdgeConfigMqttSyncReport, AppliedEdgeConfig)>
where
    C: EdgeConfigSyncClient + Send,
    P: MqttPublisher + Send,
{
    let desired = client.fetch_desired_config(edge_id).await?;
    let applied = apply_desired_config(edge_id, desired)?;
    let mut runtime = ConfiguredSimulatedRuntime::new(applied.clone());
    let collection = if applied.package().data_configs.is_empty() {
        runtime.collect_once_and_publish_mqtt(publisher).await?
    } else {
        runtime
            .collect_data_configs_once_and_publish_mqtt(publisher)
            .await?
    };
    let applied_version = runtime.reported_version().to_string();

    client
        .report_applied_version(edge_id, &applied_version)
        .await?;

    Ok((
        EdgeConfigMqttSyncReport {
            applied_version,
            samples_collected: collection.collection.samples_collected,
            mqtt_messages_published: collection.mqtt_messages_published,
        },
        applied,
    ))
}

fn apply_desired_config(edge_id: &str, desired: EdgeDesiredConfig) -> Result<AppliedEdgeConfig> {
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

    AppliedEdgeConfig::apply(desired.package)
}
