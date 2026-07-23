use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::Parser;
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(about = "Captures MQTT messages for edge-runtime acceptance evidence")]
struct Args {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 1883)]
    port: u16,
    #[arg(long, default_value = "factory/#")]
    topic: String,
    #[arg(long, default_value_t = 1)]
    count: usize,
    #[arg(long, default_value_t = 10)]
    timeout_seconds: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CapturedMessage {
    topic: String,
    qos: u8,
    retained: bool,
    payload_bytes: usize,
    payload: serde_json::Value,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if args.count == 0 || args.count > 1_000 {
        bail!("count must be between 1 and 1000");
    }

    let mut options = MqttOptions::new(
        format!("edge-runtime-capture-{}", uuid::Uuid::new_v4().simple()),
        &args.host,
        args.port,
    );
    options.set_keep_alive(Duration::from_secs(10));
    let (client, mut event_loop) = AsyncClient::new(options, 32);
    client
        .subscribe(&args.topic, QoS::AtLeastOnce)
        .await
        .with_context(|| format!("failed to subscribe to {}", args.topic))?;

    let capture = async {
        let mut messages = Vec::with_capacity(args.count);
        while messages.len() < args.count {
            if let Event::Incoming(Incoming::Publish(publish)) = event_loop.poll().await? {
                let payload = serde_json::from_slice(&publish.payload).unwrap_or_else(|_| {
                    serde_json::Value::String(
                        String::from_utf8_lossy(&publish.payload).into_owned(),
                    )
                });
                messages.push(CapturedMessage {
                    topic: publish.topic,
                    qos: match publish.qos {
                        QoS::AtMostOnce => 0,
                        QoS::AtLeastOnce => 1,
                        QoS::ExactlyOnce => 2,
                    },
                    retained: publish.retain,
                    payload_bytes: publish.payload.len(),
                    payload,
                });
            }
        }
        Ok::<_, anyhow::Error>(messages)
    };

    let messages = tokio::time::timeout(Duration::from_secs(args.timeout_seconds), capture)
        .await
        .context("timed out waiting for MQTT messages")??;
    println!("{}", serde_json::to_string_pretty(&messages)?);
    Ok(())
}
