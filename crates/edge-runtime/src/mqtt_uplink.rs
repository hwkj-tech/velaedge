use std::time::Duration;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use edge_core::{
    AlgorithmSpec, DataConfig, DataConfigPayloadMode, DataConfigPoint, EdgeConfigPackage,
    MqttUplinkConfig, PointAddress, TelemetrySample, TelemetryType, TelemetryValue,
};
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

pub fn build_data_config_mqtt_publish_messages(
    package: &EdgeConfigPackage,
    samples: &[TelemetrySample],
) -> Result<Vec<MqttPublishMessage>> {
    let mut messages = Vec::new();
    for data_config in &package.data_configs {
        if !data_config.enabled {
            continue;
        }

        let uplink = package
            .mqtt_uplinks
            .iter()
            .find(|uplink| uplink.sink_id == data_config.publish.sink_id)
            .with_context(|| {
                format!(
                    "mqtt sink not found for data config {}: {}",
                    data_config.config_id, data_config.publish.sink_id
                )
            })?;
        validate_uplink(uplink)?;
        validate_qos(data_config.publish.qos)?;

        let synthetic_points = algorithm_output_points(package, data_config, samples);
        let selected = data_config_selected_samples(data_config, samples, &synthetic_points);

        if selected.is_empty() {
            continue;
        }

        messages.push(MqttPublishMessage {
            sink_id: uplink.sink_id.clone(),
            broker: uplink.broker.clone(),
            client_id: uplink.client_id.clone(),
            topic: render_data_config_topic(package, data_config),
            qos: data_config.publish.qos,
            payload: build_data_config_payload(package, data_config, &selected)?,
        });
    }
    Ok(messages)
}

fn data_config_selected_samples<'a>(
    data_config: &'a DataConfig,
    samples: &'a [TelemetrySample],
    synthetic_points: &'a [DataConfigPoint],
) -> Vec<(&'a DataConfigPoint, &'a TelemetrySample)> {
    data_config
        .points
        .iter()
        .chain(synthetic_points.iter())
        .filter_map(|point| {
            samples
                .iter()
                .find(|sample| {
                    sample.device_id == data_config.device_id
                        && sample.telemetry_id == point.point_id
                })
                .map(|sample| (point, sample))
        })
        .collect()
}

fn algorithm_output_points(
    package: &EdgeConfigPackage,
    data_config: &DataConfig,
    samples: &[TelemetrySample],
) -> Vec<DataConfigPoint> {
    let configured_point_ids = data_config
        .points
        .iter()
        .map(|point| point.point_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    package
        .algorithms
        .iter()
        .filter(|algorithm| data_config.algorithm_ids.contains(&algorithm.id))
        .flat_map(|algorithm| {
            algorithm_outputs_for_samples(algorithm, samples, &configured_point_ids)
        })
        .collect()
}

fn algorithm_outputs_for_samples(
    algorithm: &AlgorithmSpec,
    samples: &[TelemetrySample],
    configured_point_ids: &std::collections::BTreeSet<&str>,
) -> Vec<DataConfigPoint> {
    algorithm
        .dsl
        .outputs
        .iter()
        .filter(|output| !configured_point_ids.contains(output.point_id.as_str()))
        .filter_map(|output| {
            samples
                .iter()
                .find(|sample| sample.telemetry_id == output.point_id)
                .map(|sample| {
                    DataConfigPoint::new(
                        output.point_id.clone(),
                        output.point_id.clone(),
                        PointAddress {
                            kind: "algorithm".to_string(),
                            value: algorithm.id.clone(),
                        },
                        telemetry_type_from_value(&sample.value),
                        if output.name.trim().is_empty() {
                            json_field_from_point_id(&output.point_id)
                        } else {
                            output.name.clone()
                        },
                    )
                })
        })
        .collect()
}

fn telemetry_type_from_value(value: &TelemetryValue) -> TelemetryType {
    match value {
        TelemetryValue::Float(_) => TelemetryType::Float,
        TelemetryValue::Integer(_) => TelemetryType::Integer,
        TelemetryValue::Boolean(_) => TelemetryType::Boolean,
        TelemetryValue::Text(_) => TelemetryType::Text,
    }
}

fn json_field_from_point_id(point_id: &str) -> String {
    point_id
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() {
                value
            } else {
                '_'
            }
        })
        .collect()
}

pub async fn publish_mqtt_samples<P>(
    package: &EdgeConfigPackage,
    samples: &[TelemetrySample],
    publisher: &mut P,
) -> Result<usize>
where
    P: MqttPublisher + ?Sized,
{
    let messages = build_mqtt_publish_messages(package, samples)?;
    let messages_published = messages.len();
    for message in messages {
        publisher.publish(message).await?;
    }
    Ok(messages_published)
}

