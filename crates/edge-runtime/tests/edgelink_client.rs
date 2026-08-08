use chrono::Utc;
use edge_core::{
    decode_edgelink_frame, encode_edgelink_frame, CloudSyncMetrics, CollectionRuntimeMetrics,
    DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint, DataConfigPublish,
    DeviceInstance, DiscoveryRequest, EdgeConfigPackage, EdgeHealth, EdgeLinkMessage,
    EdgeLinkMessageKind, EdgeLinkPayload, EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot,
    LocalStoreMetrics, MqttUplinkConfig, PointAddress, ProtocolConnection, ProtocolRuntimeMetrics,
    RuntimeEventCategory, RuntimeEventSeverity, SerialConnectionSettings, SystemRuntimeMetrics,
    TelemetryPointMapping, TelemetryType,
};
use edge_runtime::{
    append_modbus_rtu_crc, connect_edgelink_once, connect_edgelink_once_with_capabilities,
    handle_edgelink_discovery_requests_with_factory, publish_edgelink_runtime_status_once,
    publish_edgelink_runtime_status_with_mqtt_publisher_once,
    publish_edgelink_runtime_status_with_store_once, RecordingMqttPublisher, RocksEdgeRuntimeStore,
    ScriptedSerialBus, ScriptedSerialBusFactory,
};
use tempfile::tempdir;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::Duration;

#[tokio::test]
async fn runtime_client_sends_hello_and_accepts_cloud_ack() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let gateway_addr = listener.local_addr().expect("listener should expose addr");

    let gateway = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("runtime should connect");
        let hello = read_one_message(&mut stream).await;
        let ack = EdgeLinkMessage::ack(
            hello.edge_id.clone(),
            hello
                .runtime_id
                .clone()
                .expect("hello envelope should include runtime id"),
            hello.message_id,
            hello.sequence,
        );
        let ack_frame = encode_edgelink_frame(&ack).expect("ack should encode");
        stream
            .write_all(&ack_frame)
            .await
            .expect("ack should be written");
        hello
    });

    let report = connect_edgelink_once(
        &gateway_addr.to_string(),
        "edge-dev",
        "runtime-dev",
        "0.1.0",
        Some("2026.06.26-001".to_string()),
    )
    .await
    .expect("runtime client should connect");

    assert_eq!(report.edge_id, "edge-dev");
    assert_eq!(report.runtime_id, "runtime-dev");
    assert_eq!(report.gateway_addr, gateway_addr.to_string());
    assert!(report.acked);

    let observed = gateway.await.expect("gateway task should finish");
    assert_eq!(observed.kind, EdgeLinkMessageKind::Hello);
    assert_eq!(observed.edge_id, "edge-dev");
    assert_eq!(observed.runtime_id.as_deref(), Some("runtime-dev"));

    let EdgeLinkPayload::Hello(payload) = observed.payload else {
        panic!("expected hello payload");
    };
    assert_eq!(payload.runtime_version, "0.1.0");
    assert_eq!(
        payload.applied_config_version.as_deref(),
        Some("2026.06.26-001")
    );
}

#[tokio::test]
async fn runtime_client_sends_configured_capabilities_in_hello() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let gateway_addr = listener.local_addr().expect("listener should expose addr");

    let gateway = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("runtime should connect");
        let hello = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &hello).await;
        hello
    });

    connect_edgelink_once_with_capabilities(
        &gateway_addr.to_string(),
        "edge-dev",
        "runtime-dev",
        "0.1.0",
        None,
        vec![
            "protocol:modbus-rtu".to_string(),
            "transport:serial".to_string(),
            "uplink:mqtt".to_string(),
        ],
    )
    .await
    .expect("runtime client should connect");

    let observed = gateway.await.expect("gateway task should finish");
    let EdgeLinkPayload::Hello(payload) = observed.payload else {
        panic!("expected hello payload");
    };
    assert_eq!(
        payload.capabilities,
        vec![
            "protocol:modbus-rtu".to_string(),
            "transport:serial".to_string(),
            "uplink:mqtt".to_string()
        ]
    );
}

