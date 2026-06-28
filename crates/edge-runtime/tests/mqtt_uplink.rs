use chrono::{TimeZone, Utc};
use edge_core::{
    CollectionTask, DataQuality, DeviceInstance, EdgeConfigPackage, MqttUplinkConfig, PointAddress,
    ProtocolConnection, TelemetryPointMapping, TelemetryType, TelemetryValue,
};
use edge_runtime::{
    build_mqtt_publish_messages, AppliedEdgeConfig, ConfiguredSimulatedRuntime,
    RecordingMqttPublisher,
};

fn package() -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", "2026.06.28-001")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_mqtt_uplink(
            MqttUplinkConfig::velamq("velamq-main", "mqtts://velamq.local:8883", "edge-dev")
                .with_topic_template("velamq/{edge_id}/{device_id}/{telemetry_id}")
                .with_qos(1),
        )
        .with_point_mapping(TelemetryPointMapping::new(
            "pressure",
            "pump-1",
            "pressure",
            "sim-main",
            PointAddress::simulated("pressure"),
            TelemetryType::Float,
        ))
        .with_collection_task(CollectionTask::interval(
            "pump-main",
            "pump-1",
            vec!["pressure".to_string()],
            1000,
        ))
}

#[test]
fn mqtt_uplink_builds_velamq_publish_messages_from_cloud_config() {
    let sample = edge_core::TelemetrySample::new(
        "pump-1",
        "pressure",
        TelemetryValue::Float(2.4),
        DataQuality::Good,
        Utc.with_ymd_and_hms(2026, 6, 28, 8, 30, 0).unwrap(),
    );

    let messages = build_mqtt_publish_messages(&package(), &[sample]).unwrap();

    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.sink_id, "velamq-main");
    assert_eq!(message.broker, "mqtts://velamq.local:8883");
    assert_eq!(message.client_id, "edge-dev");
    assert_eq!(message.topic, "velamq/edge-dev/pump-1/pressure");
    assert_eq!(message.qos, 1);

    let payload: serde_json::Value = serde_json::from_slice(&message.payload).unwrap();
    assert_eq!(payload["edge_id"], "edge-dev");
    assert_eq!(payload["device_id"], "pump-1");
    assert_eq!(payload["telemetry_id"], "pressure");
    assert_eq!(payload["config_version"], "2026.06.28-001");
    assert_eq!(payload["value"], serde_json::json!({"Float": 2.4}));
    assert_eq!(payload["quality"], "Good");
}

#[tokio::test]
async fn configured_runtime_collects_and_publishes_to_recording_mqtt_sink() {
    let applied = AppliedEdgeConfig::apply(package()).unwrap();
    let mut runtime = ConfiguredSimulatedRuntime::new(applied);
    let mut publisher = RecordingMqttPublisher::default();

    let report = runtime
        .collect_once_and_publish_mqtt(&mut publisher)
        .await
        .unwrap();

    assert_eq!(report.collection.samples_collected, 1);
    assert_eq!(report.mqtt_messages_published, 1);
    assert_eq!(publisher.messages().len(), 1);
    assert_eq!(
        publisher.messages()[0].topic,
        "velamq/edge-dev/pump-1/pressure"
    );
}
