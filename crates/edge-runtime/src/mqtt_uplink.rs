use std::{collections::BTreeMap, collections::BTreeSet, fs, time::Duration};

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use edge_core::{
    AlgorithmSpec, DataConfig, DataConfigGraphNodeKind, DataConfigPayloadMode, DataConfigPoint,
    EdgeConfigPackage, MqttUplinkConfig, PointAddress, TelemetrySample, TelemetryType,
    TelemetryValue,
};
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Outgoing, Packet, QoS, Transport};
use serde::{Deserialize, Serialize};
use tokio::{sync::mpsc, task::JoinHandle};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct MqttPublishMessage {
    pub sink_id: String,
    pub broker: String,
    pub client_id: String,
    pub topic: String,
    pub qos: u8,
    pub payload: Vec<u8>,
}

use crate::RocksEdgeRuntimeStore;

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
    sink_id: String,
    broker: String,
    client_id: String,
    client: AsyncClient,
    broker_events: mpsc::UnboundedReceiver<MqttBrokerEvent>,
    acknowledgement_timeout: Duration,
    _eventloop_task: JoinHandle<()>,
}

#[derive(Debug)]
enum MqttBrokerEvent {
    PublishSent(u16),
    PublishAcknowledged(u16),
    PublishCompleted(u16),
    ConnectionError(String),
}

impl RumqttcMqttPublisher {
    pub fn connect_from_uplink(uplink: &MqttUplinkConfig) -> Result<Self> {
        Self::connect_from_uplink_with_ack_timeout(uplink, Duration::from_secs(10))
    }

    pub fn connect_from_uplink_with_ack_timeout(
        uplink: &MqttUplinkConfig,
        acknowledgement_timeout: Duration,
    ) -> Result<Self> {
        validate_uplink(uplink)?;
        if acknowledgement_timeout.is_zero() {
            bail!("mqtt acknowledgement timeout must be greater than zero");
        }
        let target = parse_mqtt_broker_target(&uplink.broker)?;
        let mut options = MqttOptions::new(&uplink.client_id, target.host, target.port);
        options.set_keep_alive(Duration::from_secs(30));
        configure_mqtt_options(&mut options, uplink, target.tls)?;

        let (client, eventloop) = AsyncClient::new(options, uplink.batch_size.max(1) as usize);
        let (eventloop_task, broker_events) = spawn_eventloop(eventloop);
        Ok(Self {
            sink_id: uplink.sink_id.clone(),
            broker: uplink.broker.clone(),
            client_id: uplink.client_id.clone(),
            client,
            broker_events,
            acknowledgement_timeout,
            _eventloop_task: eventloop_task,
        })
    }

    async fn await_broker_confirmation(&mut self, qos: u8) -> Result<()> {
        tokio::time::timeout(self.acknowledgement_timeout, async {
            let packet_id = loop {
                match self.next_broker_event().await? {
                    MqttBrokerEvent::PublishSent(packet_id) => break packet_id,
                    MqttBrokerEvent::ConnectionError(error) => bail!(error),
                    MqttBrokerEvent::PublishAcknowledged(_)
                    | MqttBrokerEvent::PublishCompleted(_) => {}
                }
            };

            if qos == 0 {
                return Ok(());
            }
            if packet_id == 0 {
                bail!("mqtt broker event did not assign a packet id for qos {qos}");
            }

            loop {
                match self.next_broker_event().await? {
                    MqttBrokerEvent::PublishAcknowledged(ack_id)
                        if qos == 1 && ack_id == packet_id =>
                    {
                        return Ok(())
                    }
                    MqttBrokerEvent::PublishCompleted(ack_id)
                        if qos == 2 && ack_id == packet_id =>
                    {
                        return Ok(())
                    }
                    MqttBrokerEvent::ConnectionError(error) => bail!(error),
                    _ => {}
                }
            }
        })
        .await
        .context("mqtt broker acknowledgement timed out")?
    }