#[tokio::test]
async fn runtime_client_publishes_metrics_and_events_after_hello() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let gateway_addr = listener.local_addr().expect("listener should expose addr");

    let gateway = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("runtime should connect");

        let hello = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &hello).await;

        let metrics = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &metrics).await;

        let event = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &event).await;

        vec![hello, metrics, event]
    });

    let report = publish_edgelink_runtime_status_once(
        &gateway_addr.to_string(),
        "edge-dev",
        "runtime-dev",
        "0.1.0",
        runtime_metrics("edge-dev", "runtime-dev"),
        vec![EdgeRuntimeEvent::new(
            "edge-dev",
            RuntimeEventSeverity::Warning,
            RuntimeEventCategory::Protocol,
            "modbus.timeout",
            "Modbus request timed out",
        )],
    )
    .await
    .expect("runtime status should publish");

    assert_eq!(report.edge_id, "edge-dev");
    assert_eq!(report.runtime_id, "runtime-dev");
    assert_eq!(report.acked_message_count, 2);
    assert_eq!(report.samples_collected, 0);

    let observed = gateway.await.expect("gateway task should finish");
    assert_eq!(observed[0].kind, EdgeLinkMessageKind::Hello);
    assert_eq!(observed[1].kind, EdgeLinkMessageKind::RuntimeMetrics);
    assert_eq!(observed[2].kind, EdgeLinkMessageKind::RuntimeEvent);

    let EdgeLinkPayload::RuntimeMetrics(metrics) = &observed[1].payload else {
        panic!("expected runtime metrics payload");
    };
    assert_eq!(metrics.config_version, "2026.06.27-001");

    let EdgeLinkPayload::RuntimeEvent(event) = &observed[2].payload else {
        panic!("expected runtime event payload");
    };
    assert_eq!(event.code, "modbus.timeout");
}

#[tokio::test]
async fn runtime_client_applies_config_deploy_before_publishing_metrics() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let gateway_addr = listener.local_addr().expect("listener should expose addr");

    let gateway = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("runtime should connect");

        let hello = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &hello).await;

        let deploy = EdgeLinkMessage::config_deploy(
            "edge-dev",
            "runtime-dev",
            2,
            EdgeConfigPackage::new("edge-dev", "2026.06.27-010"),
        );
        let deploy_frame = encode_edgelink_frame(&deploy).expect("deploy should encode");
        stream
            .write_all(&deploy_frame)
            .await
            .expect("deploy should be written");

        let report = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &report).await;

        let metrics = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &metrics).await;

        vec![hello, report, metrics]
    });

    let report = publish_edgelink_runtime_status_once(
        &gateway_addr.to_string(),
        "edge-dev",
        "runtime-dev",
        "0.1.0",
        runtime_metrics("edge-dev", "runtime-dev"),
        Vec::new(),
    )
    .await
    .expect("runtime status should publish");

    assert_eq!(
        report.applied_config_version.as_deref(),
        Some("2026.06.27-010")
    );

    let observed = gateway.await.expect("gateway task should finish");
    assert_eq!(observed[1].kind, EdgeLinkMessageKind::ConfigReport);
    let EdgeLinkPayload::ConfigReport(config_report) = &observed[1].payload else {
        panic!("expected config report payload");
    };
    assert_eq!(config_report.desired_version, "2026.06.27-010");
    assert_eq!(
        config_report.applied_version.as_deref(),
        Some("2026.06.27-010")
    );
    assert!(config_report.accepted);
    assert_eq!(observed[2].kind, EdgeLinkMessageKind::RuntimeMetrics);
    let EdgeLinkPayload::RuntimeMetrics(metrics) = &observed[2].payload else {
        panic!("expected runtime metrics payload");
    };
    assert_eq!(metrics.config_version, "2026.06.27-010");
    assert_eq!(metrics.cloud_sync.desired_version, "2026.06.27-010");
    assert_eq!(metrics.cloud_sync.reported_version, "2026.06.27-010");
}

