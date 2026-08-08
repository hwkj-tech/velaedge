use std::{collections::BTreeSet, path::PathBuf, time::Duration};

use anyhow::{bail, Context, Result};
use clap::Parser;
use edge_core::EdgeConfigPackage;
use edge_runtime::{run_field_endurance_acceptance, FieldDeviceIdentity, FieldEnduranceOptions};
use sha2::{Digest, Sha256};

#[derive(Debug, Parser)]
#[command(
    name = "field-endurance",
    about = "Run a released VelaEdge product package through production protocol and MQTT paths"
)]
struct Cli {
    #[arg(long)]
    config: PathBuf,
    #[arg(long, default_value_t = 86_400)]
    duration_seconds: u64,
    #[arg(long, default_value_t = 100)]
    scheduler_interval_ms: u64,
    #[arg(long)]
    minimum_cycles: Option<u64>,
    #[arg(long, default_value_t = 0.01)]
    maximum_failure_ratio: f64,
    /// Maximum interval without successful collection or MQTT publish progress.
    #[arg(long, default_value_t = 300)]
    maximum_progress_gap_seconds: u64,
    #[arg(long)]
    require_recovery: bool,
    #[arg(long = "require-changing-point", value_name = "DEVICE_ID/POINT_ID")]
    changing_points: Vec<String>,
    #[arg(long)]
    skip_mqtt: bool,
    #[arg(long)]
    physical_device_exercised: bool,
    #[arg(long)]
    site_id: Option<String>,
    #[arg(long)]
    operator: Option<String>,
    #[arg(long)]
    device_connection_id: Option<String>,
    #[arg(long)]
    device_manufacturer: Option<String>,
    #[arg(long)]
    device_model: Option<String>,
    #[arg(long)]
    device_serial: Option<String>,
    #[arg(long, default_value = "target/field-endurance/runtime.rocksdb")]
    rocksdb_path: PathBuf,
    #[arg(long, default_value = "target/field-endurance/report.json")]
    report: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let bytes = tokio::fs::read(&cli.config)
        .await
        .with_context(|| format!("read configuration package {}", cli.config.display()))?;
    let package = serde_json::from_slice::<EdgeConfigPackage>(&bytes)
        .with_context(|| format!("decode configuration package {}", cli.config.display()))?;
    let duration = Duration::from_secs(cli.duration_seconds);
    let minimum_period_ms = package
        .data_configs
        .iter()
        .filter(|config| config.enabled)
        .map(|config| config.collection.period_ms)
        .min()
        .unwrap_or(1_000);
    let derived_minimum_cycles = duration
        .as_millis()
        .div_ceil(u128::from(minimum_period_ms.max(1)))
        .saturating_mul(9)
        / 10;
    let physical_device = cli.physical_device_exercised.then(|| FieldDeviceIdentity {
        site_id: cli.site_id.unwrap_or_default(),
        operator: cli.operator.unwrap_or_default(),
        connection_id: cli.device_connection_id.unwrap_or_default(),
        manufacturer: cli.device_manufacturer.unwrap_or_default(),
        model: cli.device_model.unwrap_or_default(),
        serial_number: cli.device_serial.unwrap_or_default(),
    });
    let options = FieldEnduranceOptions {
        package,
        package_sha256: Some(format!("{:x}", Sha256::digest(&bytes))),
        duration,
        scheduler_interval: Duration::from_millis(cli.scheduler_interval_ms),
        minimum_cycles: cli
            .minimum_cycles
            .unwrap_or_else(|| u64::try_from(derived_minimum_cycles.max(1)).unwrap_or(u64::MAX)),
        maximum_failure_ratio: cli.maximum_failure_ratio,
        maximum_progress_gap: Duration::from_secs(cli.maximum_progress_gap_seconds),
        require_recovery: cli.require_recovery,
        changing_points: cli.changing_points.into_iter().collect::<BTreeSet<_>>(),
        exercise_mqtt: !cli.skip_mqtt,
        physical_device_exercised: cli.physical_device_exercised,
        physical_device,
        rocksdb_path: cli.rocksdb_path,
    };
    let report = run_field_endurance_acceptance(options).await?;
    if let Some(parent) = cli.report.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("create report directory {}", parent.display()))?;
    }
    tokio::fs::write(&cli.report, serde_json::to_vec_pretty(&report)?)
        .await
        .with_context(|| format!("write field endurance report {}", cli.report.display()))?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.passed() {
        bail!(
            "field endurance acceptance failed; report retained at {}",
            cli.report.display()
        );
    }
    Ok(())
}
