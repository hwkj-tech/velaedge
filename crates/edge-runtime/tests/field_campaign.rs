use std::{
    collections::BTreeSet,
    io,
    process::{Command, Stdio},
    time::Duration,
};

use edge_core::{
    DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint, DataConfigPublish,
    DeviceInstance, EdgeConfigPackage, MqttUplinkConfig, PointAddress, ProtocolConnection,
    TelemetryPointMapping, TelemetryType,
};
use edge_runtime::{
    run_field_endurance_acceptance, start_mqtt_field_receipt_session, FieldEnduranceOptions,
    MqttFieldReceiptOptions,
};
use sha2::{Digest, Sha256};
use tempfile::tempdir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

#[tokio::test]
async fn coordinated_campaign_captures_every_runtime_publish_after_subscription_readiness() {
    let (broker, broker_task) = spawn_relay_broker().await;
    let package = campaign_package(broker);
    let package_bytes = serde_json::to_vec(&package).unwrap();
    let package_sha256 = format!("{:x}", Sha256::digest(&package_bytes));
    let mut receipt_session = start_mqtt_field_receipt_session(
        MqttFieldReceiptOptions::new(package.clone(), package_sha256.clone())
            .with_startup_timeout(Duration::from_secs(2)),
    )
    .unwrap();
    receipt_session.wait_ready().await.unwrap();

    let directory = tempdir().unwrap();
    let mut endurance =
        FieldEnduranceOptions::laboratory(package, directory.path().join("runtime.rocksdb"));
    endurance.package_sha256 = Some(package_sha256.clone());
    // The simulated pressure wave is intentionally slow and rounded to three decimals. Cover
    // enough of its period to prove value change even when the test starts near a wave extremum.
    endurance.duration = Duration::from_millis(1_200);
    endurance.scheduler_interval = Duration::from_millis(10);
    endurance.minimum_cycles = 20;
    endurance.maximum_failure_ratio = 0.0;
    endurance.changing_points = BTreeSet::from(["pump-1/pressure".to_string()]);
    endurance.exercise_mqtt = true;

    let report = run_field_endurance_acceptance(endurance).await.unwrap();
    tokio::time::sleep(Duration::from_millis(30)).await;
    let receipt = receipt_session.finish().await.unwrap();

    assert!(report.passed(), "{report:#?}");
    assert_eq!(
        report.package_sha256.as_deref(),
        Some(package_sha256.as_str())
    );
    assert_eq!(receipt.package_sha256, package_sha256);
    assert_eq!(receipt.edge_id, report.edge_id);
    assert_eq!(receipt.config_version, report.config_version);
    assert_eq!(receipt.message_count, report.mqtt.publish_success_count);
    assert_eq!(
        receipt.routes[0].topics,
        vec!["campaign/field-campaign-edge/pump-1/telemetry"]
    );
    broker_task.await.unwrap();
}

