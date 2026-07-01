use chrono::{TimeZone, Utc};
use edge_core::{
    AlgorithmDsl, AlgorithmInputBinding, AlgorithmKind, AlgorithmOutput, AlgorithmReportMode,
    AlgorithmReportPolicy, AlgorithmSpec, AlgorithmStep, AlgorithmTrigger, CollectionTask,
    DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint, DataConfigPublish,
    DataQuality, DeviceInstance, EdgeConfigPackage, MqttUplinkConfig, PointAddress,
    ProtocolConnection, TelemetryPointMapping, TelemetryType, TelemetryValue,
};
use edge_runtime::{
    build_data_config_mqtt_publish_messages, build_mqtt_publish_messages, parse_mqtt_broker_target,
    AppliedEdgeConfig, ConfiguredSimulatedRuntime, RecordingMqttPublisher,
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

#[test]
fn data_config_builds_one_json_message_per_config() {
    let package = EdgeConfigPackage::new("edge-dev", "v1")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtts://velamq.local:8883",
            "edge-dev-runtime",
        ))
        .with_data_config(
            DataConfig::new(
                "pump_status",
                "泵运行状态上报",
                "pump-1",
                "modbus-line-a",
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
                PointAddress::modbus_holding_register(40001),
                TelemetryType::Float,
                "pressure",
            ))
            .with_point(DataConfigPoint::new(
                "running",
                "pump.running",
                PointAddress::modbus_holding_register(40002),
                TelemetryType::Boolean,
                "running",
            )),
        );

    let samples = vec![
        edge_core::TelemetrySample::new(
            "pump-1",
            "pressure",
            TelemetryValue::Float(0.82),
            DataQuality::Good,
            Utc.with_ymd_and_hms(2026, 6, 30, 8, 30, 0).unwrap(),
        ),
        edge_core::TelemetrySample::new(
            "pump-1",
            "running",
            TelemetryValue::Boolean(true),
            DataQuality::Good,
            Utc.with_ymd_and_hms(2026, 6, 30, 8, 30, 1).unwrap(),
        ),
    ];

    let messages = build_data_config_mqtt_publish_messages(&package, &samples).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].topic, "factory/edge-dev/pump-1/status");

    let payload: serde_json::Value = serde_json::from_slice(&messages[0].payload).unwrap();
    assert_eq!(payload["edge_id"], "edge-dev");
    assert_eq!(payload["device_id"], "pump-1");
    assert_eq!(payload["values"]["pressure"], 0.82);
    assert_eq!(payload["values"]["running"], true);
    assert_eq!(payload["quality"]["pressure"], "good");
}

