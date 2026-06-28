use anyhow::{bail, Result};
use async_trait::async_trait;
use edge_core::{EdgeConfigPackage, MqttUplinkConfig, TelemetrySample};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MqttPublishMessage {
    pub sink_id: String,
    pub broker: String,
    pub client_id: String,
    pub topic: String,
    pub qos: u8,
    pub payload: Vec<u8>,
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
