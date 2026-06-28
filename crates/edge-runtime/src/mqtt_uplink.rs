use std::time::Duration;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use edge_core::{EdgeConfigPackage, MqttUplinkConfig, TelemetrySample};
use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS, Transport};
use serde::Serialize;
use tokio::task::JoinHandle;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MqttPublishMessage {
    pub sink_id: String,
    pub broker: String,
    pub client_id: String,
    pub topic: String,
    pub qos: u8,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MqttBrokerTarget {
    pub host: String,
    pub port: u16,
    pub tls: bool,
}

#[derive(Clone, Debug, Serialize)]
struct MqttTelemetryPayload<'a> {
    edge_id: &'a str,
    config_version: &'a str,
    device_id: &'a str,
    telemetry_id: &'a str,
    value: &'a edge_core::TelemetryValue,
    quality: edge_core::DataQuality,
    timestamp: chrono::DateTime<chrono::Utc>,
}

pub struct RumqttcMqttPublisher {
    client: AsyncClient,
    _eventloop_task: JoinHandle<()>,
}

impl RumqttcMqttPublisher {
    pub fn connect_from_uplink(uplink: &MqttUplinkConfig) -> Result<Self> {
        validate_uplink(uplink)?;
        let target = parse_mqtt_broker_target(&uplink.broker)?;
        let mut options = MqttOptions::new(&uplink.client_id, target.host, target.port);
        options.set_keep_alive(Duration::from_secs(30));
        if target.tls {
            options.set_transport(Transport::tls_with_default_config());
        }

        let (client, eventloop) = AsyncClient::new(options, uplink.batch_size.max(1) as usize);
        Ok(Self {
            client,
            _eventloop_task: spawn_eventloop(eventloop),
        })
    }
}

#[async_trait]
impl MqttPublisher for RumqttcMqttPublisher {
    async fn publish(&mut self, message: MqttPublishMessage) -> Result<()> {
        self.client
            .publish(
                message.topic,
                rumqttc_qos(message.qos)?,
                false,
                message.payload,
            )
            .await
            .context("enqueue mqtt publish")?;
        Ok(())
    }
}

#[async_trait]
pub trait MqttPublisher: Send {
    async fn publish(&mut self, message: MqttPublishMessage) -> Result<()>;
}

#[derive(Default, Debug)]
pub struct RecordingMqttPublisher {
    messages: Vec<MqttPublishMessage>,
}

impl RecordingMqttPublisher {
    pub fn messages(&self) -> &[MqttPublishMessage] {
        &self.messages
    }
}

#[async_trait]
impl MqttPublisher for RecordingMqttPublisher {
    async fn publish(&mut self, message: MqttPublishMessage) -> Result<()> {
        self.messages.push(message);
        Ok(())
    }
}

pub fn build_mqtt_publish_messages(
    package: &EdgeConfigPackage,
    samples: &[TelemetrySample],
) -> Result<Vec<MqttPublishMessage>> {
    let mut messages = Vec::new();
    for uplink in &package.mqtt_uplinks {
        validate_uplink(uplink)?;
        for sample in samples {
            let payload = MqttTelemetryPayload {
                edge_id: &package.edge_id,
                config_version: &package.version,
                device_id: &sample.device_id,
                telemetry_id: &sample.telemetry_id,
                value: &sample.value,
                quality: sample.quality,
                timestamp: sample.timestamp,
            };
            messages.push(MqttPublishMessage {
                sink_id: uplink.sink_id.clone(),
                broker: uplink.broker.clone(),
                client_id: uplink.client_id.clone(),
                topic: render_topic(uplink, package, sample),
                qos: uplink.qos,
                payload: serde_json::to_vec(&payload)?,
            });
        }
    }
    Ok(messages)
}

pub async fn publish_mqtt_samples<P>(
    package: &EdgeConfigPackage,
    samples: &[TelemetrySample],
    publisher: &mut P,
) -> Result<usize>
where
    P: MqttPublisher,
{
    let messages = build_mqtt_publish_messages(package, samples)?;
    let messages_published = messages.len();
    for message in messages {
        publisher.publish(message).await?;
    }
    Ok(messages_published)
}

pub fn parse_mqtt_broker_target(broker: &str) -> Result<MqttBrokerTarget> {
    let broker = broker.trim();
    let Some((scheme, rest)) = broker.split_once("://") else {
        bail!("mqtt broker must include a scheme such as mqtt:// or mqtts://");
    };

    let (tls, default_port) = match scheme.to_ascii_lowercase().as_str() {
        "mqtt" | "tcp" => (false, 1883),
        "mqtts" | "ssl" => (true, 8883),
        _ => bail!("unsupported mqtt broker scheme: {scheme}"),
    };

    let authority = rest.split('/').next().unwrap_or_default();
    if authority.is_empty() {
        bail!("mqtt broker host is required");
    }
    if authority.contains('@') {
        bail!("mqtt broker credentials must be configured separately");
    }

    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && !port.is_empty() => {
            let port = port
                .parse::<u16>()
                .with_context(|| format!("invalid mqtt broker port: {port}"))?;
            (host.to_string(), port)
        }
        _ => (authority.to_string(), default_port),
    };

    Ok(MqttBrokerTarget { host, port, tls })
}

fn validate_uplink(uplink: &MqttUplinkConfig) -> Result<()> {
    if uplink.sink_id.trim().is_empty() {
        bail!("mqtt uplink sink id is required");
    }
    if uplink.broker.trim().is_empty() {
        bail!("mqtt uplink broker is required");
    }
    if uplink.client_id.trim().is_empty() {
        bail!("mqtt uplink client id is required");
    }
    if uplink.qos > 2 {
        bail!("mqtt uplink qos must be 0, 1, or 2");
    }
    Ok(())
}

fn rumqttc_qos(qos: u8) -> Result<QoS> {
    match qos {
        0 => Ok(QoS::AtMostOnce),
        1 => Ok(QoS::AtLeastOnce),
        2 => Ok(QoS::ExactlyOnce),
        _ => bail!("mqtt uplink qos must be 0, 1, or 2"),
    }
}

fn spawn_eventloop(mut eventloop: EventLoop) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if let Err(error) = eventloop.poll().await {
                tracing::warn!(?error, "mqtt eventloop poll failed");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    })
}

fn render_topic(
    uplink: &MqttUplinkConfig,
    package: &EdgeConfigPackage,
    sample: &TelemetrySample,
) -> String {
    uplink
        .topic_template
        .replace("{edge_id}", &package.edge_id)
        .replace("{device_id}", &sample.device_id)
        .replace("{telemetry_id}", &sample.telemetry_id)
}
