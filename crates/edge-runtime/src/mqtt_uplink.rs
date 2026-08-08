use std::{
    collections::BTreeMap,
    collections::BTreeSet,
    fs,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use edge_core::{
    AlgorithmSpec, CommandFlowConfig, DataConfig, DataConfigGraphNodeKind, DataConfigPayloadMode,
    DataConfigPoint, EdgeConfigPackage, MqttProtocolVersion, MqttRuntimeMetrics,
    MqttSinkRuntimeMetrics, MqttUplinkConfig, PointAddress, TelemetrySample, TelemetryType,
    TelemetryValue,
};
use rumqttc::v5::{
    mqttbytes::{
        v5::{LastWill as LastWillV5, LastWillProperties, Packet as PacketV5},
        QoS as QoSV5,
    },
    AsyncClient as AsyncClientV5, Event as EventV5, EventLoop as EventLoopV5,
    MqttOptions as MqttOptionsV5,
};
use rumqttc::{
    AsyncClient, Event, EventLoop, LastWill as LastWillV3, MqttOptions, Outgoing, Packet, QoS,
    Transport,
};
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfiguredMqttOutputRoute {
    pub sink_id: String,
    pub broker: String,
    pub topic: String,
    pub qos: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MqttCommandMessage {
    pub sink_id: String,
    pub topic: String,
    pub payload: Vec<u8>,
    pub flow_ids: Vec<String>,
}

pub struct MqttCommandSubscriber {
    messages: mpsc::UnboundedReceiver<MqttCommandMessage>,
    connected: Vec<Arc<AtomicBool>>,
    eventloop_tasks: Vec<JoinHandle<()>>,
}

impl Drop for MqttCommandSubscriber {
    fn drop(&mut self) {
        for task in &self.eventloop_tasks {
            task.abort();
        }
    }
}

impl MqttCommandSubscriber {
    pub async fn connect_from_package(package: &EdgeConfigPackage) -> Result<Self> {
        let mut flows_by_sink = BTreeMap::<String, Vec<CommandFlowConfig>>::new();
        for flow in package.command_flows.iter().filter(|flow| flow.enabled) {
            flows_by_sink
                .entry(flow.mqtt_connection_id.clone())
                .or_default()
                .push(flow.clone());
        }
        if flows_by_sink.is_empty() {
            bail!("at least one enabled command flow is required");
        }

        let (messages_tx, messages) = mpsc::unbounded_channel();
        let mut connected = Vec::new();
        let mut eventloop_tasks = Vec::new();

        for (sink_id, flows) in flows_by_sink {
            let uplink = package
                .mqtt_uplinks
                .iter()
                .find(|uplink| uplink.sink_id == sink_id)
                .with_context(|| format!("command MQTT connection not found: {sink_id}"))?;
            validate_uplink(uplink)?;
            let target = parse_mqtt_broker_target(&uplink.broker)?;
            let route_connected = Arc::new(AtomicBool::new(false));
            let subscriptions = command_subscriptions(&package.edge_id, &flows);
            let command_client_id = format!("{}-commands", uplink.client_id);

            match uplink.protocol_version {
                MqttProtocolVersion::V3_1_1 => {
                    let mut options = MqttOptions::new(command_client_id, target.host, target.port);
                    options.set_keep_alive(Duration::from_secs(uplink.keep_alive_seconds.into()));
                    options.set_clean_session(uplink.clean_session);
                    configure_mqtt_options(&mut options, uplink, target.tls)?;
                    let (client, eventloop) = AsyncClient::new(options, 100);
                    let task = spawn_command_eventloop(
                        eventloop,
                        client.clone(),
                        sink_id.clone(),
                        subscriptions.clone(),
                        messages_tx.clone(),
                        route_connected.clone(),
                    );
                    eventloop_tasks.push(task);
                }
                MqttProtocolVersion::V5_0 => {
                    let mut options =
                        MqttOptionsV5::new(command_client_id, target.host, target.port);
                    options.set_keep_alive(Duration::from_secs(uplink.keep_alive_seconds.into()));
                    options.set_clean_start(uplink.clean_start);
                    options.set_session_expiry_interval(
                        (uplink.session_expiry_interval_seconds > 0)
                            .then_some(uplink.session_expiry_interval_seconds),
                    );
                    configure_mqtt_v5_options(&mut options, uplink, target.tls)?;
                    let (client, eventloop) = AsyncClientV5::new(options, 100);
                    let task = spawn_v5_command_eventloop(
                        eventloop,
                        client.clone(),
                        sink_id.clone(),
                        subscriptions.clone(),
                        messages_tx.clone(),
                        route_connected.clone(),
                    );
                    eventloop_tasks.push(task);
                }
            }
            connected.push(route_connected);
        }

        Ok(Self {
            messages,
            connected,
            eventloop_tasks,
        })
    }

    pub async fn recv(&mut self) -> Option<MqttCommandMessage> {
        self.messages.recv().await
    }

    pub fn configured_connection_count(&self) -> usize {
        self.connected.len()
    }

    pub fn connected_connection_count(&self) -> usize {
        self.connected
            .iter()
            .filter(|connected| connected.load(Ordering::Relaxed))
            .count()
    }
}

fn command_subscriptions(edge_id: &str, flows: &[CommandFlowConfig]) -> Vec<(String, String, u8)> {
    flows
        .iter()
        .map(|flow| {
            (
                flow.flow_id.clone(),
                flow.subscribe_topic.replace("{edge_id}", edge_id),
                flow.qos,
            )
        })
        .collect()
}

async fn subscribe_v3_topics(
    client: &AsyncClient,
    subscriptions: &[(String, String, u8)],
) -> Result<()> {
    let mut topics = BTreeMap::<&str, u8>::new();
    for (_, topic, qos) in subscriptions {
        topics
            .entry(topic)
            .and_modify(|configured| *configured = (*configured).max(*qos))
            .or_insert(*qos);
    }
    for (topic, qos) in topics {
        client
            .subscribe(topic, rumqttc_qos(qos)?)
            .await
            .with_context(|| format!("subscribe MQTT command topic {topic}"))?;
    }
    Ok(())
}

async fn subscribe_v5_topics(
    client: &AsyncClientV5,
    subscriptions: &[(String, String, u8)],
) -> Result<()> {
    let mut topics = BTreeMap::<&str, u8>::new();
    for (_, topic, qos) in subscriptions {
        topics
            .entry(topic)
            .and_modify(|configured| *configured = (*configured).max(*qos))
            .or_insert(*qos);
    }
    for (topic, qos) in topics {
        client
            .subscribe(topic, rumqttc_v5_qos(qos)?)
            .await
            .with_context(|| format!("subscribe MQTT 5 command topic {topic}"))?;
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
struct MqttTelemetryPayload<'a> {
    edge_id: &'a str,
    config_version: &'a str,
    device_id: &'a str,
    telemetry_id: &'a str,
    value: &'a edge_core::TelemetryValue,
    quality: edge_core::DataQuality,
    quality_code: edge_core::DataQualityCode,
    timestamp: chrono::DateTime<chrono::Utc>,
}

pub struct RumqttcMqttPublisher {
    sink_id: String,
    broker: String,
    client_id: String,
    client: MqttClient,
    broker_events: mpsc::UnboundedReceiver<MqttBrokerEvent>,
    connected: Arc<AtomicBool>,
    acknowledgement_timeout: Duration,
    publish_success_count: u64,
    publish_failure_count: u64,
    published_bytes: u64,
    acknowledgement_latency_total_ms: u64,
    last_ack_latency_ms: Option<u64>,
    last_publish_at: Option<DateTime<Utc>>,
    last_topic: Option<String>,
    last_error: Option<String>,
    _eventloop_task: JoinHandle<()>,
}

enum MqttClient {
    V3_1_1(AsyncClient),
    V5_0(AsyncClientV5),
}

impl Drop for RumqttcMqttPublisher {
    fn drop(&mut self) {
        // A dropped JoinHandle detaches the task. Abort explicitly so a publisher
        // created for a short collection session cannot leave an MQTT socket and
        // reconnect loop alive behind it.
        self._eventloop_task.abort();
    }
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
        let connected = Arc::new(AtomicBool::new(false));
        let (client, eventloop_task, broker_events) = match uplink.protocol_version {
            MqttProtocolVersion::V3_1_1 => {
                let mut options = MqttOptions::new(&uplink.client_id, target.host, target.port);
                options.set_keep_alive(Duration::from_secs(uplink.keep_alive_seconds.into()));
                options.set_clean_session(uplink.clean_session);
                configure_mqtt_options(&mut options, uplink, target.tls)?;
                let (client, eventloop) = AsyncClient::new(options, 100);
                let (task, events) = spawn_eventloop(eventloop, connected.clone());
                (MqttClient::V3_1_1(client), task, events)
            }
            MqttProtocolVersion::V5_0 => {
                let mut options = MqttOptionsV5::new(&uplink.client_id, target.host, target.port);
                options.set_keep_alive(Duration::from_secs(uplink.keep_alive_seconds.into()));
                options.set_clean_start(uplink.clean_start);
                options.set_session_expiry_interval(
                    (uplink.session_expiry_interval_seconds > 0)
                        .then_some(uplink.session_expiry_interval_seconds),
                );
                configure_mqtt_v5_options(&mut options, uplink, target.tls)?;
                let (client, eventloop) = AsyncClientV5::new(options, 100);
                let (task, events) = spawn_v5_eventloop(eventloop, connected.clone());
                (MqttClient::V5_0(client), task, events)
            }
        };
        Ok(Self {
            sink_id: uplink.sink_id.clone(),
            broker: uplink.broker.clone(),
            client_id: uplink.client_id.clone(),
            client,
            broker_events,
            connected,
            acknowledgement_timeout,
            publish_success_count: 0,
            publish_failure_count: 0,
            published_bytes: 0,
            acknowledgement_latency_total_ms: 0,
            last_ack_latency_ms: None,
            last_publish_at: None,
            last_topic: None,
            last_error: None,
            _eventloop_task: eventloop_task,
        })
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn runtime_status(&self) -> MqttSinkRuntimeStatus {
        MqttSinkRuntimeStatus {
            sink_id: self.sink_id.clone(),
            broker: self.broker.clone(),
            client_id: self.client_id.clone(),
            connected: self.is_connected(),
            publish_success_count: self.publish_success_count,
            publish_failure_count: self.publish_failure_count,
            published_bytes: self.published_bytes,
            average_ack_latency_ms: self
                .acknowledgement_latency_total_ms
                .checked_div(self.publish_success_count)
                .unwrap_or(0),
            last_ack_latency_ms: self.last_ack_latency_ms,
            last_publish_at: self.last_publish_at,
            last_topic: self.last_topic.clone(),
            last_error: self.last_error.clone(),
        }
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
        let started = Instant::now();
        let payload_bytes = message.payload.len() as u64;
        let topic = message.topic.clone();
        self.last_topic = Some(topic);

        let result: Result<()> = async {
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
            match &self.client {
                MqttClient::V3_1_1(client) => {
                    client
                        .publish(
                            message.topic,
                            rumqttc_qos(message.qos)?,
                            false,
                            message.payload,
                        )
                        .await
                        .context("enqueue MQTT 3.1.1 publish")?;
                }
                MqttClient::V5_0(client) => {
                    client
                        .publish(
                            message.topic,
                            rumqttc_v5_qos(message.qos)?,
                            false,
                            message.payload,
                        )
                        .await
                        .context("enqueue MQTT 5.0 publish")?;
                }
            }
            self.await_broker_confirmation(qos).await
        }
        .await;

        let latency_ms = duration_millis(started.elapsed());
        match result {
            Ok(()) => {
                self.publish_success_count = self.publish_success_count.saturating_add(1);
                self.published_bytes = self.published_bytes.saturating_add(payload_bytes);
                self.acknowledgement_latency_total_ms = self
                    .acknowledgement_latency_total_ms
                    .saturating_add(latency_ms);
                self.last_ack_latency_ms = Some(latency_ms);
                self.last_publish_at = Some(Utc::now());
                self.last_error = None;
                Ok(())
            }
            Err(error) => {
                self.publish_failure_count = self.publish_failure_count.saturating_add(1);
                self.last_error = Some(error.to_string());
                Err(error)
            }
        }
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

    pub fn configured_sink_count(&self) -> usize {
        self.publishers.len()
    }

    pub fn connected_sink_count(&self) -> usize {
        self.publishers
            .values()
            .filter(|publisher| publisher.is_connected())
            .count()
    }

    pub fn runtime_statuses(&self) -> Vec<MqttSinkRuntimeStatus> {
        self.publishers
            .values()
            .map(RumqttcMqttPublisher::runtime_status)
            .collect()
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

pub struct PersistentMqttPublisher {
    uplinks: Vec<MqttUplinkConfig>,
    publisher: Option<MultiBrokerMqttPublisher>,
    acknowledgement_timeout: Duration,
    connection_generation: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct MqttSinkRuntimeStatus {
    pub sink_id: String,
    pub broker: String,
    pub client_id: String,
    pub connected: bool,
    pub publish_success_count: u64,
    pub publish_failure_count: u64,
    pub published_bytes: u64,
    pub average_ack_latency_ms: u64,
    pub last_ack_latency_ms: Option<u64>,
    pub last_publish_at: Option<DateTime<Utc>>,
    pub last_topic: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PersistentMqttStatus {
    pub configured_sink_count: usize,
    pub connected_sink_count: usize,
    pub connection_generation: u64,
    pub publish_success_count: u64,
    pub publish_failure_count: u64,
    pub published_bytes: u64,
    pub sinks: Vec<MqttSinkRuntimeStatus>,
}

impl PersistentMqttStatus {
    pub fn runtime_metrics(&self) -> MqttRuntimeMetrics {
        MqttRuntimeMetrics {
            configured_sink_count: self.configured_sink_count,
            connected_sink_count: self.connected_sink_count,
            connection_generation: self.connection_generation,
            publish_success_count: self.publish_success_count,
            publish_failure_count: self.publish_failure_count,
            published_bytes: self.published_bytes,
            sinks: self
                .sinks
                .iter()
                .map(|sink| MqttSinkRuntimeMetrics {
                    sink_id: sink.sink_id.clone(),
                    broker: sink.broker.clone(),
                    client_id: sink.client_id.clone(),
                    connected: sink.connected,
                    publish_success_count: sink.publish_success_count,
                    publish_failure_count: sink.publish_failure_count,
                    published_bytes: sink.published_bytes,
                    average_ack_latency_ms: sink.average_ack_latency_ms,
                    last_ack_latency_ms: sink.last_ack_latency_ms,
                    last_publish_at: sink.last_publish_at,
                    last_topic: sink.last_topic.clone(),
                    last_error: sink.last_error.clone(),
                })
                .collect(),
        }
    }
}

impl Default for PersistentMqttPublisher {
    fn default() -> Self {
        Self::new()
    }
}

impl PersistentMqttPublisher {
    pub fn new() -> Self {
        Self::with_ack_timeout(Duration::from_secs(10))
    }

    pub fn with_ack_timeout(acknowledgement_timeout: Duration) -> Self {
        Self {
            uplinks: Vec::new(),
            publisher: None,
            acknowledgement_timeout,
            connection_generation: 0,
        }
    }

    pub fn configure(
        &mut self,
        uplinks: &[MqttUplinkConfig],
    ) -> Result<Option<&mut MultiBrokerMqttPublisher>> {
        if uplinks.is_empty() {
            if self.publisher.is_some() {
                self.connection_generation = self.connection_generation.saturating_add(1);
            }
            self.publisher = None;
            self.uplinks.clear();
            return Ok(None);
        }

        if self.publisher.is_none() || self.uplinks != uplinks {
            let publisher = MultiBrokerMqttPublisher::connect_from_uplinks_with_ack_timeout(
                uplinks,
                self.acknowledgement_timeout,
            )?;
            self.publisher = Some(publisher);
            self.uplinks = uplinks.to_vec();
            self.connection_generation = self.connection_generation.saturating_add(1);
        }

        Ok(self.publisher.as_mut())
    }

    pub fn status(&self) -> PersistentMqttStatus {
        let (configured_sink_count, connected_sink_count, sinks) = self
            .publisher
            .as_ref()
            .map(|publisher| {
                (
                    publisher.configured_sink_count(),
                    publisher.connected_sink_count(),
                    publisher.runtime_statuses(),
                )
            })
            .unwrap_or_default();
        PersistentMqttStatus {
            configured_sink_count,
            connected_sink_count,
            connection_generation: self.connection_generation,
            publish_success_count: sinks.iter().map(|sink| sink.publish_success_count).sum(),
            publish_failure_count: sinks.iter().map(|sink| sink.publish_failure_count).sum(),
            published_bytes: sinks.iter().map(|sink| sink.published_bytes).sum(),
            sinks,
        }
    }
}

fn duration_millis(duration: Duration) -> u64 {
    if duration.is_zero() {
        0
    } else {
        duration.as_millis().max(1).min(u128::from(u64::MAX)) as u64
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
                quality_code: sample
                    .quality_code
                    .unwrap_or_else(|| edge_core::DataQualityCode::default_for(sample.quality)),
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
                payload: build_data_config_payload(
                    package,
                    data_config,
                    &selected,
                    &output.payload,
                )?,
            });
        }
    }
    Ok(messages)
}

/// Resolves every MQTT topic that enabled data orchestration can publish to.
///
/// The field acceptance consumer uses this contract so it subscribes to the
/// same expanded topics as the production publisher, including visual graphs
/// with multiple MQTT output nodes and packages with multiple sinks.
pub fn configured_data_mqtt_output_routes(
    package: &EdgeConfigPackage,
) -> Result<Vec<ConfiguredMqttOutputRoute>> {
    let mut routes = BTreeMap::<(String, String), ConfiguredMqttOutputRoute>::new();
    for data_config in package.data_configs.iter().filter(|config| config.enabled) {
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
            let topic =
                render_data_config_topic_template(package, data_config, &output.topic_template);
            if topic.trim().is_empty() {
                bail!(
                    "data config {} resolves an empty MQTT output topic",
                    data_config.config_id
                );
            }
            if topic.contains(['#', '+']) {
                bail!(
                    "data config {} MQTT output topic must not contain wildcards: {}",
                    data_config.config_id,
                    topic
                );
            }
            let key = (uplink.sink_id.clone(), topic.clone());
            routes
                .entry(key)
                .and_modify(|route| route.qos = route.qos.max(data_config.publish.qos))
                .or_insert_with(|| ConfiguredMqttOutputRoute {
                    sink_id: uplink.sink_id.clone(),
                    broker: uplink.broker.clone(),
                    topic,
                    qos: data_config.publish.qos,
                });
        }
    }
    Ok(routes.into_values().collect())
}

fn data_config_selected_samples<'a>(
    data_config: &'a DataConfig,
    samples: &'a [TelemetrySample],
    synthetic_points: &'a [DataConfigPoint],
    graph_scope: &DataConfigGraphScope,
) -> Vec<(&'a DataConfigPoint, &'a TelemetrySample)> {
    let configured_points = data_config
        .points
        .iter()
        .filter(|point| graph_scope.allows_point(&point.point_id));
    configured_points
        .chain(synthetic_points.iter())
        .filter_map(|point| {
            samples
                .iter()
                .filter(|sample| {
                    sample.device_id == data_config.device_id
                        && sample.telemetry_id == point.point_id
                })
                .max_by_key(|sample| sample.timestamp)
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
            algorithm_outputs_for_samples(algorithm, samples, &configured_point_ids, graph_scope)
        })
        .collect()
}

#[derive(Debug, Default)]
struct DataConfigGraphScope {
    active: bool,
    algorithm_ids: BTreeSet<String>,
    algorithm_output_names: BTreeMap<String, BTreeSet<String>>,
    point_ids: BTreeSet<String>,
}

#[derive(Debug)]
struct DataConfigGraphOutput {
    topic_template: String,
    scope: DataConfigGraphScope,
    payload: DataConfigGraphPayload,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DataConfigPayloadLayout {
    Business,
    Envelope,
}

#[derive(Clone, Copy, Debug)]
struct DataConfigGraphPayload {
    layout: DataConfigPayloadLayout,
    include_timestamp: bool,
    include_quality: bool,
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
            .map(|node| {
                let layout = node
                    .params
                    .get("payloadLayout")
                    .or_else(|| node.params.get("payload_layout"))
                    .and_then(serde_json::Value::as_str)
                    .map(|value| match value.trim().to_ascii_lowercase().as_str() {
                        "business" | "flat" | "plain" => DataConfigPayloadLayout::Business,
                        _ => DataConfigPayloadLayout::Envelope,
                    })
                    .unwrap_or(DataConfigPayloadLayout::Envelope);
                let include_timestamp = node
                    .params
                    .get("includeTimestamp")
                    .or_else(|| node.params.get("include_timestamp"))
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(matches!(layout, DataConfigPayloadLayout::Envelope));
                let include_quality = node
                    .params
                    .get("includeQuality")
                    .or_else(|| node.params.get("include_quality"))
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(data_config.publish.payload.include_quality);
                Self {
                    topic_template: node
                        .ref_id
                        .as_deref()
                        .filter(|topic| !topic.trim().is_empty())
                        .unwrap_or(&data_config.publish.topic_template)
                        .to_string(),
                    scope: DataConfigGraphScope::from_output(data_config, &node.node_id),
                    payload: DataConfigGraphPayload {
                        layout,
                        include_timestamp,
                        include_quality,
                    },
                }
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
            payload: DataConfigGraphPayload {
                layout: DataConfigPayloadLayout::Envelope,
                include_timestamp: true,
                include_quality: data_config.publish.payload.include_quality,
            },
        }
    }
}

impl DataConfigGraphScope {
    fn from_output(data_config: &DataConfig, output_node_id: &str) -> Self {
        let mut scope = Self {
            active: true,
            ..Self::default()
        };
        let nodes = data_config
            .visual_graph
            .nodes
            .iter()
            .map(|node| (node.node_id.as_str(), node))
            .collect::<BTreeMap<_, _>>();
        let mut stack = data_config
            .visual_graph
            .edges
            .iter()
            .filter(|edge| edge.to == output_node_id)
            .map(|edge| (edge.from.as_str(), edge.from_port.as_deref()))
            .collect::<Vec<_>>();
        let mut visited = BTreeSet::new();

        while let Some((node_id, output_port)) = stack.pop() {
            if !visited.insert((node_id.to_string(), output_port.map(str::to_string))) {
                continue;
            }
            let Some(node) = nodes.get(node_id) else {
                continue;
            };
            match node.kind {
                DataConfigGraphNodeKind::Point => {
                    if let Some(point_id) = node.ref_id.as_deref() {
                        scope.point_ids.insert(point_id.to_string());
                    }
                }
                DataConfigGraphNodeKind::Algorithm | DataConfigGraphNodeKind::Json => {
                    if let Some(algorithm_id) = node.ref_id.as_deref() {
                        if algorithm_id != "merge_points" {
                            scope.algorithm_ids.insert(algorithm_id.to_string());
                            if let Some(output_port) = output_port {
                                scope
                                    .algorithm_output_names
                                    .entry(algorithm_id.to_string())
                                    .or_default()
                                    .insert(output_port.to_string());
                            }
                            continue;
                        }
                    }
                }
                DataConfigGraphNodeKind::Mqtt => {}
            }
            for edge in data_config
                .visual_graph
                .edges
                .iter()
                .filter(|edge| edge.to == node_id)
            {
                stack.push((edge.from.as_str(), edge.from_port.as_deref()));
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

    fn allows_algorithm_output(&self, algorithm_id: &str, output_name: &str) -> bool {
        if !self.active {
            return true;
        }
        self.algorithm_output_names
            .get(algorithm_id)
            .map(|ports| ports.contains("output") || ports.contains(output_name))
            .unwrap_or(true)
    }
}

fn algorithm_outputs_for_samples(
    algorithm: &AlgorithmSpec,
    samples: &[TelemetrySample],
    configured_point_ids: &std::collections::BTreeSet<&str>,
    graph_scope: &DataConfigGraphScope,
) -> Vec<DataConfigPoint> {
    algorithm
        .dsl
        .outputs
        .iter()
        .filter(|output| graph_scope.allows_algorithm_output(&algorithm.id, &output.name))
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
                            modbus: None,
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
    let _flush_guard = store.lock_mqtt_outbox_flush().await;
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

/// Validates the local environment required to establish one production MQTT connection without
/// opening a socket.
///
/// This covers the same broker URI, credential environment reference, and custom CA inputs used by
/// the MQTT 3.1.1 and MQTT 5.0 publishers. Field preflight tooling calls it before starting a
/// physical evidence window.
pub fn validate_mqtt_uplink_runtime_environment(uplink: &MqttUplinkConfig) -> Result<()> {
    let target = parse_mqtt_broker_target(&uplink.broker)?;
    validate_uplink_environment(uplink, target.tls)
}

/// Validates the persisted MQTT contract without reading credentials or CA
/// files from the current process environment. Deployment preflight should use
/// `validate_mqtt_uplink_runtime_environment`; read-only status tooling uses
/// this variant so it does not need access to Runtime secrets.
pub fn validate_mqtt_uplink_config(uplink: &MqttUplinkConfig) -> Result<()> {
    let target = parse_mqtt_broker_target(&uplink.broker)?;
    validate_uplink(uplink)?;
    if uplink.tls_ca_path.is_some() && !target.tls {
        bail!("mqtt TLS CA path requires an mqtts:// broker");
    }
    Ok(())
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
    if uplink.keep_alive_seconds < 5 {
        bail!("mqtt keep alive must be at least 5 seconds");
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
    if uplink.receive_maximum == Some(0) {
        bail!("mqtt receive maximum must be greater than zero");
    }
    if uplink.maximum_packet_size_bytes == Some(0) {
        bail!("mqtt maximum packet size must be greater than zero");
    }
    if uplink
        .user_properties
        .iter()
        .any(|property| property.key.trim().is_empty())
    {
        bail!("mqtt user property key must not be empty");
    }
    if let Some(will) = &uplink.last_will {
        if will.topic.trim().is_empty() {
            bail!("mqtt last will topic must not be empty");
        }
        validate_qos(will.qos)?;
        if will
            .user_properties
            .iter()
            .any(|property| property.key.trim().is_empty())
        {
            bail!("mqtt last will user property key must not be empty");
        }
    }
    validate_qos(uplink.qos)?;
    Ok(())
}

pub(crate) fn configure_mqtt_options(
    options: &mut MqttOptions,
    uplink: &MqttUplinkConfig,
    tls: bool,
) -> Result<()> {
    validate_uplink_environment(uplink, tls)?;
    if let (Some(username), Some(password_env)) = (&uplink.username, &uplink.password_env) {
        let password = std::env::var(password_env).with_context(|| {
            format!("mqtt password environment variable is not available: {password_env}")
        })?;
        options.set_credentials(username, password);
    }
    if let Some(will) = &uplink.last_will {
        options.set_last_will(LastWillV3::new(
            &will.topic,
            will.payload.as_bytes(),
            rumqttc_qos(will.qos)?,
            will.retain,
        ));
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

pub(crate) fn configure_mqtt_v5_options(
    options: &mut MqttOptionsV5,
    uplink: &MqttUplinkConfig,
    tls: bool,
) -> Result<()> {
    validate_uplink_environment(uplink, tls)?;
    if let (Some(username), Some(password_env)) = (&uplink.username, &uplink.password_env) {
        let password = std::env::var(password_env).with_context(|| {
            format!("mqtt password environment variable is not available: {password_env}")
        })?;
        options.set_credentials(username, password);
    }
    options
        .set_receive_maximum(uplink.receive_maximum)
        .set_max_packet_size(uplink.maximum_packet_size_bytes)
        .set_topic_alias_max(uplink.topic_alias_maximum)
        .set_request_response_info(Some(u8::from(uplink.request_response_information)))
        .set_request_problem_info(Some(u8::from(uplink.request_problem_information)))
        .set_user_properties(
            uplink
                .user_properties
                .iter()
                .map(|property| (property.key.clone(), property.value.clone()))
                .collect(),
        );
    if let Some(will) = &uplink.last_will {
        let properties = LastWillProperties {
            delay_interval: (will.delay_interval_seconds > 0)
                .then_some(will.delay_interval_seconds),
            payload_format_indicator: will.payload_format_utf8.then_some(1),
            message_expiry_interval: (will.message_expiry_interval_seconds > 0)
                .then_some(will.message_expiry_interval_seconds),
            content_type: will.content_type.clone(),
            response_topic: will.response_topic.clone(),
            correlation_data: will
                .correlation_data
                .as_ref()
                .map(|value| value.as_bytes().to_vec().into()),
            user_properties: will
                .user_properties
                .iter()
                .map(|property| (property.key.clone(), property.value.clone()))
                .collect(),
        };
        options.set_last_will(LastWillV5::new(
            &will.topic,
            will.payload.as_bytes(),
            rumqttc_v5_qos(will.qos)?,
            will.retain,
            Some(properties),
        ));
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

fn validate_uplink_environment(uplink: &MqttUplinkConfig, tls: bool) -> Result<()> {
    validate_uplink(uplink)?;
    if let Some(password_env) = uplink.password_env.as_deref() {
        std::env::var(password_env).with_context(|| {
            format!("mqtt password environment variable is not available: {password_env}")
        })?;
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
    }
    Ok(())
}

fn validate_qos(qos: u8) -> Result<()> {
    if qos > 2 {
        bail!("mqtt qos must be 0, 1, or 2");
    }
    Ok(())
}

pub(crate) fn rumqttc_qos(qos: u8) -> Result<QoS> {
    validate_qos(qos)?;
    match qos {
        0 => Ok(QoS::AtMostOnce),
        1 => Ok(QoS::AtLeastOnce),
        2 => Ok(QoS::ExactlyOnce),
        _ => bail!("mqtt uplink qos must be 0, 1, or 2"),
    }
}

pub(crate) fn rumqttc_v5_qos(qos: u8) -> Result<QoSV5> {
    validate_qos(qos)?;
    match qos {
        0 => Ok(QoSV5::AtMostOnce),
        1 => Ok(QoSV5::AtLeastOnce),
        2 => Ok(QoSV5::ExactlyOnce),
        _ => bail!("mqtt uplink qos must be 0, 1, or 2"),
    }
}

fn spawn_eventloop(
    mut eventloop: EventLoop,
    connected: Arc<AtomicBool>,
) -> (JoinHandle<()>, mpsc::UnboundedReceiver<MqttBrokerEvent>) {
    let (events_tx, events_rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    connected.store(true, Ordering::Relaxed);
                }
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
                    connected.store(false, Ordering::Relaxed);
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
        connected.store(false, Ordering::Relaxed);
    });
    (task, events_rx)
}

fn spawn_command_eventloop(
    mut eventloop: EventLoop,
    client: AsyncClient,
    sink_id: String,
    subscriptions: Vec<(String, String, u8)>,
    messages: mpsc::UnboundedSender<MqttCommandMessage>,
    connected: Arc<AtomicBool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    connected.store(true, Ordering::Relaxed);
                    if let Err(error) = subscribe_v3_topics(&client, &subscriptions).await {
                        tracing::warn!(sink_id, %error, "subscribe MQTT command topics failed");
                    }
                }
                Ok(Event::Incoming(Packet::Publish(publish))) => {
                    if !route_command_message(
                        &sink_id,
                        &subscriptions,
                        &publish.topic,
                        publish.payload.to_vec(),
                        &messages,
                    ) {
                        break;
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    connected.store(false, Ordering::Relaxed);
                    tracing::warn!(sink_id, ?error, "mqtt command eventloop poll failed");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
        connected.store(false, Ordering::Relaxed);
    })
}

fn spawn_v5_eventloop(
    mut eventloop: EventLoopV5,
    connected: Arc<AtomicBool>,
) -> (JoinHandle<()>, mpsc::UnboundedReceiver<MqttBrokerEvent>) {
    let (events_tx, events_rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(EventV5::Incoming(PacketV5::ConnAck(_))) => {
                    connected.store(true, Ordering::Relaxed);
                }
                Ok(EventV5::Outgoing(Outgoing::Publish(packet_id))) => {
                    if events_tx
                        .send(MqttBrokerEvent::PublishSent(packet_id))
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(EventV5::Incoming(PacketV5::PubAck(ack))) => {
                    if events_tx
                        .send(MqttBrokerEvent::PublishAcknowledged(ack.pkid))
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(EventV5::Incoming(PacketV5::PubComp(ack))) => {
                    if events_tx
                        .send(MqttBrokerEvent::PublishCompleted(ack.pkid))
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    connected.store(false, Ordering::Relaxed);
                    tracing::warn!(?error, "mqtt 5 eventloop poll failed");
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
        connected.store(false, Ordering::Relaxed);
    });
    (task, events_rx)
}

fn spawn_v5_command_eventloop(
    mut eventloop: EventLoopV5,
    client: AsyncClientV5,
    sink_id: String,
    subscriptions: Vec<(String, String, u8)>,
    messages: mpsc::UnboundedSender<MqttCommandMessage>,
    connected: Arc<AtomicBool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(EventV5::Incoming(PacketV5::ConnAck(_))) => {
                    connected.store(true, Ordering::Relaxed);
                    if let Err(error) = subscribe_v5_topics(&client, &subscriptions).await {
                        tracing::warn!(sink_id, %error, "subscribe MQTT 5 command topics failed");
                    }
                }
                Ok(EventV5::Incoming(PacketV5::Publish(publish))) => {
                    let Ok(topic) = std::str::from_utf8(&publish.topic) else {
                        tracing::warn!(sink_id, "ignored MQTT 5 command with non-UTF-8 topic");
                        continue;
                    };
                    if !route_command_message(
                        &sink_id,
                        &subscriptions,
                        topic,
                        publish.payload.to_vec(),
                        &messages,
                    ) {
                        break;
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    connected.store(false, Ordering::Relaxed);
                    tracing::warn!(sink_id, ?error, "mqtt 5 command eventloop poll failed");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
        connected.store(false, Ordering::Relaxed);
    })
}

fn route_command_message(
    sink_id: &str,
    subscriptions: &[(String, String, u8)],
    topic: &str,
    payload: Vec<u8>,
    messages: &mpsc::UnboundedSender<MqttCommandMessage>,
) -> bool {
    let flow_ids = subscriptions
        .iter()
        .filter(|(_, filter, _)| mqtt_topic_matches(filter, topic))
        .map(|(flow_id, _, _)| flow_id.clone())
        .collect::<Vec<_>>();
    if flow_ids.is_empty() {
        return true;
    }
    messages
        .send(MqttCommandMessage {
            sink_id: sink_id.to_string(),
            topic: topic.to_string(),
            payload,
            flow_ids,
        })
        .is_ok()
}

pub fn mqtt_topic_matches(filter: &str, topic: &str) -> bool {
    if filter.is_empty() || topic.is_empty() {
        return false;
    }
    if topic.starts_with('$') && matches!(filter.as_bytes().first(), Some(b'#' | b'+')) {
        return false;
    }

    let filter_levels = filter.split('/').collect::<Vec<_>>();
    let topic_levels = topic.split('/').collect::<Vec<_>>();
    let mut topic_index = 0;
    for (filter_index, level) in filter_levels.iter().enumerate() {
        match *level {
            "#" => return filter_index + 1 == filter_levels.len(),
            "+" => {
                if topic_index >= topic_levels.len() {
                    return false;
                }
            }
            literal => {
                if topic_levels.get(topic_index).copied() != Some(literal) {
                    return false;
                }
            }
        }
        topic_index += 1;
    }
    topic_index == topic_levels.len()
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
    graph_payload: &DataConfigGraphPayload,
) -> Result<Vec<u8>> {
    ensure_unique_json_fields(data_config, selected)?;

    let timestamp = selected
        .iter()
        .map(|(_, sample)| sample.timestamp)
        .max()
        .unwrap_or_default();

    let (values, quality, quality_code) = data_config_object_fields(selected);
    let value = match (graph_payload.layout, data_config.publish.payload.mode) {
        (DataConfigPayloadLayout::Business, DataConfigPayloadMode::Object) => {
            let mut payload = values;
            if graph_payload.include_timestamp {
                ensure_reserved_business_field_available(
                    data_config,
                    &payload,
                    &data_config.publish.payload.timestamp_field,
                )?;
                payload.insert(
                    data_config.publish.payload.timestamp_field.clone(),
                    serde_json::json!(timestamp),
                );
            }
            if graph_payload.include_quality {
                ensure_reserved_business_field_available(data_config, &payload, "_quality")?;
                ensure_reserved_business_field_available(data_config, &payload, "_quality_code")?;
                payload.insert("_quality".to_string(), serde_json::Value::Object(quality));
                payload.insert(
                    "_quality_code".to_string(),
                    serde_json::Value::Object(quality_code),
                );
            }
            serde_json::Value::Object(payload)
        }
        (DataConfigPayloadLayout::Business, DataConfigPayloadMode::Array) => {
            serde_json::Value::Array(data_config_array_items(
                selected,
                graph_payload.include_timestamp,
                graph_payload.include_quality,
                &data_config.publish.payload.timestamp_field,
            ))
        }
        (DataConfigPayloadLayout::Envelope, DataConfigPayloadMode::Object) => {
            let mut payload = data_config_envelope(package, data_config);
            if graph_payload.include_timestamp {
                payload.insert(
                    data_config.publish.payload.timestamp_field.clone(),
                    serde_json::json!(timestamp),
                );
            }
            payload.insert("values".to_string(), serde_json::Value::Object(values));
            if graph_payload.include_quality {
                payload.insert("quality".to_string(), serde_json::Value::Object(quality));
                payload.insert(
                    "quality_code".to_string(),
                    serde_json::Value::Object(quality_code),
                );
            }
            serde_json::Value::Object(payload)
        }
        (DataConfigPayloadLayout::Envelope, DataConfigPayloadMode::Array) => {
            let mut payload = data_config_envelope(package, data_config);
            if graph_payload.include_timestamp {
                payload.insert(
                    data_config.publish.payload.timestamp_field.clone(),
                    serde_json::json!(timestamp),
                );
            }
            payload.insert(
                "points".to_string(),
                serde_json::Value::Array(data_config_array_items(
                    selected,
                    false,
                    graph_payload.include_quality,
                    &data_config.publish.payload.timestamp_field,
                )),
            );
            serde_json::Value::Object(payload)
        }
    };

    Ok(serde_json::to_vec(&value)?)
}

fn data_config_envelope(
    package: &EdgeConfigPackage,
    data_config: &DataConfig,
) -> serde_json::Map<String, serde_json::Value> {
    serde_json::Map::from_iter([
        ("edge_id".to_string(), serde_json::json!(package.edge_id)),
        (
            "config_version".to_string(),
            serde_json::json!(package.version),
        ),
        (
            "config_id".to_string(),
            serde_json::json!(data_config.config_id),
        ),
        (
            "device_id".to_string(),
            serde_json::json!(data_config.device_id),
        ),
    ])
}

fn data_config_object_fields(
    selected: &[(&DataConfigPoint, &TelemetrySample)],
) -> (
    serde_json::Map<String, serde_json::Value>,
    serde_json::Map<String, serde_json::Value>,
    serde_json::Map<String, serde_json::Value>,
) {
    let mut values = serde_json::Map::new();
    let mut quality = serde_json::Map::new();
    let mut quality_code = serde_json::Map::new();
    for (point, sample) in selected {
        values.insert(
            point.json_field.clone(),
            telemetry_value_to_json(&sample.value),
        );
        quality.insert(
            point.json_field.clone(),
            serde_json::json!(quality_to_json_label(sample.quality)),
        );
        quality_code.insert(
            point.json_field.clone(),
            serde_json::json!(quality_code_to_json_label(sample)),
        );
    }
    (values, quality, quality_code)
}

fn data_config_array_items(
    selected: &[(&DataConfigPoint, &TelemetrySample)],
    include_timestamp: bool,
    include_quality: bool,
    timestamp_field: &str,
) -> Vec<serde_json::Value> {
    selected
        .iter()
        .map(|(point, sample)| {
            let mut item = serde_json::Map::new();
            item.insert("point_id".to_string(), serde_json::json!(point.point_id));
            item.insert("field".to_string(), serde_json::json!(point.json_field));
            item.insert("value".to_string(), telemetry_value_to_json(&sample.value));
            if include_timestamp {
                item.insert(
                    timestamp_field.to_string(),
                    serde_json::json!(sample.timestamp),
                );
            }
            if include_quality {
                item.insert(
                    "quality".to_string(),
                    serde_json::json!(quality_to_json_label(sample.quality)),
                );
                item.insert(
                    "quality_code".to_string(),
                    serde_json::json!(quality_code_to_json_label(sample)),
                );
            }
            serde_json::Value::Object(item)
        })
        .collect()
}

fn ensure_reserved_business_field_available(
    data_config: &DataConfig,
    values: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<()> {
    if values.contains_key(field) {
        bail!(
            "data config {} business payload field conflicts with reserved field {}",
            data_config.config_id,
            field
        );
    }
    Ok(())
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

fn quality_code_to_json_label(sample: &TelemetrySample) -> &'static str {
    sample
        .quality_code
        .unwrap_or_else(|| edge_core::DataQualityCode::default_for(sample.quality))
        .as_str()
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

#[cfg(test)]
mod persistent_publisher_tests {
    use super::*;
    use edge_core::{MqttLastWillConfig, MqttUserProperty};

    fn uplink(sink_id: &str, client_id: &str) -> MqttUplinkConfig {
        MqttUplinkConfig::velamq(sink_id, "mqtt://127.0.0.1:65530", client_id)
    }

    #[tokio::test]
    async fn unchanged_configuration_reuses_the_same_mqtt_generation() {
        let mut publisher = PersistentMqttPublisher::new();
        let config = vec![uplink("primary", "runtime-primary")];

        publisher.configure(&config).unwrap();
        assert_eq!(publisher.status().connection_generation, 1);
        assert_eq!(publisher.status().configured_sink_count, 1);

        publisher.configure(&config).unwrap();
        assert_eq!(publisher.status().connection_generation, 1);

        publisher
            .configure(&[uplink("secondary", "runtime-secondary")])
            .unwrap();
        assert_eq!(publisher.status().connection_generation, 2);

        publisher.configure(&[]).unwrap();
        assert_eq!(publisher.status().connection_generation, 3);
        assert_eq!(publisher.status().configured_sink_count, 0);
    }

    #[test]
    fn mqtt5_options_include_connect_properties_and_last_will() {
        let mut config = uplink("primary", "runtime-primary");
        config.protocol_version = MqttProtocolVersion::V5_0;
        config.receive_maximum = Some(32);
        config.maximum_packet_size_bytes = Some(1_048_576);
        config.topic_alias_maximum = Some(16);
        config.request_response_information = true;
        config.request_problem_information = false;
        config.user_properties = vec![MqttUserProperty {
            key: "tenant".to_string(),
            value: "factory-a".to_string(),
        }];
        config.last_will = Some(MqttLastWillConfig {
            topic: "edge/runtime-primary/status".to_string(),
            payload: r#"{"status":"offline"}"#.to_string(),
            qos: 1,
            retain: true,
            delay_interval_seconds: 10,
            payload_format_utf8: true,
            message_expiry_interval_seconds: 300,
            content_type: Some("application/json".to_string()),
            response_topic: Some("edge/runtime-primary/status/ack".to_string()),
            correlation_data: Some("runtime-primary".to_string()),
            user_properties: vec![MqttUserProperty {
                key: "reason".to_string(),
                value: "disconnect".to_string(),
            }],
        });

        let mut options = MqttOptionsV5::new("runtime-primary", "127.0.0.1", 1883);
        configure_mqtt_v5_options(&mut options, &config, false).unwrap();

        assert_eq!(options.receive_maximum(), Some(32));
        assert_eq!(options.max_packet_size(), Some(1_048_576));
        assert_eq!(options.topic_alias_max(), Some(16));
        assert_eq!(options.request_response_info(), Some(1));
        assert_eq!(options.request_problem_info(), Some(0));
        assert_eq!(
            options.user_properties(),
            vec![("tenant".to_string(), "factory-a".to_string())]
        );

        let will = options.last_will().expect("last will is configured");
        assert_eq!(will.topic.as_ref(), b"edge/runtime-primary/status");
        assert_eq!(will.message.as_ref(), br#"{"status":"offline"}"#);
        assert_eq!(will.qos, QoSV5::AtLeastOnce);
        assert!(will.retain);
        let properties = will.properties.expect("MQTT 5 will properties exist");
        assert_eq!(properties.delay_interval, Some(10));
        assert_eq!(properties.message_expiry_interval, Some(300));
        assert_eq!(properties.content_type.as_deref(), Some("application/json"));
        assert_eq!(
            properties.response_topic.as_deref(),
            Some("edge/runtime-primary/status/ack")
        );
    }
}
