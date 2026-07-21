use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use edge_core::{
    CloudSyncMetrics, CollectionRuntimeMetrics, DataQuality, EdgeHealth,
    EdgeRuntimeMetricsSnapshot, LocalStoreMetrics, ProtocolRuntimeMetrics, SystemRuntimeMetrics,
    TelemetrySample, TelemetryValue,
};
use edge_runtime::{
    publish_edgelink_runtime_daemon_session, publish_edgelink_runtime_status_authenticated_once,
    publish_edgelink_runtime_status_tls_once,
    publish_edgelink_runtime_status_with_mqtt_uplink_once,
    publish_edgelink_runtime_status_with_store_and_capabilities_once,
    sync_and_report_mqtt_uplink_with_store_once, sync_and_report_once, AppliedEdgeConfig,
    CollectionRunStats, CollectionSchedule, ConfiguredEdgeRuntime, DataConfigSchedule,
    EdgeConfigSyncClient, EdgeLinkClientTlsConfig, EdgeRuntime, HttpEdgeConfigSyncClient,
    HttpRuntimeStatusReporter, JsonlLocalStore, MqttOutboxStats, MqttPublisher,
    MultiBrokerMqttPublisher, RocksEdgeRuntimeStore, RuntimeCapabilityConfig,
    RuntimeStatusReporter, SimulatedProtocolAdapter, SimulatedRuntimeMetricsCollector,
    TokioSerialBusFactory,
};
use tracing::{info, warn};