    async fn next_broker_event(&mut self) -> Result<MqttBrokerEvent> {
        self.broker_events
            .recv()
            .await
            .context("mqtt eventloop stopped before broker acknowledgement")
    }
}

#[async_trait]
impl MqttPublisher for RumqttcMqttPublisher {
    async fn publish(&mut self, message: MqttPublishMessage) -> Result<()> {
        if message.sink_id != self.sink_id
            || message.broker != self.broker
            || message.client_id != self.client_id
        {
            bail!(
                "mqtt message route {} ({}, {}) does not match connected route {} ({}, {})",
                message.sink_id,
                message.broker,
                message.client_id,
                self.sink_id,
                self.broker,
                self.client_id
            );
        }
        let qos = message.qos;
        self.client
            .publish(
                message.topic,
                rumqttc_qos(message.qos)?,
                false,
                message.payload,
            )
            .await
            .context("enqueue mqtt publish")?;
        self.await_broker_confirmation(qos).await
    }
}

pub struct MultiBrokerMqttPublisher {
    publishers: BTreeMap<String, RumqttcMqttPublisher>,
}

impl MultiBrokerMqttPublisher {
    pub fn connect_from_uplinks(uplinks: &[MqttUplinkConfig]) -> Result<Self> {
        Self::connect_from_uplinks_with_ack_timeout(uplinks, Duration::from_secs(10))
    }

    pub fn connect_from_uplinks_with_ack_timeout(
        uplinks: &[MqttUplinkConfig],
        acknowledgement_timeout: Duration,
    ) -> Result<Self> {
        if uplinks.is_empty() {
            bail!("at least one mqtt uplink is required");
        }
        let mut publishers = BTreeMap::new();
        for uplink in uplinks {
            if publishers.contains_key(&uplink.sink_id) {
                bail!("duplicate mqtt sink id: {}", uplink.sink_id);
            }
            let publisher = RumqttcMqttPublisher::connect_from_uplink_with_ack_timeout(
                uplink,
                acknowledgement_timeout,
            )?;
            publishers.insert(uplink.sink_id.clone(), publisher);
        }
        Ok(Self { publishers })
    }
}

#[async_trait]
impl MqttPublisher for MultiBrokerMqttPublisher {
    async fn publish(&mut self, message: MqttPublishMessage) -> Result<()> {
        let publisher = self
            .publishers
            .get_mut(&message.sink_id)
            .with_context(|| format!("mqtt sink is not configured: {}", message.sink_id))?;
        publisher.publish(message).await
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

        for output in DataConfigGraphOutput::from_data_config(data_config) {
            let synthetic_points =
                algorithm_output_points(package, data_config, samples, &output.scope);
            let selected = data_config_selected_samples(
                data_config,
                samples,
                &synthetic_points,
                &output.scope,
            );

            if selected.is_empty() {
                continue;
            }

            messages.push(MqttPublishMessage {
                sink_id: uplink.sink_id.clone(),
                broker: uplink.broker.clone(),
                client_id: uplink.client_id.clone(),
                topic: render_data_config_topic_template(
                    package,
                    data_config,
                    &output.topic_template,
                ),
                qos: data_config.publish.qos,
                payload: build_data_config_payload(package, data_config, &selected)?,
            });
        }
    }
    Ok(messages)
}

