use std::{path::PathBuf, time::Duration};

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use edge_core::{MqttProtocolVersion, MqttUplinkConfig};
use edge_runtime::{run_modbus_tcp_endurance_acceptance, ModbusTcpEnduranceOptions};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum MqttVersionArg {
    #[value(name = "3.1.1")]
    V3_1_1,
    #[value(name = "5.0")]
    V5_0,
}

impl From<MqttVersionArg> for MqttProtocolVersion {
    fn from(value: MqttVersionArg) -> Self {
        match value {
            MqttVersionArg::V3_1_1 => Self::V3_1_1,
            MqttVersionArg::V5_0 => Self::V5_0,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "modbus-tcp-endurance",
    about = "Run production-path Modbus TCP endurance and optional MQTT QoS 1 acceptance"
)]
struct Cli {
    #[arg(long, default_value = "127.0.0.1:1502")]
    endpoint: String,
    #[arg(long, default_value_t = 86_400)]
    duration_seconds: u64,
    #[arg(long, default_value_t = 1_000)]
    interval_ms: u64,
    #[arg(long)]
    minimum_cycles: Option<u64>,
    #[arg(long, default_value_t = 0.01)]
    maximum_failure_ratio: f64,
    #[arg(long)]
    allow_static_values: bool,
    #[arg(long)]
    require_recovery: bool,
    #[arg(long)]
    physical_device_exercised: bool,
    #[arg(long)]
    mqtt_broker: Option<String>,
    #[arg(long, default_value = "modbus-endurance")]
    mqtt_sink_id: String,
    #[arg(long, default_value = "modbus-endurance-runtime")]
    mqtt_client_id: String,
    #[arg(long, value_enum, default_value = "3.1.1")]
    mqtt_version: MqttVersionArg,
    #[arg(long)]
    mqtt_username: Option<String>,
    #[arg(long)]
    mqtt_password_env: Option<String>,
    #[arg(long)]
    mqtt_ca_path: Option<String>,
    #[arg(long, default_value = "target/modbus-endurance/runtime.rocksdb")]
    rocksdb_path: PathBuf,
    #[arg(long, default_value = "target/modbus-endurance/report.json")]
    report: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let duration = Duration::from_secs(cli.duration_seconds);
    let interval = Duration::from_millis(cli.interval_ms);
    let derived_minimum_cycles = duration
        .as_millis()
        .div_ceil(interval.as_millis().max(1))
        .saturating_mul(9)
        / 10;
    let mqtt_uplink = cli.mqtt_broker.map(|broker| {
        let mut uplink = MqttUplinkConfig::velamq(cli.mqtt_sink_id, broker, cli.mqtt_client_id)
            .with_protocol_version(cli.mqtt_version.into())
            .with_qos(1);
        uplink.username = cli.mqtt_username;
        uplink.password_env = cli.mqtt_password_env;
        uplink.tls_ca_path = cli.mqtt_ca_path;
        uplink
    });
    let options = ModbusTcpEnduranceOptions {
        endpoint: cli.endpoint,
        duration,
        interval,
        minimum_cycles: cli
            .minimum_cycles
            .unwrap_or_else(|| u64::try_from(derived_minimum_cycles.max(1)).unwrap_or(u64::MAX)),
        maximum_failure_ratio: cli.maximum_failure_ratio,
        require_dynamic_values: !cli.allow_static_values,
        require_recovery: cli.require_recovery,
        physical_device_exercised: cli.physical_device_exercised,
        mqtt_uplink,
        rocksdb_path: cli.rocksdb_path,
    };
    let report = run_modbus_tcp_endurance_acceptance(options).await?;
    if let Some(parent) = cli.report.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("create report directory {}", parent.display()))?;
    }
    tokio::fs::write(&cli.report, serde_json::to_vec_pretty(&report)?)
        .await
        .with_context(|| format!("write endurance report {}", cli.report.display()))?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.passed() {
        bail!(
            "Modbus TCP endurance acceptance failed; report retained at {}",
            cli.report.display()
        );
    }
    Ok(())
}
