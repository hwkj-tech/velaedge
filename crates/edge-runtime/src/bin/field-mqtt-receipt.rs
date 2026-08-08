use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use clap::Parser;
use edge_core::EdgeConfigPackage;
use edge_runtime::{capture_mqtt_field_receipt, MqttFieldReceiptOptions};
use sha2::{Digest, Sha256};

#[derive(Debug, Parser)]
#[command(
    name = "field-mqtt-receipt",
    about = "Capture broker-side MQTT delivery evidence for a released VelaEdge package"
)]
struct Cli {
    #[arg(long)]
    config: PathBuf,
    #[arg(long, default_value_t = 86_460)]
    duration_seconds: u64,
    #[arg(long, default_value_t = 30)]
    startup_timeout_seconds: u64,
    #[arg(long, default_value = "target/field-endurance/broker-receipt.json")]
    output: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    let package_bytes = tokio::fs::read(&cli.config)
        .await
        .with_context(|| format!("read configuration package {}", cli.config.display()))?;
    let package = serde_json::from_slice::<EdgeConfigPackage>(&package_bytes)
        .with_context(|| format!("decode configuration package {}", cli.config.display()))?;
    let package_sha256 = format!("{:x}", Sha256::digest(&package_bytes));

    eprintln!(
        "connecting field consumer for edge {} version {}; capture starts after every subscription is acknowledged",
        package.edge_id, package.version
    );
    let receipt = capture_mqtt_field_receipt(
        MqttFieldReceiptOptions::new(package, package_sha256)
            .with_duration(Duration::from_secs(cli.duration_seconds))
            .with_startup_timeout(Duration::from_secs(cli.startup_timeout_seconds)),
    )
    .await?;
    write_json_atomically(&cli.output, &receipt).await?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    eprintln!("broker receipt retained at {}", cli.output.display());
    Ok(())
}

async fn write_json_atomically(
    path: &std::path::Path,
    value: &impl serde::Serialize,
) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("create broker receipt directory {}", parent.display()))?;
    }
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4().simple()));
    tokio::fs::write(&temporary, serde_json::to_vec_pretty(value)?)
        .await
        .with_context(|| format!("write temporary broker receipt {}", temporary.display()))?;
    if let Err(error) = tokio::fs::rename(&temporary, path).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error).with_context(|| {
            format!(
                "publish broker receipt {} from {}",
                path.display(),
                temporary.display()
            )
        });
    }
    Ok(())
}