#[tokio::test]
async fn runtime_client_persists_config_deploy_to_rocksdb_before_reporting() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let gateway_addr = listener.local_addr().expect("listener should expose addr");
    let dir = tempdir().unwrap();
    let store = RocksEdgeRuntimeStore::open(dir.path().join("runtime.rocksdb")).unwrap();

    let gateway = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("runtime should connect");

        let hello = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &hello).await;

        let deploy = EdgeLinkMessage::config_deploy(
            "edge-dev",
            "runtime-dev",
            2,
            EdgeConfigPackage::new("edge-dev", "2026.06.27-030"),
        );
        let deploy_frame = encode_edgelink_frame(&deploy).expect("deploy should encode");
        stream
            .write_all(&deploy_frame)
            .await
            .expect("deploy should be written");

        let report = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &report).await;

        let metrics = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &metrics).await;
    });

    let report = publish_edgelink_runtime_status_with_store_once(
        &gateway_addr.to_string(),
        "edge-dev",
        "runtime-dev",
        "0.1.0",
        runtime_metrics("edge-dev", "runtime-dev"),
        Vec::new(),
        &store,
    )
    .await
    .expect("runtime status should publish");

    assert_eq!(
        report.applied_config_version.as_deref(),
        Some("2026.06.27-030")
    );
    gateway.await.expect("gateway task should finish");

    let desired = store
        .desired_config("edge-dev", "2026.06.27-030")
        .unwrap()
        .expect("desired config should be persisted");
    assert_eq!(desired.version, "2026.06.27-030");

    let active = store
        .active_config("edge-dev")
        .unwrap()
        .expect("active config should be promoted");
    assert_eq!(active.version, "2026.06.27-030");
}

#[tokio::test]
async fn runtime_client_publishes_mqtt_after_edgelink_config_deploy() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let gateway_addr = listener.local_addr().expect("listener should expose addr");

    let gateway = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("runtime should connect");

        let hello = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &hello).await;

        let deploy =
            EdgeLinkMessage::config_deploy("edge-dev", "runtime-dev", 2, mqtt_deploy_package());
        let deploy_frame = encode_edgelink_frame(&deploy).expect("deploy should encode");
        stream
            .write_all(&deploy_frame)
            .await
            .expect("deploy should be written");

        let report = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &report).await;

        let metrics = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &metrics).await;
        metrics
    });

    let mut mqtt = RecordingMqttPublisher::default();
    let report = publish_edgelink_runtime_status_with_mqtt_publisher_once(
        &gateway_addr.to_string(),
        "edge-dev",
        "runtime-dev",
        "0.1.0",
        runtime_metrics("edge-dev", "runtime-dev"),
        Vec::new(),
        &mut mqtt,
    )
    .await
    .expect("runtime status should publish");

    assert_eq!(
        report.applied_config_version.as_deref(),
        Some("2026.06.27-040")
    );
    assert_eq!(report.samples_collected, 1);
    assert_eq!(report.mqtt_messages_published, 1);
    assert_eq!(mqtt.messages().len(), 1);
    assert_eq!(mqtt.messages()[0].topic, "velamq/edge-dev/pump-1/pressure");
    let metrics = gateway.await.expect("gateway task should finish");
    let EdgeLinkPayload::RuntimeMetrics(snapshot) = metrics.payload else {
        panic!("expected runtime metrics");
    };
    assert_eq!(snapshot.collection.success_rate, 1.0);
    assert!(snapshot.collection.average_latency_ms >= 1);
    assert_eq!(snapshot.protocols.len(), 1);
    assert_eq!(snapshot.protocols[0].connection_id, "sim-main");
    assert!(snapshot.protocols[0].connected);
    assert!(snapshot.protocols[0].latency_ms >= 1);
}