#[test]
fn physical_preflight_validates_without_creating_evidence_or_opening_sessions() {
    let temporary = tempdir().unwrap();
    let config = temporary.path().join("configuration-package.json");
    let evidence = temporary.path().join("campaign");
    let audit = temporary.path().join("native-broker-audit.json");
    std::fs::write(
        &config,
        serde_json::to_vec_pretty(&physical_campaign_package()).unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_field-campaign"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--output-dir",
            evidence.to_str().unwrap(),
            "--native-broker-audit",
            audit.to_str().unwrap(),
            "--duration-seconds",
            "86400",
            "--scheduler-interval-ms",
            "100",
            "--maximum-progress-gap-seconds",
            "300",
            "--physical-device-exercised",
            "--site-id",
            "WO-42",
            "--operator",
            "operator-a",
            "--device-connection-id",
            "modbus-main",
            "--device-manufacturer",
            "Vendor A",
            "--device-model",
            "PLC-100",
            "--device-serial",
            "ASSET-001",
            "--preflight-only",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "preflight failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!evidence.exists(), "preflight must not create evidence");
    assert!(!audit.exists(), "preflight must not create a broker audit");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["status"], "passed");
    assert_eq!(report["physicalDevice"]["connectionId"], "modbus-main");
    assert_eq!(report["protocolConnections"][0]["protocol"], "Modbus TCP");
    assert_eq!(report["mqttOutputRoutes"].as_array().unwrap().len(), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn sigterm_retains_an_interrupted_campaign_manifest() {
    let (broker, broker_task) = spawn_relay_broker().await;
    let temporary = tempdir().unwrap();
    let config = temporary.path().join("configuration-package.json");
    let evidence = temporary.path().join("campaign");
    let audit = temporary.path().join("native-broker-audit.json");
    tokio::fs::write(
        &config,
        serde_json::to_vec_pretty(&campaign_package(broker)).unwrap(),
    )
    .await
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_field-campaign"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--output-dir",
            evidence.to_str().unwrap(),
            "--native-broker-audit",
            audit.to_str().unwrap(),
            "--duration-seconds",
            "30",
            "--scheduler-interval-ms",
            "10",
            "--minimum-cycles",
            "100000",
            "--maximum-progress-gap-seconds",
            "5",
            "--receipt-post-run-grace-seconds",
            "0",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let manifest_path = evidence.join("manifest.json");
    wait_for_manifest_phase(&manifest_path, "runtime_endurance").await;
    let signal_result = unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
    assert_eq!(signal_result, 0, "failed to deliver SIGTERM to campaign");

    let exit_status = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        let _ = child.kill();
        panic!("field campaign did not exit after SIGTERM")
    });
    assert!(!exit_status.success());

    let manifest: serde_json::Value =
        serde_json::from_slice(&tokio::fs::read(&manifest_path).await.unwrap()).unwrap();
    assert_eq!(manifest["status"], "failed");
    assert_eq!(manifest["phase"], "interrupted");
    let errors = manifest["errors"].as_array().unwrap();
    assert!(errors.iter().any(|error| error
        .as_str()
        .is_some_and(|error| error.contains("SIGTERM"))));
    assert!(errors.iter().any(|error| error
        .as_str()
        .is_some_and(|error| error.contains("native broker audit was not awaited"))));

    broker_task.abort();
}

fn campaign_package(broker: String) -> EdgeConfigPackage {
    let address = PointAddress::simulated("pressure");
    EdgeConfigPackage::new("field-campaign-edge", "campaign-v1")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_mqtt_uplink(
            MqttUplinkConfig::velamq("campaign-main", broker, "runtime-campaign").with_qos(1),
        )
        .with_point_mapping(TelemetryPointMapping::new(
            "pressure",
            "pump-1",
            "pump.pressure",
            "sim-main",
            address.clone(),
            TelemetryType::Float,
        ))
        .with_data_config(
            DataConfig::new(
                "pump-telemetry",
                "Pump telemetry",
                "pump-1",
                "sim-main",
                DataConfigCollection::new(5),
                DataConfigPublish::new(
                    "campaign-main",
                    "campaign/{edge_id}/{device_id}/telemetry",
                    DataConfigPayload::object(),
                ),
            )
            .with_point(DataConfigPoint::new(
                "pressure",
                "pump.pressure",
                address,
                TelemetryType::Float,
                "pressure",
            )),
        )
}

fn physical_campaign_package() -> EdgeConfigPackage {
    let address = PointAddress::modbus_holding_register(40001);
    EdgeConfigPackage::new("physical-field-edge", "campaign-v1")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::modbus_tcp(
            "modbus-main",
            "192.0.2.10:502",
        ))
        .with_mqtt_uplink(
            MqttUplinkConfig::velamq(
                "campaign-main",
                "mqtt://192.0.2.20:1883",
                "runtime-campaign",
            )
            .with_qos(1),
        )
        .with_point_mapping(TelemetryPointMapping::new(
            "pressure",
            "pump-1",
            "pump.pressure",
            "modbus-main",
            address.clone(),
            TelemetryType::Float,
        ))
        .with_data_config(
            DataConfig::new(
                "pump-telemetry",
                "Pump telemetry",
                "pump-1",
                "modbus-main",
                DataConfigCollection::new(1_000),
                DataConfigPublish::new(
                    "campaign-main",
                    "campaign/{edge_id}/{device_id}/telemetry",
                    DataConfigPayload::object(),
                ),
            )
            .with_point(DataConfigPoint::new(
                "pressure",
                "pump.pressure",
                address,
                TelemetryType::Float,
                "pressure",
            )),
        )
}