#[derive(Debug, Parser)]
#[command(name = "edge-runtime")]
#[command(about = "Runs the edge runtime MVP with a simulated protocol adapter")]
struct Args {
    #[arg(long, default_value = "edge-dev")]
    edge_id: String,
    #[arg(long, default_value = "pump-1")]
    device_id: String,
    #[arg(long, default_value = "runtime-dev")]
    runtime_id: String,
    #[arg(long, default_value = "data/telemetry.jsonl")]
    storage: PathBuf,
    #[arg(long, default_value = "data/edge-runtime.rocksdb")]
    runtime_db: PathBuf,
    #[arg(long)]
    cloud_api_url: Option<String>,
    #[arg(long)]
    cloud_gateway_addr: Option<String>,
    #[arg(long)]
    edgelink_tls_ca: Option<PathBuf>,
    #[arg(long)]
    edgelink_tls_cert: Option<PathBuf>,
    #[arg(long)]
    edgelink_tls_key: Option<PathBuf>,
    #[arg(long, default_value = "localhost")]
    edgelink_tls_server_name: String,
    #[arg(long)]
    access_token: Option<String>,
    #[arg(long, conflicts_with = "access_token")]
    access_token_env: Option<String>,
    #[arg(long)]
    mqtt_uplink: bool,
    #[arg(long)]
    edgelink_daemon: bool,
    #[arg(long, default_value_t = 30_000)]
    edgelink_command_wait_ms: u64,
    #[arg(long, default_value_t = 1_000)]
    edgelink_reconnect_ms: u64,
    #[arg(long, default_value_t = 0)]
    scheduled_ticks: u32,
    #[arg(long, default_value_t = 1000)]
    scheduler_tick_ms: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "edge_runtime=info".into()),
        )
        .init();

    let args = Args::parse();
    if let Some(cloud_gateway_addr) = args.cloud_gateway_addr.as_deref() {
        let access_token = resolve_access_token(&args)?;
        let edgelink_tls = load_edgelink_tls_config(&args)?;
        let runtime_store = RocksEdgeRuntimeStore::open(&args.runtime_db)?;
        let capabilities = RuntimeCapabilityConfig::serial_mqtt_defaults().capabilities();
        if args.edgelink_daemon {
            loop {
                let active_config = runtime_store
                    .recover_active_config(&args.edge_id)
                    .context("failed to recover active runtime config")?;
                let snapshot = runtime_metrics_snapshot(
                    &args.edge_id,
                    &args.runtime_id,
                    &args.storage,
                    active_config,
                    Some(runtime_store.mqtt_outbox_stats()?),
                );
                match publish_edgelink_runtime_daemon_session(
                    cloud_gateway_addr,
                    &args.edge_id,
                    &args.runtime_id,
                    env!("CARGO_PKG_VERSION"),
                    snapshot,
                    Vec::new(),
                    &runtime_store,
                    capabilities.clone(),
                    access_token.as_deref(),
                    args.mqtt_uplink,
                    edgelink_tls.as_ref(),
                    Duration::from_millis(args.edgelink_command_wait_ms),
                )
                .await
                {
                    Ok(report) => info!(
                        edge_id = %report.edge_id,
                        runtime_id = %report.runtime_id,
                        acked_message_count = report.acked_message_count,
                        discovery_reports_published = report.discovery_reports_published,
                        "edgelink daemon session completed"
                    ),
                    Err(error) => warn!(
                        edge_id = %args.edge_id,
                        error = %error,
                        "edgelink daemon session failed; reconnecting"
                    ),
                }
                tokio::time::sleep(Duration::from_millis(args.edgelink_reconnect_ms)).await;
            }
        }
        let active_config = runtime_store
            .recover_active_config(&args.edge_id)
            .context("failed to recover active runtime config")?;
        let snapshot = runtime_metrics_snapshot(
            &args.edge_id,
            &args.runtime_id,
            &args.storage,
            active_config,
            Some(runtime_store.mqtt_outbox_stats()?),
        );
        let report = if let Some(tls_config) = edgelink_tls.as_ref() {
            publish_edgelink_runtime_status_tls_once(
                cloud_gateway_addr,
                &args.edge_id,
                &args.runtime_id,
                env!("CARGO_PKG_VERSION"),
                snapshot,
                Vec::new(),
                &runtime_store,
                capabilities,
                access_token.as_deref(),
                args.mqtt_uplink,
                tls_config,
            )
            .await?
        } else if let Some(access_token) = access_token.as_deref() {
            publish_edgelink_runtime_status_authenticated_once(
                cloud_gateway_addr,
                &args.edge_id,
                &args.runtime_id,
                env!("CARGO_PKG_VERSION"),
                snapshot,
                Vec::new(),
                &runtime_store,
                capabilities,
                access_token,
                args.mqtt_uplink,
            )
            .await?
        } else if args.mqtt_uplink {
            publish_edgelink_runtime_status_with_mqtt_uplink_once(
                cloud_gateway_addr,
                &args.edge_id,
                &args.runtime_id,
                env!("CARGO_PKG_VERSION"),
                snapshot,
                Vec::new(),
                &runtime_store,
                capabilities,
            )
            .await?
        } else {
            publish_edgelink_runtime_status_with_store_and_capabilities_once(
                cloud_gateway_addr,
                &args.edge_id,
                &args.runtime_id,
                env!("CARGO_PKG_VERSION"),
                snapshot,
                Vec::new(),
                &runtime_store,
                capabilities,
            )
            .await?
        };

        info!(
            edge_id = %report.edge_id,
            runtime_id = %report.runtime_id,
            gateway_addr = %report.gateway_addr,
            acked_message_count = report.acked_message_count,
            samples_collected = report.samples_collected,
            mqtt_messages_published = report.mqtt_messages_published,
            discovery_reports_published = report.discovery_reports_published,
            runtime_db = %args.runtime_db.display(),
            "edgelink runtime status report completed"
        );

        return Ok(());
    }

    if let Some(cloud_api_url) = args.cloud_api_url {
        let runtime_store = RocksEdgeRuntimeStore::open(&args.runtime_db)?;
        let mut config_client = HttpEdgeConfigSyncClient::new(&cloud_api_url)?;
        let mut runtime_reporter = HttpRuntimeStatusReporter::new(&cloud_api_url)?;
        if args.scheduled_ticks > 0 {
            let report = run_scheduled_cloud_ticks(
                &args.edge_id,
                &args.runtime_id,
                &mut config_client,
                &mut runtime_reporter,
                args.scheduled_ticks,
                args.scheduler_tick_ms,
                args.mqtt_uplink,
                &runtime_store,
            )
            .await?;

            info!(
                edge_id = %args.edge_id,
                runtime_id = %args.runtime_id,
                applied_version = %report.applied_version,
                tasks_run = report.tasks_run,
                samples_collected = report.samples_collected,
                mqtt_messages_published = report.mqtt_messages_published,
                events_reported = report.events_reported,
                cloud_api_url = %cloud_api_url,
                "scheduled cloud config collection completed"
            );

            return Ok(());
        }

        if args.mqtt_uplink {
            let report = sync_and_report_mqtt_uplink_with_store_once(
                &args.edge_id,
                &args.runtime_id,
                &mut config_client,
                &mut runtime_reporter,
                &runtime_store,
            )
            .await?;

            info!(
                edge_id = %args.edge_id,
                runtime_id = %args.runtime_id,
                applied_version = %report.applied_version,
                samples_collected = report.samples_collected,
                mqtt_messages_published = report.mqtt_messages_published,
                cloud_api_url = %cloud_api_url,
                "cloud config sync, mqtt uplink, and runtime status report completed"
            );

            return Ok(());
        }

        let report = sync_and_report_once(
            &args.edge_id,
            &args.runtime_id,
            &mut config_client,
            &mut runtime_reporter,
        )
        .await?;

        info!(
            edge_id = %args.edge_id,
            runtime_id = %args.runtime_id,
            applied_version = %report.applied_version,
            samples_collected = report.samples_collected,
            cloud_api_url = %cloud_api_url,
            "cloud config sync and runtime status report completed"
        );

        return Ok(());
    }

    let sample = TelemetrySample::new(
        args.device_id.clone(),
        "pressure",
        TelemetryValue::Float(2.4),
        DataQuality::Good,
        Utc::now(),
    );
    let adapter = SimulatedProtocolAdapter::new(vec![sample]);
    let store = JsonlLocalStore::new(&args.storage);
    let mut runtime = EdgeRuntime::new(&args.edge_id, &args.device_id, adapter, store);
    let report = runtime.collect_once().await?;

    info!(
        edge_id = %args.edge_id,
        device_id = %args.device_id,
        samples_collected = report.samples_collected,
        storage = %args.storage.display(),
        "collection cycle completed"
    );

    Ok(())
}