#[tokio::test]
async fn runtime_client_publishes_data_config_json_after_edgelink_config_deploy() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let gateway_addr = listener.local_addr().expect("listener should expose addr");

    let gateway = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("runtime should connect");

        let hello = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &hello).await;

        let deploy = EdgeLinkMessage::config_deploy(
            "edge-dev",
            "runtime-dev",
            2,
            data_config_deploy_package(),
        );
        let deploy_frame = encode_edgelink_frame(&deploy).expect("deploy should encode");
        stream
            .write_all(&deploy_frame)
            .await
            .expect("deploy should be written");

        let report = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &report).await;

        let metrics = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &metrics).await;
    });

    let mut mqtt = RecordingMqttPublisher::default();
    let report = publish_edgelink_runtime_status_with_mqtt_publisher_once(
        &gateway_addr.to_string(),
        "edge-dev",
        "runtime-dev",
        "0.1.0",
        runtime_metrics("edge-dev", "runtime-dev"),
        Vec::new(),
        &mut mqtt,
    )
    .await
    .expect("runtime status should publish");

    assert_eq!(
        report.applied_config_version.as_deref(),
        Some("2026.06.27-041")
    );
    assert_eq!(report.samples_collected, 1);
    assert_eq!(report.mqtt_messages_published, 1);
    assert_eq!(mqtt.messages().len(), 1);
    assert_eq!(mqtt.messages()[0].topic, "factory/edge-dev/pump-1/status");
    let payload: serde_json::Value = serde_json::from_slice(&mqtt.messages()[0].payload).unwrap();
    assert_eq!(payload["config_id"], "pump_status");
    let pressure = payload["values"]["pressure"]
        .as_f64()
        .expect("simulated pressure should be numeric");
    assert!((2.22..=2.58).contains(&pressure));
    gateway.await.expect("gateway task should finish");
}

#[tokio::test]
async fn runtime_keeps_edgelink_online_when_an_active_protocol_is_unavailable() {
    let directory = tempdir().expect("temp directory should be created");
    let store = RocksEdgeRuntimeStore::open(directory.path()).expect("store should open");
    let package = EdgeConfigPackage::new("edge-dev", "2026.07.23-unavailable-serial")
        .with_device(DeviceInstance::new("meter-1", "meter"))
        .with_protocol_connection(ProtocolConnection::modbus_rtu_serial(
            "serial-offline",
            SerialConnectionSettings::new("/dev/edgeops-device-does-not-exist", 9600),
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "voltage",
            "meter-1",
            "voltage",
            "serial-offline",
            PointAddress::modbus_holding_register(40001),
            TelemetryType::Float,
        ));
    store
        .put_desired_config(&package)
        .expect("desired config should persist");
    store
        .promote_active_config("edge-dev", &package.version)
        .expect("config should become active");

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let gateway_addr = listener.local_addr().expect("listener should expose addr");
    let gateway = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("runtime should connect");
        let hello = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &hello).await;
        let metrics = read_one_message(&mut stream).await;
        write_ack_for(&mut stream, &metrics).await;
        metrics
    });

    let report = publish_edgelink_runtime_status_with_store_once(
        &gateway_addr.to_string(),
        "edge-dev",
        "runtime-dev",
        "0.1.0",
        runtime_metrics("edge-dev", "runtime-dev"),
        Vec::new(),
        &store,
    )
    .await
    .expect("protocol failure should not terminate the EdgeLink control session");

    assert_eq!(report.acked_message_count, 1);
    assert_eq!(report.samples_collected, 0);
    let metrics = gateway.await.expect("gateway task should finish");
    let EdgeLinkPayload::RuntimeMetrics(snapshot) = metrics.payload else {
        panic!("expected runtime metrics");
    };
    assert_eq!(snapshot.collection.success_rate, 0.0);
    assert_eq!(snapshot.collection.bad_point_count, 1);
    assert_eq!(snapshot.health, EdgeHealth::Degraded);
    assert_eq!(snapshot.protocols.len(), 1);
    assert!(!snapshot.protocols[0].connected);
    assert_eq!(snapshot.protocols[0].error_count, 1);
}

