use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use edge_core::{EdgeConfigPackage, MqttProtocolVersion, MqttUplinkConfig};
use rumqttc::v5::{
    mqttbytes::{
        v5::{Packet as PacketV5, SubscribeReasonCode as SubscribeReasonCodeV5},
        QoS as QoSV5,
    },
    AsyncClient as AsyncClientV5, Event as EventV5, EventLoop as EventLoopV5,
    MqttOptions as MqttOptionsV5,
};
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, SubscribeReasonCode};
use sha2::{Digest, Sha256};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::Instant,
};

use crate::{
    interoperability_acceptance::{BrokerConsumerReceipt, BrokerConsumerRouteReceipt},
    mqtt_uplink::{
        configure_mqtt_options, configure_mqtt_v5_options, configured_data_mqtt_output_routes,
        parse_mqtt_broker_target, rumqttc_qos, rumqttc_v5_qos,
    },
};

#[derive(Clone, Debug)]
pub struct MqttFieldReceiptOptions {
    pub package: EdgeConfigPackage,
    pub package_sha256: String,
    pub duration: Duration,
    pub startup_timeout: Duration,
}

impl MqttFieldReceiptOptions {
    pub fn new(package: EdgeConfigPackage, package_sha256: impl Into<String>) -> Self {
        Self {
            package,
            package_sha256: package_sha256.into(),
            duration: Duration::from_secs(86_460),
            startup_timeout: Duration::from_secs(30),
        }
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    pub fn with_startup_timeout(mut self, startup_timeout: Duration) -> Self {
        self.startup_timeout = startup_timeout;
        self
    }
}

#[derive(Clone, Debug)]
struct MqttReceiptRoutePlan {
    sink_id: String,
    broker: String,
    consumer_id: String,
    uplink: MqttUplinkConfig,
    topics: BTreeMap<String, u8>,
}

#[derive(Debug)]
enum MqttReceiptWorkerSignal {
    Ready(usize),
    Error(usize, String),
}

#[derive(Debug)]
struct MqttReceiptDelivery {
    route_index: usize,
    topic: String,
    payload: Vec<u8>,
    duplicate: bool,
    retained: bool,
}

struct MqttReceiptWorkers(Vec<JoinHandle<()>>);

impl Drop for MqttReceiptWorkers {
    fn drop(&mut self) {
        for worker in &self.0 {
            worker.abort();
        }
    }
}

enum MqttReceiptCaptureStop {
    Duration(Duration),
    Shutdown(oneshot::Receiver<()>),
}

/// A broker-side receipt capture whose evidence window is controlled by the caller.
///
/// Field campaign orchestration waits for [`Self::wait_ready`] before starting
/// the Runtime and calls [`Self::finish`] only after Runtime publishing has
/// stopped. Dropping the session aborts its background task.
pub struct MqttFieldReceiptSession {
    ready: Option<oneshot::Receiver<()>>,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<Result<BrokerConsumerReceipt>>>,
}

impl MqttFieldReceiptSession {
    pub async fn wait_ready(&mut self) -> Result<()> {
        let mut ready = self
            .ready
            .take()
            .context("field MQTT receipt readiness was already consumed")?;
        let task = self
            .task
            .as_mut()
            .context("field MQTT receipt session is not running")?;
        enum WaitOutcome {
            Ready(Result<(), oneshot::error::RecvError>),
            Finished(Result<Result<BrokerConsumerReceipt>, tokio::task::JoinError>),
        }
        let outcome = tokio::select! {
            ready = &mut ready => WaitOutcome::Ready(ready),
            result = task => WaitOutcome::Finished(result),
        };
        match outcome {
            WaitOutcome::Ready(result) => result
                .context("field MQTT receipt capture stopped before subscriptions became ready"),
            WaitOutcome::Finished(result) => {
                self.task.take();
                result.context("field MQTT receipt task failed")??;
                bail!("field MQTT receipt capture ended before subscriptions became ready")
            }
        }
    }