fn resolve_access_token(args: &Args) -> Result<Option<String>> {
    if let Some(variable_name) = args.access_token_env.as_deref() {
        let variable_name = variable_name.trim();
        anyhow::ensure!(
            !variable_name.is_empty(),
            "--access-token-env cannot be empty"
        );
        let token = std::env::var(variable_name).with_context(|| {
            format!("missing EdgeLink access token environment variable {variable_name}")
        })?;
        anyhow::ensure!(
            !token.trim().is_empty(),
            "EdgeLink access token environment variable {variable_name} is empty"
        );
        return Ok(Some(token));
    }

    Ok(args.access_token.clone())
}

fn load_edgelink_tls_config(args: &Args) -> Result<Option<EdgeLinkClientTlsConfig>> {
    match (
        args.edgelink_tls_ca.as_ref(),
        args.edgelink_tls_cert.as_ref(),
        args.edgelink_tls_key.as_ref(),
    ) {
        (None, None, None) => Ok(None),
        (Some(ca_path), Some(cert_path), Some(key_path)) => Ok(Some(EdgeLinkClientTlsConfig {
            ca_cert_pem: std::fs::read_to_string(ca_path).with_context(|| {
                format!("failed to read EdgeLink CA certificate {}", ca_path.display())
            })?,
            client_cert_pem: std::fs::read_to_string(cert_path).with_context(|| {
                format!(
                    "failed to read EdgeLink client certificate {}",
                    cert_path.display()
                )
            })?,
            client_key_pem: std::fs::read_to_string(key_path).with_context(|| {
                format!("failed to read EdgeLink client key {}", key_path.display())
            })?,
            server_name: args.edgelink_tls_server_name.clone(),
        })),
        _ => anyhow::bail!(
            "--edgelink-tls-ca, --edgelink-tls-cert and --edgelink-tls-key must be configured together"
        ),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScheduledCloudRunReport {
    applied_version: String,
    tasks_run: usize,
    samples_collected: usize,
    mqtt_messages_published: usize,
    events_reported: usize,
}

async fn run_scheduled_cloud_ticks<C, R>(
    edge_id: &str,
    runtime_id: &str,
    config_client: &mut C,
    runtime_reporter: &mut R,
    scheduled_ticks: u32,
    scheduler_tick_ms: u64,
    mqtt_uplink: bool,
    store: &RocksEdgeRuntimeStore,
) -> Result<ScheduledCloudRunReport>
where
    C: EdgeConfigSyncClient + Send,
    R: RuntimeStatusReporter + Send,
{
    let desired = config_client.fetch_desired_config(edge_id).await?;
    store.put_desired_config(&desired.package)?;
    if desired.package.edge_id != edge_id {
        anyhow::bail!(
            "desired package targets edge {}, but runtime is {}",
            desired.package.edge_id,
            edge_id
        );
    }
    if desired.package.version != desired.desired_version {
        anyhow::bail!(
            "desired version {} does not match package version {}",
            desired.desired_version,
            desired.package.version
        );
    }

    let applied = edge_runtime::AppliedEdgeConfig::apply(desired.package.clone())?;
    if mqtt_uplink && !applied.package().data_configs.is_empty() {
        let mut publisher =
            MultiBrokerMqttPublisher::connect_from_uplinks(&applied.package().mqtt_uplinks)?;
        return run_scheduled_data_config_ticks(
            edge_id,
            runtime_id,
            config_client,
            runtime_reporter,
            applied,
            scheduled_ticks,
            scheduler_tick_ms,
            store,
            &mut publisher,
        )
        .await;
    }

    let mut schedule = CollectionSchedule::from_package(&desired.package)?;
    let mut runtime = ConfiguredEdgeRuntime::new(desired.package, TokioSerialBusFactory)?;
    let mut stats = CollectionRunStats::new(
        applied
            .package()
            .collection_tasks
            .iter()
            .filter(|task| task.enabled)
            .count(),
    );
    let mut tasks_run = 0;
    let mut samples_collected = 0;
    let mut failure_events = Vec::new();

    for tick in 0..scheduled_ticks {
        let now_ms = u64::from(tick).saturating_mul(scheduler_tick_ms);
        let started = Instant::now();
        let report = runtime
            .collect_due_tasks_resilient_once(&mut schedule, now_ms)
            .await?;
        let latency_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        stats.record_tick(
            report.tasks_run,
            report.tasks_succeeded,
            report.tasks_failed,
            latency_ms,
        );
        tasks_run += report.tasks_run;
        samples_collected += report.samples_collected;
        failure_events.extend(
            report
                .failures
                .into_iter()
                .map(|failure| failure.to_runtime_event(edge_id)),
        );
    }

    let applied_version = runtime.reported_version().to_string();
    store.promote_active_config(edge_id, &applied_version)?;
    config_client
        .report_applied_version(edge_id, &applied_version)
        .await?;
    let snapshot = SimulatedRuntimeMetricsCollector::new(runtime_id, applied)
        .with_collection_metrics(stats.metrics())
        .with_mqtt_outbox_stats(store.mqtt_outbox_stats()?)
        .snapshot();
    runtime_reporter.report_metrics(snapshot).await?;
    let events_reported = failure_events.len();
    for event in failure_events {
        runtime_reporter.report_event(event).await?;
    }

    Ok(ScheduledCloudRunReport {
        applied_version,
        tasks_run,
        samples_collected,
        mqtt_messages_published: 0,
        events_reported,
    })
}

async fn run_scheduled_data_config_ticks<C, R, P>(
    edge_id: &str,
    runtime_id: &str,
    config_client: &mut C,
    runtime_reporter: &mut R,
    applied: edge_runtime::AppliedEdgeConfig,
    scheduled_ticks: u32,
    scheduler_tick_ms: u64,
    store: &RocksEdgeRuntimeStore,
    publisher: &mut P,
) -> Result<ScheduledCloudRunReport>
where
    C: EdgeConfigSyncClient + Send,
    R: RuntimeStatusReporter + Send,
    P: MqttPublisher + Send,
{
    store.put_desired_config(applied.package())?;
    let mut schedule = DataConfigSchedule::from_package(applied.package())?;
    let mut runtime = ConfiguredEdgeRuntime::new(applied.package().clone(), TokioSerialBusFactory)?;
    let mut stats = CollectionRunStats::new(
        applied
            .package()
            .data_configs
            .iter()
            .filter(|config| config.enabled)
            .count(),
    );
    let mut tasks_run = 0;
    let mut samples_collected = 0;
    let mut mqtt_messages_published = 0;
    let mut failure_events = Vec::new();

    for tick in 0..scheduled_ticks {
        let now_ms = u64::from(tick).saturating_mul(scheduler_tick_ms);
        let started = Instant::now();
        let report = runtime
            .collect_due_data_configs_resilient_once_with_outbox(
                &mut schedule,
                now_ms,
                store,
                publisher,
            )
            .await?;
        let latency_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        stats.record_tick(
            report.data_configs_run,
            report.data_configs_succeeded,
            report.data_configs_failed,
            latency_ms,
        );
        tasks_run += report.data_configs_run;
        samples_collected += report.samples_collected;
        mqtt_messages_published += report.mqtt_messages_published;
        failure_events.extend(
            report
                .failures
                .into_iter()
                .map(|failure| failure.to_runtime_event(edge_id)),
        );
    }

    let applied_version = runtime.reported_version().to_string();
    store.promote_active_config(edge_id, &applied_version)?;
    config_client
        .report_applied_version(edge_id, &applied_version)
        .await?;
    let snapshot = SimulatedRuntimeMetricsCollector::new(runtime_id, applied)
        .with_collection_metrics(stats.metrics())
        .with_mqtt_outbox_stats(store.mqtt_outbox_stats()?)
        .snapshot();
    runtime_reporter.report_metrics(snapshot).await?;
    let events_reported = failure_events.len();
    for event in failure_events {
        runtime_reporter.report_event(event).await?;
    }

    Ok(ScheduledCloudRunReport {
        applied_version,
        tasks_run,
        samples_collected,
        mqtt_messages_published,
        events_reported,
    })
}

fn runtime_metrics_snapshot(
    edge_id: &str,
    runtime_id: &str,
    storage: &std::path::Path,
    active_config: Option<AppliedEdgeConfig>,
    mqtt_outbox_stats: Option<MqttOutboxStats>,
) -> EdgeRuntimeMetricsSnapshot {
    let fallback = EdgeRuntimeMetricsSnapshot {
        edge_id: edge_id.to_string(),
        runtime_id: runtime_id.to_string(),
        config_version: "local-runtime".to_string(),
        timestamp: Utc::now(),
        health: EdgeHealth::Healthy,
        system: SystemRuntimeMetrics {
            cpu_percent: 0.0,
            memory_percent: 0.0,
            disk_percent: 0.0,
            process_uptime_seconds: 0,
        },
        collection: CollectionRuntimeMetrics {
            active_task_count: 0,
            success_rate: 1.0,
            average_latency_ms: 0,
            bad_point_count: 0,
        },
        protocols: vec![ProtocolRuntimeMetrics {
            connection_id: "simulated-local".to_string(),
            protocol: "Simulated".to_string(),
            connected: true,
            latency_ms: 0,
            timeout_count: 0,
            error_count: 0,
            reconnect_count: 0,
        }],
        local_store: LocalStoreMetrics {
            backend: storage
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or("jsonl")
                .to_string(),
            buffered_records: 0,
            oldest_buffer_age_seconds: 0,
            disk_usage_percent: 0.0,
        },
        algorithms: Vec::new(),
        cloud_sync: CloudSyncMetrics {
            connected: true,
            last_sync_seconds_ago: 0,
            pending_uploads: 0,
            desired_version: "local-runtime".to_string(),
            reported_version: "local-runtime".to_string(),
        },
    };
    let Some(active_config) = active_config else {
        return apply_outbox_stats(fallback, mqtt_outbox_stats);
    };

    let mut collector = SimulatedRuntimeMetricsCollector::new(runtime_id, active_config);
    if let Some(stats) = mqtt_outbox_stats {
        collector = collector.with_mqtt_outbox_stats(stats);
    }
    let mut recovered = collector.snapshot();
    recovered.timestamp = fallback.timestamp;
    recovered.system = fallback.system;
    recovered.local_store = fallback.local_store;
    apply_outbox_stats(recovered, mqtt_outbox_stats)
}

fn apply_outbox_stats(
    mut snapshot: EdgeRuntimeMetricsSnapshot,
    stats: Option<MqttOutboxStats>,
) -> EdgeRuntimeMetricsSnapshot {
    let Some(stats) = stats else {
        return snapshot;
    };
    snapshot.local_store.backend = "rocksdb-mqtt-outbox".to_string();
    snapshot.local_store.buffered_records = stats.pending_messages;
    snapshot.local_store.oldest_buffer_age_seconds = stats.oldest_message_age_seconds;
    snapshot.cloud_sync.pending_uploads = stats.pending_messages;
    if stats.pending_messages > 0 {
        snapshot.health = EdgeHealth::Degraded;
    }
    snapshot
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use edge_core::{
        DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint, DataConfigPublish,
        DeviceInstance, EdgeConfigPackage, EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot,
        MqttUplinkConfig, PointAddress, ProtocolConnection, TelemetryPointMapping, TelemetryType,
    };
    use edge_runtime::{AppliedEdgeConfig, EdgeDesiredConfig, RecordingMqttPublisher};
    use tempfile::tempdir;

    use super::*;

    #[derive(Clone)]
    struct MemorySyncClient {
        desired: EdgeDesiredConfig,
        reported: Vec<(String, String)>,
    }

    #[derive(Default)]
    struct MemoryRuntimeReporter {
        metrics: Vec<EdgeRuntimeMetricsSnapshot>,
        events: Vec<EdgeRuntimeEvent>,
    }

    #[async_trait]
    impl EdgeConfigSyncClient for MemorySyncClient {
        async fn fetch_desired_config(
            &mut self,
            edge_id: &str,
        ) -> anyhow::Result<EdgeDesiredConfig> {
            assert_eq!(edge_id, "edge-dev");
            Ok(self.desired.clone())
        }

        async fn report_applied_version(
            &mut self,
            edge_id: &str,
            version: &str,
        ) -> anyhow::Result<()> {
            self.reported
                .push((edge_id.to_string(), version.to_string()));
            Ok(())
        }
    }

    #[async_trait]
    impl RuntimeStatusReporter for MemoryRuntimeReporter {
        async fn report_metrics(
            &mut self,
            snapshot: EdgeRuntimeMetricsSnapshot,
        ) -> anyhow::Result<()> {
            self.metrics.push(snapshot);
            Ok(())
        }

        async fn report_event(&mut self, event: EdgeRuntimeEvent) -> anyhow::Result<()> {
            self.events.push(event);
            Ok(())
        }
    }

    fn data_config_package() -> EdgeConfigPackage {
        EdgeConfigPackage::new("edge-dev", "2026.07.01-scheduled-data")
            .with_device(DeviceInstance::new("pump-1", "pump"))
            .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
            .with_mqtt_uplink(
                MqttUplinkConfig::velamq("velamq-main", "mqtt://velamq.local:1883", "edge-dev")
                    .with_topic_template("unused/{edge_id}/{device_id}/{telemetry_id}"),
            )
            .with_point_mapping(TelemetryPointMapping::new(
                "pressure",
                "pump-1",
                "pump.pressure",
                "sim-main",
                PointAddress::simulated("pressure"),
                TelemetryType::Float,
            ))
            .with_point_mapping(TelemetryPointMapping::new(
                "running",
                "pump-1",
                "pump.running",
                "sim-main",
                PointAddress::simulated("running"),
                TelemetryType::Boolean,
            ))
            .with_data_config(
                DataConfig::new(
                    "pump_status_fast",
                    "泵状态",
                    "pump-1",
                    "sim-main",
                    DataConfigCollection::new(1000),
                    DataConfigPublish::new(
                        "velamq-main",
                        "factory/{edge_id}/{device_id}/status",
                        DataConfigPayload::object(),
                    ),
                )
                .with_point(DataConfigPoint::new(
                    "pressure",
                    "pump.pressure",
                    PointAddress::simulated("pressure"),
                    TelemetryType::Float,
                    "pressure",
                )),
            )
            .with_data_config(
                DataConfig::new(
                    "pump_running_slow",
                    "泵运行",
                    "pump-1",
                    "sim-main",
                    DataConfigCollection::new(5000),
                    DataConfigPublish::new(
                        "velamq-main",
                        "factory/{edge_id}/{device_id}/running",
                        DataConfigPayload::object(),
                    ),
                )
                .with_point(DataConfigPoint::new(
                    "running",
                    "pump.running",
                    PointAddress::simulated("running"),
                    TelemetryType::Boolean,
                    "running",
                )),
            )
    }

    #[tokio::test]
    async fn scheduled_data_config_ticks_publish_mqtt_and_report_metrics() {
        let package = data_config_package();
        let applied = AppliedEdgeConfig::apply(package.clone()).unwrap();
        let mut client = MemorySyncClient {
            desired: EdgeDesiredConfig {
                desired_version: package.version.clone(),
                package,
            },
            reported: Vec::new(),
        };
        let mut reporter = MemoryRuntimeReporter::default();
        let mut publisher = RecordingMqttPublisher::default();
        let dir = tempdir().unwrap();
        let store = RocksEdgeRuntimeStore::open(dir.path().join("runtime.rocksdb")).unwrap();

        let report = run_scheduled_data_config_ticks(
            "edge-dev",
            "runtime-test",
            &mut client,
            &mut reporter,
            applied,
            2,
            1000,
            &store,
            &mut publisher,
        )
        .await
        .unwrap();

        assert_eq!(report.applied_version, "2026.07.01-scheduled-data");
        assert_eq!(report.tasks_run, 3);
        assert_eq!(report.samples_collected, 3);
        assert_eq!(report.mqtt_messages_published, 3);
        assert_eq!(report.events_reported, 0);
        assert_eq!(
            client.reported,
            vec![(
                "edge-dev".to_string(),
                "2026.07.01-scheduled-data".to_string()
            )]
        );
        assert_eq!(reporter.metrics.len(), 1);
        assert_eq!(reporter.metrics[0].collection.active_task_count, 2);
        assert_eq!(reporter.metrics[0].collection.success_rate, 1.0);
        assert_eq!(publisher.messages().len(), 3);
        assert_eq!(
            store.active_version("edge-dev").unwrap().as_deref(),
            Some("2026.07.01-scheduled-data")
        );
        assert_eq!(store.mqtt_outbox_len().unwrap(), 0);
        assert_eq!(
            publisher.messages()[0].topic,
            "factory/edge-dev/pump-1/status"
        );
        assert_eq!(
            publisher.messages()[1].topic,
            "factory/edge-dev/pump-1/running"
        );
        assert_eq!(
            publisher.messages()[2].topic,
            "factory/edge-dev/pump-1/status"
        );
    }
}