#[tokio::test]
async fn runtime_executes_discovery_request_from_active_config_and_reports_result() {
    let directory = tempdir().expect("temp directory should be created");
    let store = RocksEdgeRuntimeStore::open(directory.path()).expect("store should open");
    let package = EdgeConfigPackage::new("edge-dev", "2026.07.16-discovery")
        .with_protocol_connection(ProtocolConnection::modbus_rtu_serial(
            "meter-rs485-bus-1",
            SerialConnectionSettings::new("/dev/ttyUSB-test", 9600),
        ));
    store
        .put_desired_config(&package)
        .expect("desired config should persist");
    store
        .promote_active_config("edge-dev", &package.version)
        .expect("config should become active");

    let bus = ScriptedSerialBus::new(vec![modbus_response(1, 231)]);
    let observed_bus = bus.clone();
    let mut factory = ScriptedSerialBusFactory::new(vec![("meter-rs485-bus-1".to_string(), bus)]);
    let (mut runtime, mut gateway) = tokio::io::duplex(16 * 1024);

    let gateway_flow = async {
        let request = EdgeLinkMessage::discovery_request(
            "edge-dev",
            "runtime-dev",
            10_000,
            DiscoveryRequest::modbus_holding_registers(
                "job-runtime-1",
                "meter-rs485-bus-1",
                40001,
                40001,
            ),
        );
        let frame = encode_edgelink_frame(&request).expect("request should encode");
        gateway
            .write_all(&frame)
            .await
            .expect("request should be written");
        let report = read_one_message(&mut gateway).await;
        write_ack_for(&mut gateway, &report).await;
        tokio::time::sleep(Duration::from_millis(40)).await;
        report
    };
    let runtime_flow = handle_edgelink_discovery_requests_with_factory(
        &mut runtime,
        "edge-dev",
        "runtime-dev",
        Some(&store),
        &mut factory,
        Duration::from_millis(25),
    );
    let (report, published) = tokio::join!(gateway_flow, runtime_flow);
    assert_eq!(published.expect("discovery command should execute"), 1);
    let EdgeLinkPayload::DiscoveryReport(report) = report.payload else {
        panic!("expected discovery report");
    };
    assert_eq!(report.job_id, "job-runtime-1");
    assert_eq!(report.discovered_points[0].sample_values, vec!["231"]);
    assert_eq!(&observed_bus.requests()[0][..6], &[1, 0x03, 0, 0, 0, 1]);
}

async fn read_one_message<S>(stream: &mut S) -> EdgeLinkMessage
where
    S: AsyncRead + Unpin,
{
    let mut header = [0_u8; 4];
    stream
        .read_exact(&mut header)
        .await
        .expect("frame header should be readable");
    let len = u32::from_be_bytes(header) as usize;
    let mut frame = vec![0_u8; 4 + len];
    frame[..4].copy_from_slice(&header);
    stream
        .read_exact(&mut frame[4..])
        .await
        .expect("frame body should be readable");
    decode_edgelink_frame(&frame).expect("message should decode")
}

async fn write_ack_for<S>(stream: &mut S, message: &EdgeLinkMessage)
where
    S: AsyncWrite + Unpin,
{
    let ack = EdgeLinkMessage::ack(
        message.edge_id.clone(),
        message
            .runtime_id
            .clone()
            .expect("runtime message should include runtime id"),
        message.message_id,
        message.sequence,
    );
    let ack_frame = encode_edgelink_frame(&ack).expect("ack should encode");
    stream
        .write_all(&ack_frame)
        .await
        .expect("ack should be written");
}