async fn spawn_relay_broker() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let broker = format!("mqtt://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut subscriber, _) = listener.accept().await.unwrap();
        let (connect_header, _) = read_packet(&mut subscriber).await.unwrap().unwrap();
        assert_eq!(connect_header >> 4, 1);
        subscriber
            .write_all(&[0x20, 0x02, 0x00, 0x00])
            .await
            .unwrap();
        let (subscribe_header, subscribe) = read_packet(&mut subscriber).await.unwrap().unwrap();
        assert_eq!(subscribe_header, 0x82);
        let subscription_packet_id = [subscribe[0], subscribe[1]];
        subscriber
            .write_all(&[
                0x90,
                0x03,
                subscription_packet_id[0],
                subscription_packet_id[1],
                0x01,
            ])
            .await
            .unwrap();

        let (mut publisher, _) = listener.accept().await.unwrap();
        let (connect_header, _) = read_packet(&mut publisher).await.unwrap().unwrap();
        assert_eq!(connect_header >> 4, 1);
        publisher
            .write_all(&[0x20, 0x02, 0x00, 0x00])
            .await
            .unwrap();

        while let Some((header, body)) = read_packet(&mut publisher).await.unwrap() {
            match header >> 4 {
                3 => relay_publish(header, &body, &mut publisher, &mut subscriber).await,
                12 => publisher.write_all(&[0xd0, 0x00]).await.unwrap(),
                14 => break,
                packet_type => panic!("unexpected MQTT packet type {packet_type}"),
            }
        }
    });
    (broker, task)
}

#[cfg(unix)]
async fn wait_for_manifest_phase(path: &std::path::Path, expected_phase: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(bytes) = tokio::fs::read(path).await {
                if let Ok(manifest) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                    if manifest["phase"] == expected_phase {
                        return;
                    }
                    if manifest["status"] == "failed" {
                        panic!("campaign failed before reaching {expected_phase}: {manifest}");
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("campaign did not reach phase {expected_phase}"));
}

async fn relay_publish(
    header: u8,
    body: &[u8],
    publisher: &mut TcpStream,
    subscriber: &mut TcpStream,
) {
    let topic_len = usize::from(u16::from_be_bytes([body[0], body[1]]));
    let topic_end = 2 + topic_len;
    let qos = (header >> 1) & 0x03;
    let payload_start = if qos > 0 { topic_end + 2 } else { topic_end };
    if qos == 1 {
        let packet_id = &body[topic_end..topic_end + 2];
        publisher
            .write_all(&[0x40, 0x02, packet_id[0], packet_id[1]])
            .await
            .unwrap();
    }

    let mut forwarded = Vec::new();
    forwarded.extend_from_slice(&body[..topic_end]);
    forwarded.extend_from_slice(&body[payload_start..]);
    write_packet(subscriber, 0x30, &forwarded).await.unwrap();
}

async fn read_packet(stream: &mut TcpStream) -> io::Result<Option<(u8, Vec<u8>)>> {
    let header = match stream.read_u8().await {
        Ok(header) => header,
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut multiplier = 1usize;
    let mut remaining_len = 0usize;
    loop {
        let encoded = stream.read_u8().await?;
        remaining_len += usize::from(encoded & 0x7f) * multiplier;
        if encoded & 0x80 == 0 {
            break;
        }
        multiplier *= 128;
    }
    let mut body = vec![0; remaining_len];
    stream.read_exact(&mut body).await?;
    Ok(Some((header, body)))
}

async fn write_packet(stream: &mut TcpStream, header: u8, body: &[u8]) -> io::Result<()> {
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
    stream.write_all(&packet).await
}
