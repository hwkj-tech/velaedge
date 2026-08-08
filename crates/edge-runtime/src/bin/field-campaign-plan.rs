use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Parser;
use edge_runtime::{evaluate_field_campaign_plan, FieldInteroperabilityPolicy};
use sha2::{Digest, Sha256};

#[derive(Debug, Parser)]
#[command(
    name = "field-campaign-plan",
    about = "Validate physical asset coverage and campaign deployment inputs before field execution"
)]
struct Cli {
    #[arg(long)]
    plan: PathBuf,
    #[arg(long, default_value = "deploy/field-acceptance-policy.json")]
    policy: PathBuf,
    #[arg(long, default_value = "target/field-campaign-plan/report.json")]
    output: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let plan_bytes = std::fs::read(&cli.plan)
        .with_context(|| format!("read field campaign plan {}", cli.plan.display()))?;
    let policy_bytes = std::fs::read(&cli.policy)
        .with_context(|| format!("read field acceptance policy {}", cli.policy.display()))?;
    let policy = FieldInteroperabilityPolicy::from_json_slice(&policy_bytes)
        .with_context(|| format!("load field acceptance policy {}", cli.policy.display()))?;
    let policy_sha256 = format!("{:x}", Sha256::digest(&policy_bytes));
    let report = evaluate_field_campaign_plan(&plan_bytes, &policy, policy_sha256)?;

    if let Some(parent) = cli.output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create plan report directory {}", parent.display()))?;
    }
    let report_bytes = serde_json::to_vec_pretty(&report)?;
    std::fs::write(&cli.output, &report_bytes)
        .with_context(|| format!("write field campaign plan report {}", cli.output.display()))?;
    println!("{}", String::from_utf8_lossy(&report_bytes));
    if !report.passed() {
        bail!(
            "field campaign deployment plan failed; report retained at {}",
            cli.output.display()
        );
    }
    Ok(())
}