pub async fn publish_data_config_mqtt_samples<P>(
    package: &EdgeConfigPackage,
    samples: &[TelemetrySample],
    publisher: &mut P,
) -> Result<usize>
where
    P: MqttPublisher + ?Sized,
{
    let messages = build_data_config_mqtt_publish_messages(package, samples)?;
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
    validate_qos(uplink.qos)?;
    Ok(())
}

fn validate_qos(qos: u8) -> Result<()> {
    if qos > 2 {
        bail!("mqtt qos must be 0, 1, or 2");
    }
    Ok(())
}

fn rumqttc_qos(qos: u8) -> Result<QoS> {
    validate_qos(qos)?;
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

fn build_data_config_payload(
    package: &EdgeConfigPackage,
    data_config: &DataConfig,
    selected: &[(&DataConfigPoint, &TelemetrySample)],
) -> Result<Vec<u8>> {
    ensure_unique_json_fields(data_config, selected)?;

    let timestamp = selected
        .iter()
        .map(|(_, sample)| sample.timestamp)
        .max()
        .unwrap_or_default();

    let mut payload = serde_json::Map::new();
    payload.insert("edge_id".to_string(), serde_json::json!(package.edge_id));
    payload.insert(
        "config_version".to_string(),
        serde_json::json!(package.version),
    );
    payload.insert(
        "config_id".to_string(),
        serde_json::json!(data_config.config_id),
    );
    payload.insert(
        "device_id".to_string(),
        serde_json::json!(data_config.device_id),
    );
    payload.insert(
        data_config.publish.payload.timestamp_field.clone(),
        serde_json::json!(timestamp),
    );

    match data_config.publish.payload.mode {
        DataConfigPayloadMode::Object => {
            let mut values = serde_json::Map::new();
            let mut quality = serde_json::Map::new();
            for (point, sample) in selected {
                values.insert(
                    point.json_field.clone(),
                    telemetry_value_to_json(&sample.value),
                );
                quality.insert(
                    point.json_field.clone(),
                    serde_json::json!(quality_to_json_label(sample.quality)),
                );
            }
            payload.insert("values".to_string(), serde_json::Value::Object(values));
            if data_config.publish.payload.include_quality {
                payload.insert("quality".to_string(), serde_json::Value::Object(quality));
            }
        }
        DataConfigPayloadMode::Array => {
            let points = selected
                .iter()
                .map(|(point, sample)| {
                    let mut item = serde_json::Map::new();
                    item.insert("point_id".to_string(), serde_json::json!(point.point_id));
                    item.insert("field".to_string(), serde_json::json!(point.json_field));
                    item.insert("value".to_string(), telemetry_value_to_json(&sample.value));
                    if data_config.publish.payload.include_quality {
                        item.insert(
                            "quality".to_string(),
                            serde_json::json!(quality_to_json_label(sample.quality)),
                        );
                    }
                    serde_json::Value::Object(item)
                })
                .collect::<Vec<_>>();
            payload.insert("points".to_string(), serde_json::Value::Array(points));
        }
    }

    Ok(serde_json::to_vec(&serde_json::Value::Object(payload))?)
}

fn ensure_unique_json_fields(
    data_config: &DataConfig,
    selected: &[(&DataConfigPoint, &TelemetrySample)],
) -> Result<()> {
    let mut fields = std::collections::BTreeSet::new();
    for (point, _) in selected {
        if !fields.insert(point.json_field.as_str()) {
            bail!(
                "data config {} has duplicate json field {}",
                data_config.config_id,
                point.json_field
            );
        }
    }
    Ok(())
}

fn telemetry_value_to_json(value: &TelemetryValue) -> serde_json::Value {
    match value {
        TelemetryValue::Float(value) => serde_json::json!(value),
        TelemetryValue::Integer(value) => serde_json::json!(value),
        TelemetryValue::Boolean(value) => serde_json::json!(value),
        TelemetryValue::Text(value) => serde_json::json!(value),
    }
}

fn quality_to_json_label(quality: edge_core::DataQuality) -> &'static str {
    match quality {
        edge_core::DataQuality::Good => "good",
        edge_core::DataQuality::Uncertain => "uncertain",
        edge_core::DataQuality::Bad => "bad",
    }
}

fn render_data_config_topic(package: &EdgeConfigPackage, data_config: &DataConfig) -> String {
    data_config
        .publish
        .topic_template
        .replace("{edge_id}", &package.edge_id)
        .replace("{device_id}", &data_config.device_id)
        .replace("{config_id}", &data_config.config_id)
        .replace("{site}", "default")
}