fn modbus_response(slave_id: u8, register: u16) -> Vec<u8> {
    let mut frame = vec![slave_id, 0x03, 2];
    frame.extend(register.to_be_bytes());
    append_modbus_rtu_crc(&mut frame);
    frame
}

fn runtime_metrics(edge_id: &str, runtime_id: &str) -> EdgeRuntimeMetricsSnapshot {
    EdgeRuntimeMetricsSnapshot {
        edge_id: edge_id.to_string(),
        runtime_id: runtime_id.to_string(),
        config_version: "2026.06.27-001".to_string(),
        timestamp: Utc::now(),
        health: EdgeHealth::Healthy,
        system: SystemRuntimeMetrics {
            cpu_percent: 21.0,
            memory_percent: 44.0,
            disk_percent: 58.0,
            process_uptime_seconds: 180,
        },
        collection: CollectionRuntimeMetrics {
            active_task_count: 1,
            success_rate: 0.99,
            average_latency_ms: 20,
            bad_point_count: 0,
        },
        protocols: vec![ProtocolRuntimeMetrics {
            connection_id: "modbus-main".to_string(),
            protocol: "Modbus TCP".to_string(),
            connected: true,
            latency_ms: 10,
            timeout_count: 1,
            error_count: 0,
            reconnect_count: 0,
            collection_attempt_count: 10,
            collection_success_count: 9,
            write_attempt_count: 0,
            write_success_count: 0,
            circuit_state: Default::default(),
            consecutive_failure_count: 0,
            circuit_open_count: 0,
            circuit_rejected_count: 0,
            last_quality_code: None,
            good_value_count: 0,
            uncertain_value_count: 0,
            bad_value_count: 0,
            subscription_count: 0,
            notification_count: 0,
            subscription_error_count: 0,
            fallback_poll_count: 0,
        }],
        local_store: LocalStoreMetrics {
            backend: "rocksdb".to_string(),
            buffered_records: 0,
            oldest_buffer_age_seconds: 0,
            disk_usage_percent: 32.0,
        },
        algorithms: Vec::new(),
        mqtt: Default::default(),
        cloud_sync: CloudSyncMetrics {
            connected: true,
            last_sync_seconds_ago: 0,
            pending_uploads: 0,
            desired_version: "2026.06.27-001".to_string(),
            reported_version: "2026.06.27-001".to_string(),
        },
    }
}

fn mqtt_deploy_package() -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", "2026.06.27-040")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_mqtt_uplink(
            MqttUplinkConfig::velamq("velamq-main", "mqtt://velamq.local:1883", "edge-dev")
                .with_topic_template("velamq/{edge_id}/{device_id}/{telemetry_id}"),
        )
        .with_point_mapping(TelemetryPointMapping::new(
            "pressure",
            "pump-1",
            "pressure",
            "sim-main",
            PointAddress::simulated("pressure"),
            TelemetryType::Float,
        ))
}

fn data_config_deploy_package() -> EdgeConfigPackage {
    mqtt_deploy_package()
        .with_data_config(
            DataConfig::new(
                "pump_status",
                "泵状态上报",
                "pump-1",
                "sim-main",
                DataConfigCollection::new(1000),
                DataConfigPublish::new(
                    "velamq-main",
                    "factory/{edge_id}/{device_id}/status",
                    DataConfigPayload::object(),
                ),
            )
            .with_point(DataConfigPoint::new(
                "pressure",
                "pump.pressure",
                PointAddress::simulated("pressure"),
                TelemetryType::Float,
                "pressure",
            )),
        )
        .tap_version("2026.06.27-041")
}

trait TestPackageVersionExt {
    fn tap_version(self, version: &str) -> Self;
}

impl TestPackageVersionExt for EdgeConfigPackage {
    fn tap_version(mut self, version: &str) -> Self {
        self.version = version.to_string();
        self
    }
}
