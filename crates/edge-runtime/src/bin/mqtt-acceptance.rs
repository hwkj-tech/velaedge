use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use edge_runtime::{run_mqtt_acceptance, MqttAcceptanceOptions};

#[derive(Debug, Parser)]
#[command(
    name = "mqtt-acceptance",
    about = "Verify an MQTT/VelaMQ broker with a subscribe-publish-readback round trip"
)]
struct Args {
    #[arg(long)]
    broker: String,

    #[arg(long, default_value = "edgeops-acceptance")]
    client_id_prefix: String,

    #[arg(long)]
    topic: Option<String>,

    #[arg(long, requires = "password_env")]
    username: Option<String>,

    #[arg(long, requires = "username")]
    password_env: Option<String>,

    #[arg(long)]
    tls_ca_path: Option<String>,

    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(0..=2))]
    qos: u8,

    #[arg(long, default_value_t = 10_000)]
    timeout_ms: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let mut options = MqttAcceptanceOptions::new(args.broker, args.client_id_prefix)
        .with_qos(args.qos)
        .with_timeout(Duration::from_millis(args.timeout_ms));
    if let Some(topic) = args.topic {
        options = options.with_topic(topic);
    }
    if let (Some(username), Some(password_env)) = (args.username, args.password_env) {
        options = options.with_credentials_env(username, password_env);
    }
    if let Some(tls_ca_path) = args.tls_ca_path {
        options = options.with_tls_ca_path(tls_ca_path);
    }

    let report = run_mqtt_acceptance(options).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
