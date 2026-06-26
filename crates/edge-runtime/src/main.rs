use std::path::PathBuf;

use anyhow::Result;
use chrono::Utc;
use clap::Parser;
use edge_core::{DataQuality, TelemetrySample, TelemetryValue};
use edge_runtime::{EdgeRuntime, JsonlLocalStore, SimulatedProtocolAdapter};
use tracing::info;

#[derive(Debug, Parser)]
#[command(name = "edge-runtime")]
#[command(about = "Runs the edge runtime MVP with a simulated protocol adapter")]
struct Args {
    #[arg(long, default_value = "edge-dev")]
    edge_id: String,
    #[arg(long, default_value = "pump-1")]
    device_id: String,
    #[arg(long, default_value = "data/telemetry.jsonl")]
    storage: PathBuf,
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
