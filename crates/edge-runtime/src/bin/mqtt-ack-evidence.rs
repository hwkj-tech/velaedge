use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::Parser;
use edge_runtime::{MqttPublishAcknowledgement, RocksEdgeRuntimeStore};
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(about = "Exports bounded MQTT broker acknowledgement evidence from Runtime RocksDB")]
struct Args {
    #[arg(long)]
    runtime_db: PathBuf,
    #[arg(long, default_value_t = 1_000)]
    limit: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceReport {
    receipt_count: usize,
    acknowledgements: Vec<MqttPublishAcknowledgement>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.limit == 0 || args.limit > 10_000 {
        bail!("limit must be between 1 and 10000");
    }
    let store = RocksEdgeRuntimeStore::open(&args.runtime_db)?;
    let acknowledgements = store.mqtt_publish_acknowledgements(args.limit)?;
    let report = EvidenceReport {
        receipt_count: acknowledgements.len(),
        acknowledgements,
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
