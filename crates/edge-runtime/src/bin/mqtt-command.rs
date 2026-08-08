use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::Parser;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};

#[derive(Debug, Parser)]
#[command(
    name = "mqtt-command",
    about = "Publish an MQTT command and wait for the VelaEdge Runtime reply"
)]
struct Args {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, default_value_t = 1883)]
    port: u16,

    #[arg(long)]
    topic: String,

    #[arg(long)]
    reply_topic: String,

    #[arg(long)]
    payload: String,

    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(0..=2))]
    qos: u8,

    #[arg(long, default_value_t = 8_000)]
    timeout_ms: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    serde_json::from_str::<serde_json::Value>(&args.payload)
        .context("command payload must be valid JSON")?;

    let client_id = format!("velaedge-command-probe-{}", uuid::Uuid::new_v4().simple());
    let mut options = MqttOptions::new(client_id, &args.host, args.port);
    options.set_keep_alive(Duration::from_secs(10));
    let (client, mut eventloop) = AsyncClient::new(options, 20);
    let qos = qos(args.qos);

    client
        .subscribe(&args.reply_topic, qos)
        .await
        .with_context(|| format!("subscribe to {}", args.reply_topic))?;

    let timeout = Duration::from_millis(args.timeout_ms);
    tokio::time::timeout(timeout, async {
        loop {
            if matches!(eventloop.poll().await?, Event::Incoming(Packet::SubAck(_))) {
                return Ok::<_, rumqttc::ConnectionError>(());
            }
        }
    })
    .await
    .context("MQTT subscription acknowledgement timed out")??;

    client
        .publish(&args.topic, qos, false, args.payload.as_bytes())
        .await
        .with_context(|| format!("publish command to {}", args.topic))?;

    let reply = tokio::time::timeout(timeout, async {
        loop {
            if let Event::Incoming(Packet::Publish(publish)) = eventloop.poll().await? {
                if publish.topic == args.reply_topic {
                    return Ok::<_, rumqttc::ConnectionError>(publish.payload);
                }
            }
        }
    })
    .await
    .context("VelaEdge Runtime command reply timed out")??;

    let reply: serde_json::Value =
        serde_json::from_slice(&reply).context("Runtime reply is not valid JSON")?;
    println!("{}", serde_json::to_string_pretty(&reply)?);
    if reply.get("status").and_then(|value| value.as_str()) != Some("succeeded") {
        bail!("Runtime reported a failed command");
    }
    Ok(())
}

fn qos(value: u8) -> QoS {
    match value {
        0 => QoS::AtMostOnce,
        1 => QoS::AtLeastOnce,
        _ => QoS::ExactlyOnce,
    }
}
