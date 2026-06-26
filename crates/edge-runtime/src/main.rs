use std::path::PathBuf;

use anyhow::Result;
use chrono::Utc;
use clap::Parser;
use edge_core::{DataQuality, TelemetrySample, TelemetryValue};
use edge_runtime::{
    sync_and_report_once, EdgeRuntime, HttpEdgeConfigSyncClient, HttpRuntimeStatusReporter,
    JsonlLocalStore, SimulatedProtocolAdapter,
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
    #[arg(long)]
    cloud_api_url: Option<String>,
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
