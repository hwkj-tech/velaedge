use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use clap::Parser;
use edge_runtime::{evaluate_field_campaign_site_status, FieldInteroperabilityPolicy};
use sha2::{Digest, Sha256};

#[derive(Debug, Parser)]
#[command(
    name = "field-campaign-status",
    about = "Inspect a physical-site campaign plan, verify completed evidence and report interoperability progress"
)]
struct Cli {
    #[arg(long)]
    plan: PathBuf,
    #[arg(long, default_value = "deploy/field-acceptance-policy.json")]
    policy: PathBuf,
    #[arg(long, default_value = "target/field-campaign-status/report.json")]
    output: PathBuf,
    /// Exit unsuccessfully while any planned campaign is pending or running.
    #[arg(long)]
    require_complete: bool,
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
    let report = evaluate_field_campaign_site_status(&plan_bytes, &policy, policy_sha256)?;

    let report_bytes = serde_json::to_vec_pretty(&report)?;
    write_atomically(&cli.output, &report_bytes)?;
    println!("{}", String::from_utf8_lossy(&report_bytes));

    if report.failed() {
        bail!(
            "field campaign site status failed; report retained at {}",
            cli.output.display()
        );
    }
    if cli.require_complete && !report.passed() {
        bail!(
            "field campaign site is not complete; report retained at {}",
            cli.output.display()
        );
    }
    Ok(())
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("create site status directory {}", parent.display()))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .context("field campaign site status output requires a UTF-8 file name")?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_nanos();
    let temporary = parent.join(format!(".{file_name}.tmp-{}-{nonce}", std::process::id()));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .with_context(|| format!("create temporary site status {}", temporary.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("write temporary site status {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("sync temporary site status {}", temporary.display()))?;
        std::fs::rename(&temporary, path).with_context(|| {
            format!(
                "replace field campaign site status {} with {}",
                path.display(),
                temporary.display()
            )
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