    pub async fn finish(mut self) -> Result<BrokerConsumerReceipt> {
        let shutdown = self
            .shutdown
            .take()
            .context("field MQTT receipt session was already stopped")?;
        let _ = shutdown.send(());
        self.task
            .take()
            .context("field MQTT receipt session is not running")?
            .await
            .context("field MQTT receipt task failed")?
    }
}

impl Drop for MqttFieldReceiptSession {
    fn drop(&mut self) {
        if let Some(task) = self.task.as_ref() {
            task.abort();
        }
    }
}

#[derive(Debug)]
struct MqttReceiptRouteAccumulator {
    message_count: u64,
    expected_topics: BTreeSet<String>,
    observed_topics: BTreeSet<String>,
}

/// Captures broker-side delivery evidence for every enabled data-orchestration output.
///
/// The capture window starts only after every broker has acknowledged every exact
/// topic subscription. Retained messages, foreign edge/config payloads and MQTT
/// duplicate retransmissions are excluded from the receipt.
pub async fn capture_mqtt_field_receipt(
    options: MqttFieldReceiptOptions,
) -> Result<BrokerConsumerReceipt> {
    validate_options(&options)?;
    let plans = build_route_plans(&options.package)?;
    let duration = options.duration;
    run_mqtt_field_receipt_capture(
        options,
        plans,
        None,
        MqttReceiptCaptureStop::Duration(duration),
    )
    .await
}

/// Starts a receipt capture that is stopped explicitly by the caller.
///
/// This is intended for a field campaign that must establish all broker
/// subscriptions before starting the production Runtime.
pub fn start_mqtt_field_receipt_session(
    options: MqttFieldReceiptOptions,
) -> Result<MqttFieldReceiptSession> {
    validate_options(&options)?;
    let plans = build_route_plans(&options.package)?;
    let (ready_tx, ready_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(run_mqtt_field_receipt_capture(
        options,
        plans,
        Some(ready_tx),
        MqttReceiptCaptureStop::Shutdown(shutdown_rx),
    ));
    Ok(MqttFieldReceiptSession {
        ready: Some(ready_rx),
        shutdown: Some(shutdown_tx),
        task: Some(task),
    })
}

async fn run_mqtt_field_receipt_capture(
    options: MqttFieldReceiptOptions,
    plans: Vec<MqttReceiptRoutePlan>,
    ready: Option<oneshot::Sender<()>>,
    stop: MqttReceiptCaptureStop,
) -> Result<BrokerConsumerReceipt> {
    if plans.is_empty() {
        bail!("at least one enabled MQTT data output is required for field receipt capture");
    }

    let (signals_tx, mut signals_rx) = mpsc::unbounded_channel();
    let (deliveries_tx, mut deliveries_rx) = mpsc::unbounded_channel();
    let mut handles = Vec::with_capacity(plans.len());
    for (route_index, plan) in plans.iter().cloned().enumerate() {
        handles.push(spawn_receipt_worker(
            route_index,
            plan,
            signals_tx.clone(),
            deliveries_tx.clone(),
        )?);
    }
    drop(signals_tx);
    drop(deliveries_tx);
    let _workers = MqttReceiptWorkers(handles);

    wait_until_routes_ready(
        plans.len(),
        &plans,
        &mut signals_rx,
        options.startup_timeout,
    )
    .await?;
    while deliveries_rx.try_recv().is_ok() {}
    if let Some(ready) = ready {
        ready
            .send(())
            .map_err(|_| anyhow::anyhow!("field MQTT receipt readiness receiver was dropped"))?;
    }
    tracing::info!(
        route_count = plans.len(),
        duration_seconds = options.duration.as_secs(),
        "field MQTT receipt subscriptions are ready"
    );

    let mut accumulators = plans
        .iter()
        .map(|plan| MqttReceiptRouteAccumulator {
            message_count: 0,
            expected_topics: plan.topics.keys().cloned().collect(),
            observed_topics: BTreeSet::new(),
        })
        .collect::<Vec<_>>();
    let mut first_received_at = None;
    let mut last_received_at = None;
    let mut message_count = 0_u64;
    let mut delivery_digests = BTreeSet::new();
    let stop_wait = async move {
        match stop {
            MqttReceiptCaptureStop::Duration(duration) => {
                tokio::time::sleep_until(Instant::now() + duration).await;
                Ok(())
            }
            MqttReceiptCaptureStop::Shutdown(shutdown) => shutdown
                .await
                .context("field MQTT receipt controller stopped without finishing the session"),
        }
    };
    tokio::pin!(stop_wait);

    loop {
        tokio::select! {
            result = &mut stop_wait => {
                result?;
                break;
            },
            signal = signals_rx.recv() => {
                if let Some(MqttReceiptWorkerSignal::Error(route_index, error)) = signal {
                    tracing::warn!(
                        sink_id = plans[route_index].sink_id,
                        %error,
                        "field MQTT receipt route disconnected; rumqttc will reconnect"
                    );
                }
            }
            delivery = deliveries_rx.recv() => {
                let Some(delivery) = delivery else {
                    bail!("all field MQTT receipt workers stopped before the capture window ended");
                };
                if delivery.retained
                    || !payload_matches_package(
                        &delivery.payload,
                        &options.package.edge_id,
                        &options.package.version,
                    )
                {
                    continue;
                }
                let digest = delivery_digest(&delivery);
                if delivery.duplicate && delivery_digests.contains(&digest) {
                    continue;
                }
                delivery_digests.insert(digest);

                let now = Utc::now();
                first_received_at.get_or_insert(now);
                last_received_at = Some(now);
                message_count = message_count.saturating_add(1);
                let accumulator = &mut accumulators[delivery.route_index];
                accumulator.message_count = accumulator.message_count.saturating_add(1);
                accumulator.observed_topics.insert(delivery.topic);
            }
        }
    }

    if message_count == 0 {
        bail!("field MQTT receipt captured no live messages for the configured edge and version");
    }
    let mut missing_topics = Vec::new();
    for (index, accumulator) in accumulators.iter().enumerate() {
        for topic in accumulator
            .expected_topics
            .difference(&accumulator.observed_topics)
        {
            missing_topics.push(format!("{}:{topic}", plans[index].sink_id));
        }
    }
    if !missing_topics.is_empty() {
        bail!(
            "field MQTT receipt did not observe configured output topics: {}",
            missing_topics.join(", ")
        );
    }

    Ok(BrokerConsumerReceipt {
        schema_version: 1,
        edge_id: options.package.edge_id,
        config_version: options.package.version,
        package_sha256: options.package_sha256,
        first_received_at: first_received_at
            .context("field MQTT first receipt timestamp missing")?,
        last_received_at: last_received_at.context("field MQTT last receipt timestamp missing")?,
        message_count,
        routes: plans
            .into_iter()
            .zip(accumulators)
            .map(|(plan, accumulator)| BrokerConsumerRouteReceipt {
                broker: plan.broker,
                consumer_id: plan.consumer_id,
                message_count: accumulator.message_count,
                topics: accumulator.observed_topics.into_iter().collect(),
            })
            .collect(),
    })
}

fn validate_options(options: &MqttFieldReceiptOptions) -> Result<()> {
    if options.duration.is_zero() {
        bail!("field MQTT receipt duration must be greater than zero");
    }
    if options.startup_timeout.is_zero() {
        bail!("field MQTT receipt startup timeout must be greater than zero");
    }
    if options.package.edge_id.trim().is_empty() || options.package.version.trim().is_empty() {
        bail!("field MQTT receipt package requires edge id and config version");
    }
    if options.package_sha256.len() != 64
        || !options
            .package_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("field MQTT receipt package SHA-256 must contain 64 hexadecimal characters");
    }
    Ok(())
}

fn build_route_plans(package: &EdgeConfigPackage) -> Result<Vec<MqttReceiptRoutePlan>> {
    let configured_routes = configured_data_mqtt_output_routes(package)?;
    let mut topics_by_sink = BTreeMap::<String, BTreeMap<String, u8>>::new();
    for route in configured_routes {
        topics_by_sink
            .entry(route.sink_id)
            .or_default()
            .entry(route.topic)
            .and_modify(|qos| *qos = (*qos).max(route.qos))
            .or_insert(route.qos);
    }

    let capture_id = uuid::Uuid::new_v4().simple().to_string();
    topics_by_sink
        .into_iter()
        .enumerate()
        .map(|(index, (sink_id, topics))| {
            let source_uplink = package
                .mqtt_uplinks
                .iter()
                .find(|uplink| uplink.sink_id == sink_id)
                .with_context(|| format!("field MQTT sink not found: {sink_id}"))?;
            let consumer_id = format!(
                "vela-field-{}-{}-{index}",
                std::process::id(),
                &capture_id[..12]
            );
            let mut uplink = source_uplink.clone();
            uplink.client_id = consumer_id.clone();
            uplink.clean_session = true;
            uplink.clean_start = true;
            uplink.session_expiry_interval_seconds = 0;
            uplink.last_will = None;
            Ok(MqttReceiptRoutePlan {
                sink_id,
                broker: source_uplink.broker.clone(),
                consumer_id,
                uplink,
                topics,
            })
        })
        .collect()
}

fn spawn_receipt_worker(
    route_index: usize,
    plan: MqttReceiptRoutePlan,
    signals: mpsc::UnboundedSender<MqttReceiptWorkerSignal>,
    deliveries: mpsc::UnboundedSender<MqttReceiptDelivery>,
) -> Result<JoinHandle<()>> {
    let target = parse_mqtt_broker_target(&plan.broker)?;
    match plan.uplink.protocol_version {
        MqttProtocolVersion::V3_1_1 => {
            let mut options = MqttOptions::new(&plan.consumer_id, target.host.clone(), target.port);
            options.set_keep_alive(Duration::from_secs(plan.uplink.keep_alive_seconds.into()));
            options.set_clean_session(true);
            configure_mqtt_options(&mut options, &plan.uplink, target.tls)?;
            let (client, eventloop) = AsyncClient::new(options, 256);
            Ok(tokio::spawn(run_v3_receipt_worker(
                route_index,
                plan,
                client,
                eventloop,
                signals,
                deliveries,
            )))
        }
        MqttProtocolVersion::V5_0 => {
            let mut options = MqttOptionsV5::new(&plan.consumer_id, target.host, target.port);
            options.set_keep_alive(Duration::from_secs(plan.uplink.keep_alive_seconds.into()));
            options.set_clean_start(true);
            options.set_session_expiry_interval(None);
            configure_mqtt_v5_options(&mut options, &plan.uplink, target.tls)?;
            let (client, eventloop) = AsyncClientV5::new(options, 256);
            Ok(tokio::spawn(run_v5_receipt_worker(
                route_index,
                plan,
                client,
                eventloop,
                signals,
                deliveries,
            )))
        }
    }
}

async fn run_v3_receipt_worker(
    route_index: usize,
    plan: MqttReceiptRoutePlan,
    client: AsyncClient,
    mut eventloop: EventLoop,
    signals: mpsc::UnboundedSender<MqttReceiptWorkerSignal>,
    deliveries: mpsc::UnboundedSender<MqttReceiptDelivery>,
) {
    let mut pending_subacks = 0_usize;
    let mut announced_ready = false;
    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                pending_subacks = plan.topics.len();
                for (topic, qos) in &plan.topics {
                    let qos = match rumqttc_qos(*qos) {
                        Ok(qos) => qos,
                        Err(error) => {
                            send_worker_error(&signals, route_index, error);
                            continue;
                        }
                    };
                    if let Err(error) = client.subscribe(topic, qos).await {
                        send_worker_error(&signals, route_index, error);
                    }
                }
            }
            Ok(Event::Incoming(Packet::SubAck(suback))) => {
                let accepted = !suback.return_codes.is_empty()
                    && suback
                        .return_codes
                        .iter()
                        .all(|code| matches!(code, SubscribeReasonCode::Success(_)));
                if !accepted {
                    send_worker_error(
                        &signals,
                        route_index,
                        anyhow::anyhow!("broker rejected MQTT 3.1.1 field subscription"),
                    );
                    continue;
                }
                pending_subacks = pending_subacks.saturating_sub(1);
                if pending_subacks == 0 && !announced_ready {
                    announced_ready = true;
                    if signals
                        .send(MqttReceiptWorkerSignal::Ready(route_index))
                        .is_err()
                    {
                        break;
                    }
                }
            }
            Ok(Event::Incoming(Packet::Publish(publish))) => {
                if plan.topics.contains_key(&publish.topic)
                    && deliveries
                        .send(MqttReceiptDelivery {
                            route_index,
                            topic: publish.topic,
                            payload: publish.payload.to_vec(),
                            duplicate: publish.dup,
                            retained: publish.retain,
                        })
                        .is_err()
                {
                    break;
                }
            }
            Ok(_) => {}
            Err(error) => {
                send_worker_error(&signals, route_index, error);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

async fn run_v5_receipt_worker(
    route_index: usize,
    plan: MqttReceiptRoutePlan,
    client: AsyncClientV5,
    mut eventloop: EventLoopV5,
    signals: mpsc::UnboundedSender<MqttReceiptWorkerSignal>,
    deliveries: mpsc::UnboundedSender<MqttReceiptDelivery>,
) {
    let mut pending_subacks = 0_usize;
    let mut announced_ready = false;
    loop {
        match eventloop.poll().await {
            Ok(EventV5::Incoming(PacketV5::ConnAck(_))) => {
                pending_subacks = plan.topics.len();
                for (topic, qos) in &plan.topics {
                    let qos = match rumqttc_v5_qos(*qos) {
                        Ok(qos) => qos,
                        Err(error) => {
                            send_worker_error(&signals, route_index, error);
                            continue;
                        }
                    };
                    if let Err(error) = client.subscribe(topic, qos).await {
                        send_worker_error(&signals, route_index, error);
                    }
                }
            }
            Ok(EventV5::Incoming(PacketV5::SubAck(suback))) => {
                let accepted = !suback.return_codes.is_empty()
                    && suback.return_codes.iter().all(|code| {
                        matches!(
                            code,
                            SubscribeReasonCodeV5::Success(
                                QoSV5::AtMostOnce | QoSV5::AtLeastOnce | QoSV5::ExactlyOnce
                            )
                        )
                    });
                if !accepted {
                    send_worker_error(
                        &signals,
                        route_index,
                        anyhow::anyhow!("broker rejected MQTT 5.0 field subscription"),
                    );
                    continue;
                }
                pending_subacks = pending_subacks.saturating_sub(1);
                if pending_subacks == 0 && !announced_ready {
                    announced_ready = true;
                    if signals
                        .send(MqttReceiptWorkerSignal::Ready(route_index))
                        .is_err()
                    {
                        break;
                    }
                }
            }
            Ok(EventV5::Incoming(PacketV5::Publish(publish))) => {
                let Ok(topic) = std::str::from_utf8(&publish.topic) else {
                    continue;
                };
                if plan.topics.contains_key(topic)
                    && deliveries
                        .send(MqttReceiptDelivery {
                            route_index,
                            topic: topic.to_string(),
                            payload: publish.payload.to_vec(),
                            duplicate: publish.dup,
                            retained: publish.retain,
                        })
                        .is_err()
                {
                    break;
                }
            }
            Ok(_) => {}
            Err(error) => {
                send_worker_error(&signals, route_index, error);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

fn send_worker_error(
    signals: &mpsc::UnboundedSender<MqttReceiptWorkerSignal>,
    route_index: usize,
    error: impl std::fmt::Display,
) {
    let _ = signals.send(MqttReceiptWorkerSignal::Error(
        route_index,
        error.to_string(),
    ));
}

async fn wait_until_routes_ready(
    expected_route_count: usize,
    plans: &[MqttReceiptRoutePlan],
    signals: &mut mpsc::UnboundedReceiver<MqttReceiptWorkerSignal>,
    timeout: Duration,
) -> Result<()> {
    let mut ready = BTreeSet::new();
    let mut latest_errors = BTreeMap::new();
    let result = tokio::time::timeout(timeout, async {
        while ready.len() < expected_route_count {
            match signals.recv().await {
                Some(MqttReceiptWorkerSignal::Ready(route_index)) => {
                    ready.insert(route_index);
                    latest_errors.remove(&route_index);
                }
                Some(MqttReceiptWorkerSignal::Error(route_index, error)) => {
                    latest_errors.insert(route_index, error);
                }
                None => bail!("all field MQTT receipt workers stopped during startup"),
            }
        }
        Ok(())
    })
    .await;
    match result {
        Ok(result) => result,
        Err(_) => {
            let unresolved = plans
                .iter()
                .enumerate()
                .filter(|(index, _)| !ready.contains(index))
                .map(|(index, plan)| {
                    latest_errors
                        .get(&index)
                        .map(|error| format!("{} ({error})", plan.sink_id))
                        .unwrap_or_else(|| plan.sink_id.clone())
                })
                .collect::<Vec<_>>();
            bail!(
                "field MQTT receipt subscriptions were not ready within {} ms: {}",
                timeout.as_millis(),
                unresolved.join(", ")
            )
        }
    }
}

fn payload_matches_package(payload: &[u8], edge_id: &str, config_version: &str) -> bool {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(payload) else {
        return false;
    };
    json_string(&value, "edge_id", "edgeId") == Some(edge_id)
        && json_string(&value, "config_version", "configVersion") == Some(config_version)
}

fn json_string<'a>(value: &'a serde_json::Value, snake: &str, camel: &str) -> Option<&'a str> {
    value
        .get(snake)
        .or_else(|| value.get(camel))
        .and_then(serde_json::Value::as_str)
}

fn delivery_digest(delivery: &MqttReceiptDelivery) -> String {
    let mut digest = Sha256::new();
    digest.update(delivery.route_index.to_be_bytes());
    digest.update(delivery.topic.as_bytes());
    digest.update(&delivery.payload);
    format!("{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use edge_core::{
        DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPublish, MqttUplinkConfig,
    };
    use sha2::{Digest, Sha256};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };

    #[test]
    fn payload_filter_accepts_only_the_bound_edge_and_version() {
        assert!(payload_matches_package(
            br#"{"edge_id":"edge-a","config_version":"v3"}"#,
            "edge-a",
            "v3"
        ));
        assert!(payload_matches_package(
            br#"{"edgeId":"edge-a","configVersion":"v3"}"#,
            "edge-a",
            "v3"
        ));
        assert!(!payload_matches_package(
            br#"{"edge_id":"edge-b","config_version":"v3"}"#,
            "edge-a",
            "v3"
        ));
        assert!(!payload_matches_package(b"not-json", "edge-a", "v3"));
    }

    #[tokio::test]
    async fn captures_v3_and_v5_live_deliveries_from_configured_topics() {
        for version in [MqttProtocolVersion::V3_1_1, MqttProtocolVersion::V5_0] {
            let (broker, broker_task) = spawn_subscription_broker(version).await;
            let mut uplink = MqttUplinkConfig::velamq("primary", broker.clone(), "runtime");
            uplink.protocol_version = version;
            let package = EdgeConfigPackage::new("edge-field", "v9")
                .with_mqtt_uplink(uplink)
                .with_data_config(DataConfig::new(
                    "telemetry",
                    "遥测",
                    "pump-1",
                    "modbus",
                    DataConfigCollection::new(1000),
                    DataConfigPublish::new(
                        "primary",
                        "field/{edge_id}/{device_id}/telemetry",
                        DataConfigPayload::object(),
                    ),
                ));
            let package_bytes = serde_json::to_vec(&package).unwrap();
            let receipt = capture_mqtt_field_receipt(
                MqttFieldReceiptOptions::new(
                    package,
                    format!("{:x}", Sha256::digest(&package_bytes)),
                )
                .with_duration(Duration::from_millis(80))
                .with_startup_timeout(Duration::from_secs(2)),
            )
            .await
            .unwrap();

            assert_eq!(receipt.message_count, 1);
            assert_eq!(receipt.routes.len(), 1);
            assert_eq!(receipt.routes[0].broker, broker);
            assert_eq!(
                receipt.routes[0].topics,
                vec!["field/edge-field/pump-1/telemetry"]
            );
            broker_task.await.unwrap();
        }
    }

    #[tokio::test]
    async fn controlled_session_reports_readiness_and_finishes_on_request() {
        let (broker, broker_task) = spawn_subscription_broker(MqttProtocolVersion::V3_1_1).await;
        let package = EdgeConfigPackage::new("edge-field", "v9")
            .with_mqtt_uplink(MqttUplinkConfig::velamq(
                "primary",
                broker.clone(),
                "runtime",
            ))
            .with_data_config(DataConfig::new(
                "telemetry",
                "遥测",
                "pump-1",
                "modbus",
                DataConfigCollection::new(1000),
                DataConfigPublish::new(
                    "primary",
                    "field/{edge_id}/{device_id}/telemetry",
                    DataConfigPayload::object(),
                ),
            ));
        let package_bytes = serde_json::to_vec(&package).unwrap();
        let mut session = start_mqtt_field_receipt_session(
            MqttFieldReceiptOptions::new(package, format!("{:x}", Sha256::digest(&package_bytes)))
                .with_startup_timeout(Duration::from_secs(2)),
        )
        .unwrap();

        tokio::time::timeout(Duration::from_secs(2), session.wait_ready())
            .await
            .unwrap()
            .unwrap();
        tokio::time::sleep(Duration::from_millis(30)).await;
        let receipt = session.finish().await.unwrap();

        assert_eq!(receipt.edge_id, "edge-field");
        assert_eq!(receipt.message_count, 1);
        assert_eq!(receipt.routes[0].broker, broker);
        broker_task.await.unwrap();
    }

    async fn spawn_subscription_broker(
        version: MqttProtocolVersion,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let broker = format!("mqtt://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let (connect_header, _) = read_packet(&mut stream).await;
            assert_eq!(connect_header >> 4, 1);
            match version {
                MqttProtocolVersion::V3_1_1 => {
                    stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap()
                }
                MqttProtocolVersion::V5_0 => stream
                    .write_all(&[0x20, 0x03, 0x00, 0x00, 0x00])
                    .await
                    .unwrap(),
            }
            let (subscribe_header, subscribe) = read_packet(&mut stream).await;
            assert_eq!(subscribe_header, 0x82);
            let packet_id = u16::from_be_bytes([subscribe[0], subscribe[1]]);
            let topic_offset = match version {
                MqttProtocolVersion::V3_1_1 => 2,
                MqttProtocolVersion::V5_0 => 3,
            };
            let topic_len = usize::from(u16::from_be_bytes([
                subscribe[topic_offset],
                subscribe[topic_offset + 1],
            ]));
            let topic =
                std::str::from_utf8(&subscribe[topic_offset + 2..topic_offset + 2 + topic_len])
                    .unwrap();
            match version {
                MqttProtocolVersion::V3_1_1 => {
                    stream
                        .write_all(&[0x90, 0x03, (packet_id >> 8) as u8, packet_id as u8, 0x01])
                        .await
                        .unwrap();
                }
                MqttProtocolVersion::V5_0 => {
                    stream
                        .write_all(&[
                            0x90,
                            0x04,
                            (packet_id >> 8) as u8,
                            packet_id as u8,
                            0x00,
                            0x01,
                        ])
                        .await
                        .unwrap();
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
            let payload =
                br#"{"edge_id":"edge-field","config_version":"v9","values":{"pressure":1.2}}"#;
            let mut publish = Vec::new();
            publish.extend_from_slice(&(topic.len() as u16).to_be_bytes());
            publish.extend_from_slice(topic.as_bytes());
            if version == MqttProtocolVersion::V5_0 {
                publish.push(0x00);
            }
            publish.extend_from_slice(payload);
            write_packet(&mut stream, 0x30, &publish).await;
            tokio::time::sleep(Duration::from_millis(120)).await;
        });
        (broker, task)
    }

    async fn read_packet(stream: &mut TcpStream) -> (u8, Vec<u8>) {
        let header = stream.read_u8().await.unwrap();
        let mut multiplier = 1usize;
        let mut remaining_len = 0usize;
        loop {
            let encoded = stream.read_u8().await.unwrap();
            remaining_len += usize::from(encoded & 0x7f) * multiplier;
            if encoded & 0x80 == 0 {
                break;
            }
            multiplier *= 128;
        }
        let mut body = vec![0; remaining_len];
        stream.read_exact(&mut body).await.unwrap();
        (header, body)
    }

    async fn write_packet(stream: &mut TcpStream, header: u8, body: &[u8]) {
        let mut packet = vec![header];
        let mut remaining = body.len();
        loop {
            let mut encoded = (remaining % 128) as u8;
            remaining /= 128;
            if remaining > 0 {
                encoded |= 0x80;
            }
            packet.push(encoded);
            if remaining == 0 {
                break;
            }
        }
        packet.extend_from_slice(body);
        stream.write_all(&packet).await.unwrap();
    }
}