fn data_config_selected_samples<'a>(
    data_config: &'a DataConfig,
    samples: &'a [TelemetrySample],
    synthetic_points: &'a [DataConfigPoint],
    graph_scope: &DataConfigGraphScope,
) -> Vec<(&'a DataConfigPoint, &'a TelemetrySample)> {
    data_config
        .points
        .iter()
        .chain(synthetic_points.iter())
        .filter(|point| graph_scope.allows_point(&point.point_id))
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
    graph_scope: &DataConfigGraphScope,
) -> Vec<DataConfigPoint> {
    let configured_point_ids = data_config
        .points
        .iter()
        .map(|point| point.point_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    package
        .algorithms
        .iter()
        .filter(|algorithm| {
            graph_scope.allows_algorithm(&algorithm.id)
                && data_config.algorithm_ids.contains(&algorithm.id)
        })
        .flat_map(|algorithm| {
            algorithm_outputs_for_samples(algorithm, samples, &configured_point_ids)
        })
        .collect()
}

#[derive(Debug, Default)]
struct DataConfigGraphScope {
    active: bool,
    algorithm_ids: BTreeSet<String>,
    point_ids: BTreeSet<String>,
}

#[derive(Debug)]
struct DataConfigGraphOutput {
    topic_template: String,
    scope: DataConfigGraphScope,
}

impl DataConfigGraphOutput {
    fn from_data_config(data_config: &DataConfig) -> Vec<Self> {
        if data_config.visual_graph.nodes.is_empty() || data_config.visual_graph.edges.is_empty() {
            return vec![Self::fallback(data_config)];
        }

        let outputs = data_config
            .visual_graph
            .nodes
            .iter()
            .filter(|node| node.kind == DataConfigGraphNodeKind::Mqtt)
            .map(|node| Self {
                topic_template: node
                    .ref_id
                    .as_deref()
                    .filter(|topic| !topic.trim().is_empty())
                    .unwrap_or(&data_config.publish.topic_template)
                    .to_string(),
                scope: DataConfigGraphScope::from_output(data_config, &node.node_id),
            })
            .collect::<Vec<_>>();

        if outputs.is_empty() {
            vec![Self::fallback(data_config)]
        } else {
            outputs
        }
    }

    fn fallback(data_config: &DataConfig) -> Self {
        Self {
            topic_template: data_config.publish.topic_template.clone(),
            scope: DataConfigGraphScope::default(),
        }
    }
}

impl DataConfigGraphScope {
    fn from_output(data_config: &DataConfig, output_node_id: &str) -> Self {
        let mut stack = data_config
            .visual_graph
            .edges
            .iter()
            .filter(|edge| edge.to == output_node_id)
            .map(|edge| edge.from.as_str())
            .collect::<Vec<_>>();
        let mut visited = BTreeSet::new();

        while let Some(node_id) = stack.pop() {
            if !visited.insert(node_id.to_string()) {
                continue;
            }
            for edge in data_config
                .visual_graph
                .edges
                .iter()
                .filter(|edge| edge.to == node_id)
            {
                stack.push(edge.from.as_str());
            }
        }

        let mut scope = Self {
            active: true,
            ..Self::default()
        };
        for node in data_config
            .visual_graph
            .nodes
            .iter()
            .filter(|node| visited.contains(&node.node_id))
        {
            match node.kind {
                DataConfigGraphNodeKind::Point => {
                    if let Some(point_id) = node.ref_id.as_deref() {
                        scope.point_ids.insert(point_id.to_string());
                    }
                }
                DataConfigGraphNodeKind::Algorithm | DataConfigGraphNodeKind::Json => {
                    if let Some(algorithm_id) = node.ref_id.as_deref() {
                        scope.algorithm_ids.insert(algorithm_id.to_string());
                    }
                }
                DataConfigGraphNodeKind::Mqtt => {}
            }
        }
        scope
    }

    fn allows_point(&self, point_id: &str) -> bool {
        !self.active || self.point_ids.contains(point_id)
    }

    fn allows_algorithm(&self, algorithm_id: &str) -> bool {
        !self.active || self.algorithm_ids.contains(algorithm_id)
    }
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

pub async fn flush_mqtt_outbox<P>(store: &RocksEdgeRuntimeStore, publisher: &mut P) -> Result<usize>
where
    P: MqttPublisher + ?Sized,
{
    let mut published = 0;
    for entry in store.pending_mqtt_messages(usize::MAX)? {
        if let Err(error) = publisher.publish(entry.message.clone()).await {
            store.mark_mqtt_message_failed(entry.sequence, &error.to_string())?;
            return Err(error).with_context(|| {
                format!(
                    "failed to publish queued mqtt message {} to {}",
                    entry.sequence, entry.message.topic
                )
            });
        }
        store.acknowledge_mqtt_message(entry.sequence)?;
        published += 1;
    }
    Ok(published)
}

pub async fn publish_mqtt_samples_with_outbox<P>(
    package: &EdgeConfigPackage,
    samples: &[TelemetrySample],
    store: &RocksEdgeRuntimeStore,
    publisher: &mut P,
) -> Result<usize>
where
    P: MqttPublisher + ?Sized,
{
    for message in build_mqtt_publish_messages(package, samples)? {
        store.enqueue_mqtt_message(message)?;
    }
    flush_mqtt_outbox(store, publisher).await
}

pub async fn publish_data_config_mqtt_samples_with_outbox<P>(
    package: &EdgeConfigPackage,
    samples: &[TelemetrySample],
    store: &RocksEdgeRuntimeStore,
    publisher: &mut P,
) -> Result<usize>
where
    P: MqttPublisher + ?Sized,
{
    for message in build_data_config_mqtt_publish_messages(package, samples)? {
        store.enqueue_mqtt_message(message)?;
    }
    flush_mqtt_outbox(store, publisher).await
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
    match (&uplink.username, &uplink.password_env) {
        (None, None) => {}
        (Some(username), Some(password_env))
            if !username.trim().is_empty() && !password_env.trim().is_empty() => {}
        _ => bail!("mqtt username and password environment reference must be configured together"),
    }
    if uplink
        .tls_ca_path
        .as_deref()
        .is_some_and(|path| path.trim().is_empty())
    {
        bail!("mqtt TLS CA path must not be empty");
    }
    validate_qos(uplink.qos)?;
    Ok(())
}

pub(crate) fn configure_mqtt_options(
    options: &mut MqttOptions,
    uplink: &MqttUplinkConfig,
    tls: bool,
) -> Result<()> {
    validate_uplink(uplink)?;
    if let (Some(username), Some(password_env)) = (&uplink.username, &uplink.password_env) {
        let password = std::env::var(password_env).with_context(|| {
            format!("mqtt password environment variable is not available: {password_env}")
        })?;
        options.set_credentials(username, password);
    }

    if let Some(ca_path) = uplink.tls_ca_path.as_deref() {
        if !tls {
            bail!("mqtt TLS CA path requires an mqtts:// broker");
        }
        let ca = fs::read(ca_path)
            .with_context(|| format!("read mqtt TLS CA certificate: {ca_path}"))?;
        if ca.is_empty() {
            bail!("mqtt TLS CA certificate is empty: {ca_path}");
        }
        let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
        options.set_transport(Transport::tls(ca, None, None));
    } else if tls {
        let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
        options.set_transport(Transport::tls_with_default_config());
    }
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

fn spawn_eventloop(
    mut eventloop: EventLoop,
) -> (JoinHandle<()>, mpsc::UnboundedReceiver<MqttBrokerEvent>) {
    let (events_tx, events_rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(Event::Outgoing(Outgoing::Publish(packet_id))) => {
                    if events_tx
                        .send(MqttBrokerEvent::PublishSent(packet_id))
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(Event::Incoming(Packet::PubAck(ack))) => {
                    if events_tx
                        .send(MqttBrokerEvent::PublishAcknowledged(ack.pkid))
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(Event::Incoming(Packet::PubComp(ack))) => {
                    if events_tx
                        .send(MqttBrokerEvent::PublishCompleted(ack.pkid))
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(?error, "mqtt eventloop poll failed");
                    if events_tx
                        .send(MqttBrokerEvent::ConnectionError(error.to_string()))
                        .is_err()
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    });
    (task, events_rx)
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

fn render_data_config_topic_template(
    package: &EdgeConfigPackage,
    data_config: &DataConfig,
    topic_template: &str,
) -> String {
    topic_template
        .replace("{edge_id}", &package.edge_id)
        .replace("{device_id}", &data_config.device_id)
        .replace("{config_id}", &data_config.config_id)
        .replace("{site}", "default")
}
