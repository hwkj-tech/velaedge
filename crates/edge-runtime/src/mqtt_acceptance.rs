use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use edge_core::MqttUplinkConfig;
use rumqttc::{
    mqttbytes::v4::SubscribeReasonCode, AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS,
};
use serde::Serialize;

use crate::{
    mqtt_uplink::configure_mqtt_options, parse_mqtt_broker_target, MqttPublishMessage,
    MqttPublisher, RumqttcMqttPublisher,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MqttAcceptanceOptions {
    pub broker: String,
    pub client_id_prefix: String,
    pub topic: Option<String>,
    pub qos: u8,
    pub timeout: Duration,
    pub username: Option<String>,
    pub password_env: Option<String>,
    pub tls_ca_path: Option<String>,
}

impl MqttAcceptanceOptions {
    pub fn new(broker: impl Into<String>, client_id_prefix: impl Into<String>) -> Self {
        Self {
            broker: broker.into(),
            client_id_prefix: client_id_prefix.into(),
            topic: None,
            qos: 1,
            timeout: Duration::from_secs(10),
            username: None,
            password_env: None,
            tls_ca_path: None,
        }
    }

    pub fn with_topic(mut self, topic: impl Into<String>) -> Self {
        self.topic = Some(topic.into());
        self
    }

    pub fn with_qos(mut self, qos: u8) -> Self {
        self.qos = qos;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_credentials_env(
        mut self,
        username: impl Into<String>,
        password_env: impl Into<String>,
    ) -> Self {
        self.username = Some(username.into());
        self.password_env = Some(password_env.into());
        self
    }

    pub fn with_tls_ca_path(mut self, tls_ca_path: impl Into<String>) -> Self {
        self.tls_ca_path = Some(tls_ca_path.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MqttAcceptanceReport {
    pub broker: String,
    pub topic: String,
    pub qos: u8,
    pub publisher_client_id: String,
    pub subscriber_client_id: String,
    pub probe_id: String,
    pub sent_at: DateTime<Utc>,
    pub round_trip_ms: u128,
    pub received_bytes: usize,
    pub payload_verified: bool,
}

#[derive(Serialize)]
struct MqttAcceptanceProbe<'a> {
    kind: &'static str,
    probe_id: &'a str,
    sent_at: DateTime<Utc>,
}

/// Performs a broker-level round trip using the same publisher implementation as the runtime.
/// A successful return means the subscription was accepted, the publish received its QoS
/// confirmation, and the subscriber received the exact probe payload.
pub async fn run_mqtt_acceptance(options: MqttAcceptanceOptions) -> Result<MqttAcceptanceReport> {
    validate_options(&options)?;

    let now = Utc::now();
    let probe_id = format!(
        "{}-{}-{}",
        options.client_id_prefix,
        std::process::id(),
        now.timestamp_micros()
    );
    let topic = options
        .topic
        .clone()
        .unwrap_or_else(|| format!("edgeops/acceptance/{probe_id}"));
    validate_topic(&topic)?;

    let publisher_client_id = format!("{}-publisher", options.client_id_prefix);
    let subscriber_client_id = format!("{}-subscriber", options.client_id_prefix);
    if publisher_client_id == subscriber_client_id {
        bail!("mqtt acceptance publisher and subscriber client ids must be distinct");
    }

    let target = parse_mqtt_broker_target(&options.broker)?;
    let mut subscriber_options = MqttOptions::new(&subscriber_client_id, target.host, target.port);
    subscriber_options.set_keep_alive(Duration::from_secs(30));
    let mut uplink = MqttUplinkConfig::velamq(
        "acceptance",
        options.broker.clone(),
        publisher_client_id.clone(),
    )
    .with_qos(options.qos);
    uplink.username = options.username.clone();
    uplink.password_env = options.password_env.clone();
    uplink.tls_ca_path = options.tls_ca_path.clone();
    configure_mqtt_options(&mut subscriber_options, &uplink, target.tls)?;
    let (subscriber, mut subscriber_events) = AsyncClient::new(subscriber_options, 16);
    subscriber
        .subscribe(topic.clone(), rumqttc_qos(options.qos)?)
        .await
        .context("enqueue mqtt acceptance subscription")?;
    wait_for_subscription(&mut subscriber_events, options.timeout).await?;

    let mut publisher =
        RumqttcMqttPublisher::connect_from_uplink_with_ack_timeout(&uplink, options.timeout)?;
    let payload = serde_json::to_vec(&MqttAcceptanceProbe {
        kind: "edgeops.mqtt.acceptance",
        probe_id: &probe_id,
        sent_at: now,
    })?;

    let started = Instant::now();
    publisher
        .publish(MqttPublishMessage {
            sink_id: uplink.sink_id,
            broker: uplink.broker,
            client_id: uplink.client_id,
            topic: topic.clone(),
            qos: options.qos,
            payload: payload.clone(),
        })
        .await
        .context("publish mqtt acceptance probe")?;
    wait_for_probe(&mut subscriber_events, &topic, &payload, options.timeout).await?;

    Ok(MqttAcceptanceReport {
        broker: options.broker,
        topic,
        qos: options.qos,
        publisher_client_id,
        subscriber_client_id,
        probe_id,
        sent_at: now,
        round_trip_ms: started.elapsed().as_millis(),
        received_bytes: payload.len(),
        payload_verified: true,
    })
}

fn validate_options(options: &MqttAcceptanceOptions) -> Result<()> {
    if options.client_id_prefix.trim().is_empty() {
        bail!("mqtt acceptance client id prefix is required");
    }
    if options.qos > 2 {
        bail!("mqtt acceptance qos must be 0, 1, or 2");
    }
    if options.timeout.is_zero() {
        bail!("mqtt acceptance timeout must be greater than zero");
    }
    Ok(())
}

fn validate_topic(topic: &str) -> Result<()> {
    if topic.trim().is_empty() {
        bail!("mqtt acceptance topic is required");
    }
    if topic.contains(['#', '+']) {
        bail!("mqtt acceptance topic must not contain wildcard characters");
    }
    Ok(())
}

async fn wait_for_subscription(eventloop: &mut EventLoop, timeout: Duration) -> Result<()> {
    tokio::time::timeout(timeout, async {
        loop {
            if let Event::Incoming(Packet::SubAck(suback)) =
                eventloop.poll().await.context("poll mqtt subscriber")?
            {
                if suback.return_codes.is_empty()
                    || suback
                        .return_codes
                        .iter()
                        .any(|code| matches!(code, SubscribeReasonCode::Failure))
                {
                    bail!("mqtt broker rejected the acceptance subscription");
                }
                return Ok(());
            }
        }
    })
    .await
    .context("mqtt acceptance subscription timed out")?
}

async fn wait_for_probe(
    eventloop: &mut EventLoop,
    topic: &str,
    expected_payload: &[u8],
    timeout: Duration,
) -> Result<()> {
    tokio::time::timeout(timeout, async {
        loop {
            match eventloop
                .poll()
                .await
                .context("poll mqtt acceptance probe")?
            {
                Event::Incoming(Packet::Publish(publish)) if publish.topic == topic => {
                    if publish.payload.as_ref() != expected_payload {
                        bail!("mqtt acceptance probe payload did not match published bytes");
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
    })
    .await
    .context("mqtt acceptance probe delivery timed out")?
}

fn rumqttc_qos(qos: u8) -> Result<QoS> {
    match qos {
        0 => Ok(QoS::AtMostOnce),
        1 => Ok(QoS::AtLeastOnce),
        2 => Ok(QoS::ExactlyOnce),
        _ => bail!("mqtt acceptance qos must be 0, 1, or 2"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };

    #[test]
    fn validates_acceptance_options_before_connecting() {
        let invalid_qos =
            MqttAcceptanceOptions::new("mqtt://127.0.0.1:1883", "edge-test").with_qos(3);
        assert!(validate_options(&invalid_qos)
            .unwrap_err()
            .to_string()
            .contains("qos"));

        assert!(validate_topic("edgeops/acceptance/+")
            .unwrap_err()
            .to_string()
            .contains("wildcard"));
    }

    #[tokio::test]
    async fn verifies_publish_ack_and_exact_subscriber_readback() {
        const PASSWORD_ENV: &str = "EDGEOPS_MQTT_ACCEPTANCE_TEST_PASSWORD";
        std::env::set_var(PASSWORD_ENV, "acceptance-secret");
        let expected_credentials = (
            "acceptance-user".to_string(),
            "acceptance-secret".to_string(),
        );
        let (broker, broker_task) = spawn_round_trip_broker(Some(expected_credentials)).await;
        let report = run_mqtt_acceptance(
            MqttAcceptanceOptions::new(broker.clone(), "acceptance-test")
                .with_topic("edgeops/acceptance/test")
                .with_qos(1)
                .with_credentials_env("acceptance-user", PASSWORD_ENV)
                .with_timeout(Duration::from_secs(2)),
        )
        .await
        .unwrap();

        assert_eq!(report.broker, broker);
        assert_eq!(report.topic, "edgeops/acceptance/test");
        assert_eq!(report.qos, 1);
        assert!(report.payload_verified);
        assert!(report.received_bytes > 0);
        broker_task.await.unwrap();
        std::env::remove_var(PASSWORD_ENV);
    }

    #[test]
    fn private_ca_configuration_fails_fast_when_the_runtime_file_is_missing() {
        let uplink =
            MqttUplinkConfig::velamq("acceptance", "mqtts://127.0.0.1:8883", "acceptance-client")
                .with_tls_ca_path("/missing/edgeops/velamq-ca.pem");

        let error = match RumqttcMqttPublisher::connect_from_uplink(&uplink) {
            Ok(_) => panic!("missing private CA file must reject MQTT construction"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("read mqtt TLS CA certificate"));
    }

    #[test]
    fn missing_password_environment_variable_fails_without_leaking_a_secret() {
        let password_env = format!("EDGEOPS_MQTT_MISSING_{}", std::process::id());
        std::env::remove_var(&password_env);
        let uplink =
            MqttUplinkConfig::velamq("acceptance", "mqtt://127.0.0.1:1883", "acceptance-client")
                .with_credentials_env("edge-device", password_env.clone());

        let error = match RumqttcMqttPublisher::connect_from_uplink(&uplink) {
            Ok(_) => panic!("missing password variable must reject MQTT construction"),
            Err(error) => error,
        };
        assert!(error.to_string().contains(&password_env));
        assert!(!error.to_string().contains("password="));
    }

    async fn spawn_round_trip_broker(
        expected_credentials: Option<(String, String)>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let broker = format!("mqtt://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut subscriber, _) = listener.accept().await.unwrap();
            accept_connect(&mut subscriber, expected_credentials.as_ref()).await;
            let (subscribe_header, subscribe_body) = read_packet(&mut subscriber).await;
            assert_eq!(subscribe_header, 0x82);
            let subscribe_packet_id = u16::from_be_bytes([subscribe_body[0], subscribe_body[1]]);
            let topic_len = usize::from(u16::from_be_bytes([subscribe_body[2], subscribe_body[3]]));
            let topic = String::from_utf8(subscribe_body[4..4 + topic_len].to_vec()).unwrap();
            subscriber
                .write_all(&[
                    0x90,
                    0x03,
                    (subscribe_packet_id >> 8) as u8,
                    subscribe_packet_id as u8,
                    0x01,
                ])
                .await
                .unwrap();

            let (mut publisher, _) = listener.accept().await.unwrap();
            accept_connect(&mut publisher, expected_credentials.as_ref()).await;
            let (publish_header, publish_body) = read_packet(&mut publisher).await;
            assert_eq!(publish_header, 0x32);
            let publish_topic_len =
                usize::from(u16::from_be_bytes([publish_body[0], publish_body[1]]));
            let publish_topic =
                String::from_utf8(publish_body[2..2 + publish_topic_len].to_vec()).unwrap();
            assert_eq!(publish_topic, topic);
            let packet_id_offset = 2 + publish_topic_len;
            let publish_packet_id = u16::from_be_bytes([
                publish_body[packet_id_offset],
                publish_body[packet_id_offset + 1],
            ]);
            let payload = &publish_body[packet_id_offset + 2..];
            publisher
                .write_all(&[
                    0x40,
                    0x02,
                    (publish_packet_id >> 8) as u8,
                    publish_packet_id as u8,
                ])
                .await
                .unwrap();

            let mut forwarded = Vec::new();
            forwarded.extend_from_slice(&(topic.len() as u16).to_be_bytes());
            forwarded.extend_from_slice(topic.as_bytes());
            forwarded.extend_from_slice(&7_u16.to_be_bytes());
            forwarded.extend_from_slice(payload);
            write_packet(&mut subscriber, 0x32, &forwarded).await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        (broker, task)
    }

    async fn accept_connect(
        stream: &mut TcpStream,
        expected_credentials: Option<&(String, String)>,
    ) {
        let (header, body) = read_packet(stream).await;
        assert_eq!(header >> 4, 1);
        let flags = body[7];
        let mut offset = 10;
        let _client_id = read_mqtt_string(&body, &mut offset);
        let credentials = if flags & 0xc0 == 0xc0 {
            Some((
                read_mqtt_string(&body, &mut offset),
                read_mqtt_string(&body, &mut offset),
            ))
        } else {
            None
        };
        assert_eq!(credentials.as_ref(), expected_credentials);
        stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
    }

    fn read_mqtt_string(body: &[u8], offset: &mut usize) -> String {
        let length = usize::from(u16::from_be_bytes([body[*offset], body[*offset + 1]]));
        *offset += 2;
        let value = String::from_utf8(body[*offset..*offset + length].to_vec()).unwrap();
        *offset += length;
        value
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