#[test]
fn data_config_payload_includes_bound_algorithm_outputs() {
    let package = EdgeConfigPackage::new("edge-dev", "v1")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtts://velamq.local:8883",
            "edge-dev-runtime",
        ))
        .with_algorithm(AlgorithmSpec::dsl(
            "pressure-change",
            "v1",
            AlgorithmKind::ChangeReport,
            AlgorithmDsl {
                inputs: vec![AlgorithmInputBinding::new("p", "pressure")],
                trigger: AlgorithmTrigger::on_sample(),
                steps: vec![AlgorithmStep::change_filter("p", 0.1)],
                outputs: vec![AlgorithmOutput::virtual_point(
                    "pressureReported",
                    "pressure.reported",
                )],
                report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnChange, "velamq-main"),
            },
        ))
        .with_algorithm(AlgorithmSpec::dsl(
            "unbound-algorithm",
            "v1",
            AlgorithmKind::ChangeReport,
            AlgorithmDsl {
                inputs: vec![AlgorithmInputBinding::new("p", "pressure")],
                trigger: AlgorithmTrigger::on_sample(),
                steps: vec![AlgorithmStep::change_filter("p", 0.1)],
                outputs: vec![AlgorithmOutput::virtual_point(
                    "unboundReported",
                    "pressure.unbound",
                )],
                report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnChange, "velamq-main"),
            },
        ))
        .with_data_config(
            DataConfig::new(
                "pump_status",
                "泵运行状态上报",
                "pump-1",
                "modbus-line-a",
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
                PointAddress::modbus_holding_register(40001),
                TelemetryType::Float,
                "pressure",
            ))
            .with_algorithm("pressure-change"),
        );

    let timestamp = Utc.with_ymd_and_hms(2026, 7, 1, 8, 30, 0).unwrap();
    let samples = vec![
        edge_core::TelemetrySample::new(
            "pump-1",
            "pressure",
            TelemetryValue::Float(0.82),
            DataQuality::Good,
            timestamp,
        ),
        edge_core::TelemetrySample::new(
            "pump-1",
            "pressure.reported",
            TelemetryValue::Float(0.82),
            DataQuality::Good,
            timestamp,
        ),
        edge_core::TelemetrySample::new(
            "pump-1",
            "pressure.unbound",
            TelemetryValue::Float(0.99),
            DataQuality::Good,
            timestamp,
        ),
    ];

    let messages = build_data_config_mqtt_publish_messages(&package, &samples).unwrap();

    assert_eq!(messages.len(), 1);
    let payload: serde_json::Value = serde_json::from_slice(&messages[0].payload).unwrap();
    assert_eq!(payload["values"]["pressure"], 0.82);
    assert_eq!(payload["values"]["pressureReported"], 0.82);
    assert!(payload["values"].get("unboundReported").is_none());
}

#[test]
fn data_config_payload_rejects_algorithm_output_json_field_collisions() {
    let package = EdgeConfigPackage::new("edge-dev", "v1")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtts://velamq.local:8883",
            "edge-dev-runtime",
        ))
        .with_algorithm(AlgorithmSpec::dsl(
            "pressure-change",
            "v1",
            AlgorithmKind::ChangeReport,
            AlgorithmDsl {
                inputs: vec![AlgorithmInputBinding::new("p", "pressure")],
                trigger: AlgorithmTrigger::on_sample(),
                steps: vec![AlgorithmStep::change_filter("p", 0.1)],
                outputs: vec![AlgorithmOutput::virtual_point(
                    "pressure",
                    "pressure.reported",
                )],
                report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnChange, "velamq-main"),
            },
        ))
        .with_data_config(
            DataConfig::new(
                "pump_status",
                "泵运行状态上报",
                "pump-1",
                "modbus-line-a",
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
                PointAddress::modbus_holding_register(40001),
                TelemetryType::Float,
                "pressure",
            ))
            .with_algorithm("pressure-change"),
        );

    let timestamp = Utc.with_ymd_and_hms(2026, 7, 1, 8, 30, 0).unwrap();
    let samples = vec![
        edge_core::TelemetrySample::new(
            "pump-1",
            "pressure",
            TelemetryValue::Float(0.82),
            DataQuality::Good,
            timestamp,
        ),
        edge_core::TelemetrySample::new(
            "pump-1",
            "pressure.reported",
            TelemetryValue::Float(0.82),
            DataQuality::Good,
            timestamp,
        ),
    ];

    let error = build_data_config_mqtt_publish_messages(&package, &samples)
        .expect_err("duplicate algorithm output json field is rejected");

    assert!(error
        .to_string()
        .contains("data config pump_status has duplicate json field pressure"));
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

#[test]
fn mqtt_broker_target_parses_tcp_and_tls_urls() {
    let mqtt = parse_mqtt_broker_target("mqtt://velamq.local").unwrap();
    assert_eq!(mqtt.host, "velamq.local");
    assert_eq!(mqtt.port, 1883);
    assert!(!mqtt.tls);

    let mqtts = parse_mqtt_broker_target("mqtts://velamq.local:8883").unwrap();
    assert_eq!(mqtts.host, "velamq.local");
    assert_eq!(mqtts.port, 8883);
    assert!(mqtts.tls);
}
