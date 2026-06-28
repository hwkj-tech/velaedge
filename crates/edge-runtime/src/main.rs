use std::path::PathBuf;

use anyhow::Result;
use chrono::Utc;
use clap::Parser;
use edge_core::{
    CloudSyncMetrics, CollectionRuntimeMetrics, DataQuality, EdgeHealth,
    EdgeRuntimeMetricsSnapshot, LocalStoreMetrics, ProtocolRuntimeMetrics, SystemRuntimeMetrics,
    TelemetrySample, TelemetryValue,
};
use edge_runtime::{
    publish_edgelink_runtime_status_with_mqtt_uplink_once,
    publish_edgelink_runtime_status_with_store_and_capabilities_once, report_runtime_status_once,
    sync_and_report_mqtt_uplink_once, sync_and_report_once, CollectionSchedule,
    ConfiguredEdgeRuntime, EdgeConfigSyncClient, EdgeRuntime, HttpEdgeConfigSyncClient,
    HttpRuntimeStatusReporter, JsonlLocalStore, RocksEdgeRuntimeStore, RuntimeCapabilityConfig,
    SimulatedProtocolAdapter, TokioSerialBusFactory,
};
use tracing::info;

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
    mqtt_uplink: bool,
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
    if let Some(cloud_gateway_addr) = args.cloud_gateway_addr {
        let snapshot = runtime_metrics_snapshot(&args.edge_id, &args.runtime_id, &args.storage);
        let runtime_store = RocksEdgeRuntimeStore::open(&args.runtime_db)?;
        let capabilities = RuntimeCapabilityConfig::serial_mqtt_defaults().capabilities();
        let report = if args.mqtt_uplink {
            publish_edgelink_runtime_status_with_mqtt_uplink_once(
                &cloud_gateway_addr,
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
                &cloud_gateway_addr,
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
            mqtt_messages_published = report.mqtt_messages_published,
            runtime_db = %args.runtime_db.display(),
            "edgelink runtime status report completed"
        );

        return Ok(());
    }

    if let Some(cloud_api_url) = args.cloud_api_url {
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
            )
            .await?;

            info!(
                edge_id = %args.edge_id,
                runtime_id = %args.runtime_id,
                applied_version = %report.applied_version,
                tasks_run = report.tasks_run,
                samples_collected = report.samples_collected,
                cloud_api_url = %cloud_api_url,
                "scheduled cloud config collection completed"
            );

            return Ok(());
        }

        if args.mqtt_uplink {
            let report = sync_and_report_mqtt_uplink_once(
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScheduledCloudRunReport {
    applied_version: String,
    tasks_run: usize,
    samples_collected: usize,
}

async fn run_scheduled_cloud_ticks<C, R>(
    edge_id: &str,
    runtime_id: &str,
    config_client: &mut C,
    runtime_reporter: &mut R,
    scheduled_ticks: u32,
    scheduler_tick_ms: u64,
) -> Result<ScheduledCloudRunReport>
where
    C: EdgeConfigSyncClient + Send,
    R: edge_runtime::RuntimeStatusReporter + Send,
{
    let desired = config_client.fetch_desired_config(edge_id).await?;
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
    let mut schedule = CollectionSchedule::from_package(&desired.package)?;
    let mut runtime = ConfiguredEdgeRuntime::new(desired.package, TokioSerialBusFactory)?;
    let mut tasks_run = 0;
    let mut samples_collected = 0;

    for tick in 0..scheduled_ticks {
        let now_ms = u64::from(tick).saturating_mul(scheduler_tick_ms);
        let report = runtime
            .collect_due_tasks_once(&mut schedule, now_ms)
            .await?;
        tasks_run += report.tasks_run;
        samples_collected += report.samples_collected;
    }

    let applied_version = runtime.reported_version().to_string();
    config_client
        .report_applied_version(edge_id, &applied_version)
        .await?;
    report_runtime_status_once(runtime_id, applied, runtime_reporter).await?;

    Ok(ScheduledCloudRunReport {
        applied_version,
        tasks_run,
        samples_collected,
    })
}

fn runtime_metrics_snapshot(
    edge_id: &str,
    runtime_id: &str,
    storage: &std::path::Path,
) -> EdgeRuntimeMetricsSnapshot {
    EdgeRuntimeMetricsSnapshot {
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
    }
}
