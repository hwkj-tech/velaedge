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
    publish_edgelink_runtime_status_with_store_and_capabilities_once, sync_and_report_once,
    EdgeRuntime, HttpEdgeConfigSyncClient, HttpRuntimeStatusReporter, JsonlLocalStore,
    RocksEdgeRuntimeStore, RuntimeCapabilityConfig, SimulatedProtocolAdapter,
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
        let report = publish_edgelink_runtime_status_with_store_and_capabilities_once(
            &cloud_gateway_addr,
            &args.edge_id,
            &args.runtime_id,
            env!("CARGO_PKG_VERSION"),
            snapshot,
            Vec::new(),
            &runtime_store,
            capabilities,
        )
        .await?;

        info!(
            edge_id = %report.edge_id,
            runtime_id = %report.runtime_id,
            gateway_addr = %report.gateway_addr,
            acked_message_count = report.acked_message_count,
            runtime_db = %args.runtime_db.display(),
            "edgelink runtime status report completed"
        );

        return Ok(());
    }

    if let Some(cloud_api_url) = args.cloud_api_url {
        let mut config_client = HttpEdgeConfigSyncClient::new(&cloud_api_url)?;
        let mut runtime_reporter = HttpRuntimeStatusReporter::new(&cloud_api_url)?;
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
