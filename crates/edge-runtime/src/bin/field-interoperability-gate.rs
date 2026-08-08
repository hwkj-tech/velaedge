use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use anyhow::{bail, Context, Result};
use clap::Parser;
use edge_runtime::{
    evaluate_field_interoperability, read_field_campaign_evidence,
    read_field_interoperability_artifacts, FieldInteroperabilityPolicy,
};

#[derive(Debug, Parser)]
#[command(
    name = "field-interoperability-gate",
    about = "Verify multi-vendor physical field-endurance evidence for industrial protocols"
)]
struct Cli {
    #[arg(long = "campaign-dir")]
    campaign_dirs: Vec<PathBuf>,
    #[arg(long = "report")]
    reports: Vec<PathBuf>,
    #[arg(long = "package")]
    packages: Vec<PathBuf>,
    #[arg(long = "broker-receipt")]
    broker_receipts: Vec<PathBuf>,
    /// Schema v1 native Broker audits, position-matched with --report.
    #[arg(long = "native-broker-audit")]
    native_broker_audits: Vec<PathBuf>,
    #[arg(long = "require-protocol")]
    required_protocols: Vec<String>,
    #[arg(
        long,
        conflicts_with_all = [
            "required_protocols",
            "minimum_manufacturers_per_protocol",
            "minimum_models_per_protocol",
            "minimum_duration_seconds",
            "maximum_failure_ratio",
            "maximum_progress_gap_seconds"
        ]
    )]
    policy: Option<PathBuf>,
    #[arg(long, default_value_t = 2)]
    minimum_manufacturers_per_protocol: usize,
    #[arg(long, default_value_t = 1)]
    minimum_models_per_protocol: usize,
    #[arg(long, default_value_t = 86_400)]
    minimum_duration_seconds: u64,
    #[arg(long, default_value_t = 0.01)]
    maximum_failure_ratio: f64,
    #[arg(long, default_value_t = 300)]
    maximum_progress_gap_seconds: u64,
    #[arg(long, default_value = "target/field-interoperability/report.json")]
    output: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.packages.len() != cli.reports.len()
        || cli.broker_receipts.len() != cli.reports.len()
        || cli.native_broker_audits.len() != cli.reports.len()
    {
        bail!(
            "each --report requires one position-matched --package, --broker-receipt and --native-broker-audit"
        );
    }
    if cli.campaign_dirs.is_empty() && cli.reports.is_empty() {
        bail!("provide at least one --campaign-dir or one report/package/broker-receipt triplet");
    }
    let mut evidence = Vec::with_capacity(cli.campaign_dirs.len() + cli.reports.len());
    for directory in &cli.campaign_dirs {
        evidence.push(read_field_campaign_evidence(directory)?);
    }
    for (index, path) in cli.reports.iter().enumerate() {
        evidence.push(read_field_interoperability_artifacts(
            path,
            &cli.packages[index],
            &cli.broker_receipts[index],
            &cli.native_broker_audits[index],
            path.display().to_string(),
        )?);
    }
    let policy = load_policy(&cli).await?;
    let report = evaluate_field_interoperability(&policy, &evidence)?;
    if let Some(parent) = cli.output.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("create report directory {}", parent.display()))?;
    }
    tokio::fs::write(&cli.output, serde_json::to_vec_pretty(&report)?)
        .await
        .with_context(|| format!("write interoperability report {}", cli.output.display()))?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.passed() {
        bail!(
            "field interoperability acceptance failed; report retained at {}",
            cli.output.display()
        );
    }
    Ok(())
}

async fn load_policy(cli: &Cli) -> Result<FieldInteroperabilityPolicy> {
    let Some(path) = cli.policy.as_ref() else {
        let required_protocols = if cli.required_protocols.is_empty() {
            FieldInteroperabilityPolicy::default().required_protocols
        } else {
            cli.required_protocols
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>()
        };
        return Ok(FieldInteroperabilityPolicy {
            required_protocols,
            minimum_manufacturers_per_protocol: cli.minimum_manufacturers_per_protocol,
            minimum_models_per_protocol: cli.minimum_models_per_protocol,
            minimum_manufacturers_by_protocol: BTreeMap::new(),
            minimum_models_by_protocol: BTreeMap::new(),
            minimum_duration_ms: cli.minimum_duration_seconds.saturating_mul(1_000),
            maximum_failure_ratio: cli.maximum_failure_ratio,
            maximum_progress_gap_ms: cli.maximum_progress_gap_seconds.saturating_mul(1_000),
        });
    };

    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("read field interoperability policy {}", path.display()))?;
    FieldInteroperabilityPolicy::from_json_slice(&bytes)
        .with_context(|| format!("load field interoperability policy {}", path.display()))
}
