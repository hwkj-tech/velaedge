use std::{io::Cursor, sync::Arc};

use anyhow::{bail, Context, Result};
use edge_core::{
    decode_edgelink_frame, encode_edgelink_frame, EdgeLinkMessage, EdgeLinkPayload,
    EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot, EDGELINK_MAX_FRAME_BYTES,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};
use tokio_rustls::{
    rustls::{
        self,
        pki_types::{CertificateDer, PrivateKeyDer, ServerName},
        RootCertStore,
    },
    TlsConnector,
};

use crate::{
    run_modbus_discovery_request, AppliedEdgeConfig, ConfiguredEdgeRuntime,
    ConfiguredMqttCollectionReport, ConfiguredSimulatedRuntime, MqttPublisher,
    MultiBrokerMqttPublisher, RocksEdgeRuntimeStore, SerialBusFactory, TokioSerialBusFactory,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeLinkConnectReport {
    pub edge_id: String,
    pub runtime_id: String,
    pub gateway_addr: String,
    pub acked: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeLinkPublishReport {
    pub edge_id: String,
    pub runtime_id: String,
    pub gateway_addr: String,
    pub acked_message_count: usize,
    pub applied_config_version: Option<String>,
    pub samples_collected: usize,
    pub mqtt_messages_published: usize,
    pub discovery_reports_published: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeLinkClientTlsConfig {
    pub ca_cert_pem: String,
    pub client_cert_pem: String,
    pub client_key_pem: String,
    pub server_name: String,
}

#[derive(Clone)]
pub struct EdgeLinkClientTlsConnector {
    connector: TlsConnector,
    server_name: ServerName<'static>,
}

impl EdgeLinkClientTlsConfig {
    pub fn build_connector(&self) -> Result<EdgeLinkClientTlsConnector> {
        let ca_certs = load_certs(&self.ca_cert_pem, "CA certificate")?;
        let client_certs = load_certs(&self.client_cert_pem, "client certificate")?;
        let client_key = load_private_key(&self.client_key_pem, "client private key")?;

        let mut roots = RootCertStore::empty();
        for cert in ca_certs {
            roots.add(cert).context("invalid CA certificate")?;
        }

        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .context("failed to select EdgeLink TLS protocol versions")?
            .with_root_certificates(roots)
            .with_client_auth_cert(client_certs, client_key)
            .context("failed to build EdgeLink client TLS config")?;
        let server_name = ServerName::try_from(self.server_name.as_str())
            .context("invalid EdgeLink TLS server name")?
            .to_owned();

        Ok(EdgeLinkClientTlsConnector {
            connector: TlsConnector::from(Arc::new(config)),
            server_name,
        })
    }
}

pub async fn connect_edgelink_once(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    applied_config_version: Option<String>,
) -> Result<EdgeLinkConnectReport> {
    connect_edgelink_once_with_capabilities(
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        applied_config_version,
        Vec::new(),
    )
    .await
}

pub async fn connect_edgelink_once_with_capabilities(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    applied_config_version: Option<String>,
    capabilities: Vec<String>,
) -> Result<EdgeLinkConnectReport> {
    let mut stream = TcpStream::connect(gateway_addr)
        .await
        .with_context(|| format!("failed to connect EdgeLink gateway at {gateway_addr}"))?;
    connect_edgelink_over_stream(
        &mut stream,
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        applied_config_version,
        capabilities,
        None,
    )
    .await
}

pub async fn connect_edgelink_tls_once(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    applied_config_version: Option<String>,
    tls_config: &EdgeLinkClientTlsConfig,
) -> Result<EdgeLinkConnectReport> {
    let stream = TcpStream::connect(gateway_addr)
        .await
        .with_context(|| format!("failed to connect EdgeLink gateway at {gateway_addr}"))?;
    let tls = tls_config.build_connector()?;
    let mut stream = tls
        .connector
        .connect(tls.server_name, stream)
        .await
        .context("failed to open EdgeLink TLS session")?;

    connect_edgelink_over_stream(
        &mut stream,
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        applied_config_version,
        Vec::new(),
        None,
    )
    .await
}

pub async fn publish_edgelink_runtime_status_once(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    snapshot: EdgeRuntimeMetricsSnapshot,
    events: Vec<EdgeRuntimeEvent>,
) -> Result<EdgeLinkPublishReport> {
    publish_edgelink_runtime_status_inner(
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        snapshot,
        events,
        None,
        Vec::new(),
        EdgeLinkMqttMode::Disabled,
        None,
        None,
        Duration::ZERO,
    )
    .await
}

pub async fn publish_edgelink_runtime_status_with_store_once(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    snapshot: EdgeRuntimeMetricsSnapshot,
    events: Vec<EdgeRuntimeEvent>,
    store: &RocksEdgeRuntimeStore,
) -> Result<EdgeLinkPublishReport> {
    publish_edgelink_runtime_status_with_store_and_capabilities_once(
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        snapshot,
        events,
        store,
        Vec::new(),
    )
    .await
}

pub async fn publish_edgelink_runtime_status_with_store_and_capabilities_once(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    snapshot: EdgeRuntimeMetricsSnapshot,
    events: Vec<EdgeRuntimeEvent>,
    store: &RocksEdgeRuntimeStore,
    capabilities: Vec<String>,
) -> Result<EdgeLinkPublishReport> {
    publish_edgelink_runtime_status_inner(
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        snapshot,
        events,
        Some(store),
        capabilities,
        EdgeLinkMqttMode::Disabled,
        None,
        None,
        Duration::ZERO,
    )
    .await
}

pub async fn publish_edgelink_runtime_status_with_mqtt_publisher_once<P>(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    snapshot: EdgeRuntimeMetricsSnapshot,
    events: Vec<EdgeRuntimeEvent>,
    publisher: &mut P,
) -> Result<EdgeLinkPublishReport>
where
    P: MqttPublisher + Send,
{
    publish_edgelink_runtime_status_inner(
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        snapshot,
        events,
        None,
        Vec::new(),
        EdgeLinkMqttMode::Provided(publisher),
        None,
        None,
        Duration::ZERO,
    )
    .await
}

pub async fn publish_edgelink_runtime_status_with_mqtt_uplink_once(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    snapshot: EdgeRuntimeMetricsSnapshot,
    events: Vec<EdgeRuntimeEvent>,
    store: &RocksEdgeRuntimeStore,
    capabilities: Vec<String>,
) -> Result<EdgeLinkPublishReport> {
    publish_edgelink_runtime_status_inner(
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        snapshot,
        events,
        Some(store),
        capabilities,
        EdgeLinkMqttMode::ConfiguredUplink,
        None,
        None,
        Duration::ZERO,
    )
    .await
}

pub async fn publish_edgelink_runtime_status_authenticated_once(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    snapshot: EdgeRuntimeMetricsSnapshot,
    events: Vec<EdgeRuntimeEvent>,
    store: &RocksEdgeRuntimeStore,
    capabilities: Vec<String>,
    access_token: &str,
    mqtt_uplink: bool,
) -> Result<EdgeLinkPublishReport> {
    publish_edgelink_runtime_status_inner(
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        snapshot,
        events,
        Some(store),
        capabilities,
        if mqtt_uplink {
            EdgeLinkMqttMode::ConfiguredUplink
        } else {
            EdgeLinkMqttMode::Disabled
        },
        Some(access_token),
        None,
        Duration::ZERO,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn publish_edgelink_runtime_status_tls_once(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    snapshot: EdgeRuntimeMetricsSnapshot,
    events: Vec<EdgeRuntimeEvent>,
    store: &RocksEdgeRuntimeStore,
    capabilities: Vec<String>,
    access_token: Option<&str>,
    mqtt_uplink: bool,
    tls_config: &EdgeLinkClientTlsConfig,
) -> Result<EdgeLinkPublishReport> {
    publish_edgelink_runtime_status_inner(
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        snapshot,
        events,
        Some(store),
        capabilities,
        if mqtt_uplink {
            EdgeLinkMqttMode::ConfiguredUplink
        } else {
            EdgeLinkMqttMode::Disabled
        },
        access_token,
        Some(tls_config),
        Duration::ZERO,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn publish_edgelink_runtime_daemon_session(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    snapshot: EdgeRuntimeMetricsSnapshot,
    events: Vec<EdgeRuntimeEvent>,
    store: &RocksEdgeRuntimeStore,
    capabilities: Vec<String>,
    access_token: Option<&str>,
    mqtt_uplink: bool,
    tls_config: Option<&EdgeLinkClientTlsConfig>,
    command_wait: Duration,
) -> Result<EdgeLinkPublishReport> {
    publish_edgelink_runtime_status_inner(
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        snapshot,
        events,
        Some(store),
        capabilities,
        if mqtt_uplink {
            EdgeLinkMqttMode::ConfiguredUplink
        } else {
            EdgeLinkMqttMode::Disabled
        },
        access_token,
        tls_config,
        command_wait,
    )
    .await
}

async fn publish_edgelink_runtime_status_inner(
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    snapshot: EdgeRuntimeMetricsSnapshot,
    events: Vec<EdgeRuntimeEvent>,
    store: Option<&RocksEdgeRuntimeStore>,
    capabilities: Vec<String>,
    mqtt_mode: EdgeLinkMqttMode<'_>,
    access_token: Option<&str>,
    tls_config: Option<&EdgeLinkClientTlsConfig>,
    command_wait: Duration,
) -> Result<EdgeLinkPublishReport> {
    if snapshot.edge_id != edge_id {
        bail!("runtime metrics edge_id does not match EdgeLink edge_id");
    }
    if snapshot.runtime_id != runtime_id {
        bail!("runtime metrics runtime_id does not match EdgeLink runtime_id");
    }
    for event in &events {
        if event.edge_id != edge_id {
            bail!("runtime event edge_id does not match EdgeLink edge_id");
        }
    }

    let stream = TcpStream::connect(gateway_addr)
        .await
        .with_context(|| format!("failed to connect EdgeLink gateway at {gateway_addr}"))?;
    if let Some(tls_config) = tls_config {
        let tls = tls_config.build_connector()?;
        let mut stream = tls
            .connector
            .connect(tls.server_name, stream)
            .await
            .context("failed to open EdgeLink TLS session")?;
        let report = publish_edgelink_runtime_status_over_stream(
            &mut stream,
            gateway_addr,
            edge_id,
            runtime_id,
            runtime_version,
            snapshot,
            events,
            store,
            capabilities,
            mqtt_mode,
            access_token,
            command_wait,
        )
        .await?;
        stream
            .shutdown()
            .await
            .context("failed to close EdgeLink TLS session")?;
        return Ok(report);
    }
    let mut stream = stream;
    publish_edgelink_runtime_status_over_stream(
        &mut stream,
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        snapshot,
        events,
        store,
        capabilities,
        mqtt_mode,
        access_token,
        command_wait,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn publish_edgelink_runtime_status_over_stream<S>(
    stream: &mut S,
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    mut snapshot: EdgeRuntimeMetricsSnapshot,
    events: Vec<EdgeRuntimeEvent>,
    store: Option<&RocksEdgeRuntimeStore>,
    capabilities: Vec<String>,
    mqtt_mode: EdgeLinkMqttMode<'_>,
    access_token: Option<&str>,
    command_wait: Duration,
) -> Result<EdgeLinkPublishReport>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    connect_edgelink_over_stream(
        stream,
        gateway_addr,
        edge_id,
        runtime_id,
        runtime_version,
        Some(snapshot.config_version.clone()),
        capabilities,
        access_token,
    )
    .await?;

    let config_apply =
        apply_optional_config_deploy(stream, edge_id, runtime_id, store, mqtt_mode).await?;
    if let Some(applied_version) = config_apply.applied_config_version.as_ref() {
        snapshot.config_version = applied_version.clone();
        snapshot.cloud_sync.desired_version = applied_version.clone();
        snapshot.cloud_sync.reported_version = applied_version.clone();
    }
    let mut acked_message_count = 0;
    let metrics =
        EdgeLinkMessage::runtime_metrics(edge_id, runtime_id, config_apply.next_sequence, snapshot);
    write_edgelink_message_and_expect_ack(stream, &metrics).await?;
    acked_message_count += 1;

    for (index, event) in events.into_iter().enumerate() {
        let message = EdgeLinkMessage::runtime_event(
            edge_id,
            runtime_id,
            config_apply.next_sequence + index as u64 + 1,
            event,
        );
        write_edgelink_message_and_expect_ack(stream, &message).await?;
        acked_message_count += 1;
    }

    let mut discovery_factory = TokioSerialBusFactory;
    let discovery_reports_published = handle_edgelink_discovery_requests_with_factory(
        stream,
        edge_id,
        runtime_id,
        store,
        &mut discovery_factory,
        command_wait,
    )
    .await?;

    Ok(EdgeLinkPublishReport {
        edge_id: edge_id.to_string(),
        runtime_id: runtime_id.to_string(),
        gateway_addr: gateway_addr.to_string(),
        acked_message_count,
        applied_config_version: config_apply.applied_config_version,
        samples_collected: config_apply.samples_collected,
        mqtt_messages_published: config_apply.mqtt_messages_published,
        discovery_reports_published,
    })
}

pub async fn handle_edgelink_discovery_requests_with_factory<S, F>(
    stream: &mut S,
    edge_id: &str,
    runtime_id: &str,
    store: Option<&RocksEdgeRuntimeStore>,
    factory: &mut F,
    command_wait: Duration,
) -> Result<usize>
where
    S: AsyncRead + AsyncWrite + Unpin,
    F: SerialBusFactory,
{
    if command_wait.is_zero() {
        return Ok(0);
    }
    let deadline = tokio::time::Instant::now() + command_wait;
    let mut published = 0;
    loop {
        let message = match tokio::time::timeout_at(deadline, read_edgelink_message(stream)).await {
            Err(_) => return Ok(published),
            Ok(result) => result.context("failed to receive EdgeLink runtime command")?,
        };
        let request_message_id = message.message_id;
        let request_sequence = message.sequence;
        let request = match message.payload {
            EdgeLinkPayload::DiscoveryRequest(request)
                if message.edge_id == edge_id
                    && message.runtime_id.as_deref() == Some(runtime_id) =>
            {
                request
            }
            EdgeLinkPayload::DiscoveryRequest(_) => {
                write_discovery_nack(
                    stream,
                    edge_id,
                    runtime_id,
                    request_message_id,
                    request_sequence,
                    "discovery request targets a different runtime",
                )
                .await?;
                continue;
            }
            _ => {
                write_discovery_nack(
                    stream,
                    edge_id,
                    runtime_id,
                    request_message_id,
                    request_sequence,
                    "runtime command is not a discovery request",
                )
                .await?;
                continue;
            }
        };

        let result = async {
            let store = store.context("discovery requires a persistent runtime store")?;
            let applied = store
                .recover_active_config(edge_id)
                .context("failed to recover active config for discovery")?
                .context("runtime has no active config for discovery")?;
            run_modbus_discovery_request(applied.package(), request, factory).await
        }
        .await;

        match result {
            Ok(report) => {
                let report = EdgeLinkMessage::discovery_report(
                    edge_id,
                    runtime_id,
                    request_sequence.saturating_add(1),
                    report,
                );
                write_edgelink_message_and_expect_ack(stream, &report).await?;
                published += 1;
            }
            Err(error) => {
                write_discovery_nack(
                    stream,
                    edge_id,
                    runtime_id,
                    request_message_id,
                    request_sequence,
                    error.to_string(),
                )
                .await?;
            }
        }
    }
}

async fn write_discovery_nack<S>(
    stream: &mut S,
    edge_id: &str,
    runtime_id: &str,
    request_message_id: uuid::Uuid,
    request_sequence: u64,
    reason: impl Into<String>,
) -> Result<()>
where
    S: AsyncWrite + Unpin,
{
    let nack = EdgeLinkMessage::nack(
        edge_id,
        Some(runtime_id),
        request_message_id,
        request_sequence,
        reason,
    );
    write_edgelink_message(stream, &nack).await
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EdgeLinkOptionalConfigApply {
    applied_config_version: Option<String>,
    next_sequence: u64,
    samples_collected: usize,
    mqtt_messages_published: usize,
}

enum EdgeLinkMqttMode<'a> {
    Disabled,
    Provided(&'a mut dyn MqttPublisher),
    ConfiguredUplink,
}

async fn connect_edgelink_over_stream<S>(
    stream: &mut S,
    gateway_addr: &str,
    edge_id: &str,
    runtime_id: &str,
    runtime_version: &str,
    applied_config_version: Option<String>,
    capabilities: Vec<String>,
    access_token: Option<&str>,
) -> Result<EdgeLinkConnectReport>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let hello = EdgeLinkMessage::hello_with_access_token(
        edge_id,
        runtime_id,
        runtime_version,
        applied_config_version,
        capabilities,
        access_token.map(str::to_string),
    );
    write_edgelink_message(stream, &hello)
        .await
        .context("failed to write EdgeLink hello")?;

    let ack = read_edgelink_message(stream)
        .await
        .context("failed to read EdgeLink hello ack")?;
    let payload = match ack.payload {
        EdgeLinkPayload::Ack(payload) => payload,
        EdgeLinkPayload::Nack(payload) => {
            bail!(
                "EdgeLink gateway rejected hello: {}",
                payload.reason.as_deref().unwrap_or("unknown reason")
            )
        }
        _ => bail!("EdgeLink gateway did not return an ack"),
    };
    if payload.ack_message_id != hello.message_id || payload.ack_sequence != hello.sequence {
        bail!("EdgeLink gateway ack does not match hello message");
    }

    Ok(EdgeLinkConnectReport {
        edge_id: edge_id.to_string(),
        runtime_id: runtime_id.to_string(),
        gateway_addr: gateway_addr.to_string(),
        acked: payload.accepted,
    })
}

async fn apply_optional_config_deploy<S>(
    stream: &mut S,
    edge_id: &str,
    runtime_id: &str,
    store: Option<&RocksEdgeRuntimeStore>,
    mqtt_mode: EdgeLinkMqttMode<'_>,
) -> Result<EdgeLinkOptionalConfigApply>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let message = match timeout(Duration::from_millis(50), read_edgelink_message(stream)).await {
        Err(_) => {
            if let Some(store) = store {
                if let Some(applied) = store
                    .recover_active_config(edge_id)
                    .context("failed to recover active EdgeLink config")?
                {
                    let applied_config_version = applied.version().to_string();
                    let mut runtime = ConfiguredSimulatedRuntime::new(applied.clone());
                    let collection =
                        collect_after_config_deploy(&mut runtime, applied, Some(store), mqtt_mode)
                            .await
                            .context("failed to collect with recovered EdgeLink config")?;
                    return Ok(EdgeLinkOptionalConfigApply {
                        applied_config_version: Some(applied_config_version),
                        next_sequence: 2,
                        samples_collected: collection.collection.samples_collected,
                        mqtt_messages_published: collection.mqtt_messages_published,
                    });
                }
            }
            return Ok(EdgeLinkOptionalConfigApply {
                applied_config_version: None,
                next_sequence: 2,
                samples_collected: 0,
                mqtt_messages_published: 0,
            });
        }
        Ok(result) => result.context("failed to read optional EdgeLink config deploy")?,
    };

    let EdgeLinkPayload::ConfigDeploy(package) = message.payload else {
        bail!("expected EdgeLink config deploy from gateway");
    };
    let desired_version = package.version.clone();
    let report_sequence = message.sequence + 1;

    if package.edge_id != edge_id {
        let report = EdgeLinkMessage::config_report(
            edge_id,
            runtime_id,
            report_sequence,
            desired_version,
            None,
            false,
            Some("config package targets a different edge".to_string()),
        );
        write_edgelink_message_and_expect_ack(stream, &report).await?;
        bail!("config package targets a different edge");
    }

    if let Some(store) = store {
        store
            .put_desired_config(&package)
            .context("failed to persist EdgeLink desired config")?;
    }

    let applied = match AppliedEdgeConfig::apply(package) {
        Ok(applied) => applied,
        Err(error) => {
            let report = EdgeLinkMessage::config_report(
                edge_id,
                runtime_id,
                report_sequence,
                desired_version,
                None,
                false,
                Some(error.to_string()),
            );
            write_edgelink_message_and_expect_ack(stream, &report).await?;
            return Err(error).context("failed to apply EdgeLink config deploy");
        }
    };
    let mut runtime = ConfiguredSimulatedRuntime::new(applied.clone());
    let collection = collect_after_config_deploy(&mut runtime, applied, store, mqtt_mode)
        .await
        .context("failed to run collection after EdgeLink config deploy")?;
    let applied_version = runtime.reported_version().to_string();
    if let Some(store) = store {
        store
            .promote_active_config(edge_id, &applied_version)
            .context("failed to promote EdgeLink active config")?;
    }

    let report = EdgeLinkMessage::config_report(
        edge_id,
        runtime_id,
        report_sequence,
        desired_version,
        Some(applied_version.clone()),
        true,
        None,
    );
    write_edgelink_message_and_expect_ack(stream, &report).await?;

    Ok(EdgeLinkOptionalConfigApply {
        applied_config_version: Some(applied_version),
        next_sequence: report_sequence + 1,
        samples_collected: collection.collection.samples_collected,
        mqtt_messages_published: collection.mqtt_messages_published,
    })
}

async fn collect_after_config_deploy(
    runtime: &mut ConfiguredSimulatedRuntime,
    applied: AppliedEdgeConfig,
    store: Option<&RocksEdgeRuntimeStore>,
    mqtt_mode: EdgeLinkMqttMode<'_>,
) -> Result<ConfiguredMqttCollectionReport> {
    match mqtt_mode {
        EdgeLinkMqttMode::Provided(publisher) => {
            if let Some(store) = store {
                if applied.package().data_configs.is_empty() {
                    runtime
                        .collect_once_and_publish_mqtt_with_outbox(store, publisher)
                        .await
                } else {
                    runtime
                        .collect_data_configs_once_and_publish_mqtt_with_outbox(store, publisher)
                        .await
                }
            } else if applied.package().data_configs.is_empty() {
                runtime.collect_once_and_publish_mqtt(publisher).await
            } else {
                runtime
                    .collect_data_configs_once_and_publish_mqtt(publisher)
                    .await
            }
        }
        EdgeLinkMqttMode::ConfiguredUplink => {
            if !applied.package().mqtt_uplinks.is_empty() {
                let mut publisher = MultiBrokerMqttPublisher::connect_from_uplinks(
                    &applied.package().mqtt_uplinks,
                )?;
                let mut configured_runtime =
                    ConfiguredEdgeRuntime::new(applied.package().clone(), TokioSerialBusFactory)?;
                if applied.package().data_configs.is_empty() {
                    if let Some(store) = store {
                        configured_runtime
                            .collect_once_and_publish_mqtt_with_outbox(store, &mut publisher)
                            .await
                    } else {
                        configured_runtime
                            .collect_once_and_publish_mqtt(&mut publisher)
                            .await
                    }
                } else {
                    if let Some(store) = store {
                        configured_runtime
                            .collect_data_configs_once_and_publish_mqtt_with_outbox(
                                store,
                                &mut publisher,
                            )
                            .await
                    } else {
                        configured_runtime
                            .collect_data_configs_once_and_publish_mqtt(&mut publisher)
                            .await
                    }
                }
            } else {
                collect_without_mqtt(runtime).await
            }
        }
        EdgeLinkMqttMode::Disabled => collect_without_mqtt(runtime).await,
    }
}

async fn collect_without_mqtt(
    runtime: &mut ConfiguredSimulatedRuntime,
) -> Result<ConfiguredMqttCollectionReport> {
    let collection = runtime.collect_once().await?;
    Ok(ConfiguredMqttCollectionReport {
        collection,
        mqtt_messages_published: 0,
    })
}

async fn write_edgelink_message_and_expect_ack<S>(
    stream: &mut S,
    message: &EdgeLinkMessage,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    write_edgelink_message(stream, message).await?;
    let ack = read_edgelink_message(stream).await?;
    let EdgeLinkPayload::Ack(payload) = ack.payload else {
        bail!("EdgeLink gateway did not return an ack");
    };
    if payload.ack_message_id != message.message_id || payload.ack_sequence != message.sequence {
        bail!("EdgeLink gateway ack does not match message");
    }
    if !payload.accepted {
        bail!(
            "EdgeLink gateway rejected message: {}",
            payload.reason.unwrap_or_else(|| "no reason".to_string())
        );
    }
    Ok(())
}

async fn read_edgelink_message<S>(stream: &mut S) -> Result<EdgeLinkMessage>
where
    S: AsyncRead + Unpin,
{
    let mut header = [0_u8; 4];
    stream
        .read_exact(&mut header)
        .await
        .context("failed to read EdgeLink frame header")?;

    let payload_len = u32::from_be_bytes(header) as usize;
    if payload_len > EDGELINK_MAX_FRAME_BYTES {
        bail!(
            "EdgeLink frame too large: {} bytes exceeds {} bytes",
            payload_len,
            EDGELINK_MAX_FRAME_BYTES
        );
    }

    let mut frame = vec![0_u8; 4 + payload_len];
    frame[..4].copy_from_slice(&header);
    stream
        .read_exact(&mut frame[4..])
        .await
        .context("failed to read EdgeLink frame body")?;

    decode_edgelink_frame(&frame).context("failed to decode EdgeLink frame")
}

async fn write_edgelink_message<S>(stream: &mut S, message: &EdgeLinkMessage) -> Result<()>
where
    S: AsyncWrite + Unpin,
{
    let frame = encode_edgelink_frame(message).context("failed to encode EdgeLink frame")?;
    stream
        .write_all(&frame)
        .await
        .context("failed to write EdgeLink frame")?;
    Ok(())
}

fn load_certs(pem: &str, label: &str) -> Result<Vec<CertificateDer<'static>>> {
    let mut reader = Cursor::new(pem.as_bytes());
    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to read {label} PEM"))?;
    if certs.is_empty() {
        bail!("missing {label}");
    }
    Ok(certs)
}

fn load_private_key(pem: &str, label: &str) -> Result<PrivateKeyDer<'static>> {
    let mut reader = Cursor::new(pem.as_bytes());
    rustls_pemfile::private_key(&mut reader)
        .with_context(|| format!("failed to read {label} PEM"))?
        .with_context(|| format!("missing {label}"))
}
