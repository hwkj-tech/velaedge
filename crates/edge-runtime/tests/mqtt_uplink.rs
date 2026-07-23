use anyhow::{bail, Result};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use edge_core::{
    AlgorithmDsl, AlgorithmInputBinding, AlgorithmKind, AlgorithmOutput, AlgorithmReportMode,
    AlgorithmReportPolicy, AlgorithmSpec, AlgorithmStep, AlgorithmTrigger, CollectionTask,
    DataConfig, DataConfigCollection, DataConfigGraphEdge, DataConfigGraphNode,
    DataConfigGraphNodeKind, DataConfigPayload, DataConfigPoint, DataConfigPublish,
    DataConfigVisualGraph, DataQuality, DeviceInstance, EdgeConfigPackage, MqttUplinkConfig,
    PointAddress, ProtocolConnection, TelemetryPointMapping, TelemetryType, TelemetryValue,
};
use edge_runtime::{
    build_data_config_mqtt_publish_messages, build_mqtt_publish_messages, flush_mqtt_outbox,
    parse_mqtt_broker_target, AppliedEdgeConfig, ConfiguredSimulatedRuntime, MqttPublishMessage,
    MqttPublisher, MultiBrokerMqttPublisher, RecordingMqttPublisher, RocksEdgeRuntimeStore,
    RumqttcMqttPublisher,
};
use tempfile::tempdir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};

#[derive(Default)]
struct FailFirstPublisher {
    attempts: usize,
}

#[async_trait]
impl MqttPublisher for FailFirstPublisher {
    async fn publish(&mut self, _message: MqttPublishMessage) -> Result<()> {
        self.attempts += 1;
        bail!("simulated mqtt outage")
    }
}

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
fn data_config_visual_graph_limits_runtime_payload_to_connected_output_inputs() {
    let mut data_config = DataConfig::new(
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
    ));
    data_config.visual_graph = DataConfigVisualGraph {
        nodes: vec![
            DataConfigGraphNode {
                node_id: "point-pressure".to_string(),
                kind: DataConfigGraphNodeKind::Point,
                label: "pressure".to_string(),
                ref_id: Some("pressure".to_string()),
                params: Default::default(),
                x: 72,
                y: 80,
            },
            DataConfigGraphNode {
                node_id: "point-running".to_string(),
                kind: DataConfigGraphNodeKind::Point,
                label: "running".to_string(),
                ref_id: Some("running".to_string()),
                params: Default::default(),
                x: 72,
                y: 160,
            },
            DataConfigGraphNode {
                node_id: "mqtt-output".to_string(),
                kind: DataConfigGraphNodeKind::Mqtt,
                label: "factory/{edge_id}/{device_id}/status".to_string(),
                ref_id: Some("factory/{edge_id}/{device_id}/status".to_string()),
                params: Default::default(),
                x: 680,
                y: 120,
            },
        ],
        edges: vec![DataConfigGraphEdge {
            edge_id: "point-pressure:value-to-mqtt-output:payload".to_string(),
            from: "point-pressure".to_string(),
            from_port: Some("value".to_string()),
            to: "mqtt-output".to_string(),
            to_port: Some("payload".to_string()),
        }],
    };

    let package = EdgeConfigPackage::new("edge-dev", "v1")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtts://velamq.local:8883",
            "edge-dev-runtime",
        ))
        .with_data_config(data_config);

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
            "running",
            TelemetryValue::Boolean(true),
            DataQuality::Good,
            timestamp,
        ),
    ];

    let messages = build_data_config_mqtt_publish_messages(&package, &samples).unwrap();

    assert_eq!(messages.len(), 1);
    let payload: serde_json::Value = serde_json::from_slice(&messages[0].payload).unwrap();
    assert_eq!(payload["values"]["pressure"], 0.82);
    assert!(payload["values"].get("running").is_none());
}

#[test]
fn data_config_visual_graph_publishes_each_mqtt_output_to_its_own_topic() {
    let mut data_config = DataConfig::new(
        "pump_telemetry",
        "泵分主题上报",
        "pump-1",
        "modbus-line-a",
        DataConfigCollection::new(1000),
        DataConfigPublish::new(
            "velamq-main",
            "factory/{edge_id}/{device_id}/fallback",
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
    ));
    data_config.visual_graph = DataConfigVisualGraph {
        nodes: vec![
            DataConfigGraphNode {
                node_id: "point-pressure".to_string(),
                kind: DataConfigGraphNodeKind::Point,
                label: "pressure".to_string(),
                ref_id: Some("pressure".to_string()),
                params: Default::default(),
                x: 72,
                y: 80,
            },
            DataConfigGraphNode {
                node_id: "point-running".to_string(),
                kind: DataConfigGraphNodeKind::Point,
                label: "running".to_string(),
                ref_id: Some("running".to_string()),
                params: Default::default(),
                x: 72,
                y: 160,
            },
            DataConfigGraphNode {
                node_id: "mqtt-pressure".to_string(),
                kind: DataConfigGraphNodeKind::Mqtt,
                label: "压力输出".to_string(),
                ref_id: Some("factory/{edge_id}/{device_id}/pressure".to_string()),
                params: Default::default(),
                x: 680,
                y: 80,
            },
            DataConfigGraphNode {
                node_id: "mqtt-status".to_string(),
                kind: DataConfigGraphNodeKind::Mqtt,
                label: "状态输出".to_string(),
                ref_id: Some("factory/{edge_id}/{device_id}/status".to_string()),
                params: Default::default(),
                x: 680,
                y: 180,
            },
        ],
        edges: vec![
            DataConfigGraphEdge {
                edge_id: "point-pressure:value-to-mqtt-pressure:payload".to_string(),
                from: "point-pressure".to_string(),
                from_port: Some("value".to_string()),
                to: "mqtt-pressure".to_string(),
                to_port: Some("payload".to_string()),
            },
            DataConfigGraphEdge {
                edge_id: "point-running:value-to-mqtt-status:payload".to_string(),
                from: "point-running".to_string(),
                from_port: Some("value".to_string()),
                to: "mqtt-status".to_string(),
                to_port: Some("payload".to_string()),
            },
        ],
    };

    let package = EdgeConfigPackage::new("edge-dev", "v1")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtts://velamq.local:8883",
            "edge-dev-runtime",
        ))
        .with_data_config(data_config);
    let timestamp = Utc.with_ymd_and_hms(2026, 7, 10, 8, 30, 0).unwrap();
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
            "running",
            TelemetryValue::Boolean(true),
            DataQuality::Good,
            timestamp,
        ),
    ];

    let messages = build_data_config_mqtt_publish_messages(&package, &samples).unwrap();

    assert_eq!(messages.len(), 2);
    let pressure = messages
        .iter()
        .find(|message| message.topic == "factory/edge-dev/pump-1/pressure")
        .unwrap();
    let status = messages
        .iter()
        .find(|message| message.topic == "factory/edge-dev/pump-1/status")
        .unwrap();
    let pressure_payload: serde_json::Value = serde_json::from_slice(&pressure.payload).unwrap();
    let status_payload: serde_json::Value = serde_json::from_slice(&status.payload).unwrap();
    assert_eq!(pressure_payload["values"]["pressure"], 0.82);
    assert!(pressure_payload["values"].get("running").is_none());
    assert_eq!(status_payload["values"]["running"], true);
    assert!(status_payload["values"].get("pressure").is_none());
}

#[test]
fn conditional_output_port_filters_mqtt_branches_and_supports_fan_out() {
    let route = AlgorithmSpec::dsl(
        "pressure-route",
        "v1",
        AlgorithmKind::ThresholdRule,
        AlgorithmDsl {
            inputs: vec![AlgorithmInputBinding::new("p", "pressure")],
            trigger: AlgorithmTrigger::on_sample(),
            steps: vec![AlgorithmStep::ConditionalRoute {
                source: "p".to_string(),
                operator: edge_core::CompareOperator::Gte,
                threshold: 10.0,
                matched_output: "matched".to_string(),
                unmatched_output: "unmatched".to_string(),
            }],
            outputs: vec![
                AlgorithmOutput::virtual_point("matched", "pressure.matched"),
                AlgorithmOutput::virtual_point("unmatched", "pressure.unmatched"),
            ],
            report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnOutput, "velamq-main"),
        },
    );
    let mut data_config = DataConfig::new(
        "pressure-routing",
        "压力分支上报",
        "pump-1",
        "modbus-line-a",
        DataConfigCollection::new(1000),
        DataConfigPublish::new(
            "velamq-main",
            "factory/{edge_id}/{device_id}/fallback",
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
    .with_algorithm("pressure-route");
    let graph_node =
        |node_id: &str, kind: DataConfigGraphNodeKind, label: &str, ref_id: &str, x, y| {
            DataConfigGraphNode {
                node_id: node_id.to_string(),
                kind,
                label: label.to_string(),
                ref_id: Some(ref_id.to_string()),
                params: Default::default(),
                x,
                y,
            }
        };
    let graph_edge = |edge_id: &str, from_port: &str, to: &str| DataConfigGraphEdge {
        edge_id: edge_id.to_string(),
        from: "route".to_string(),
        from_port: Some(from_port.to_string()),
        to: to.to_string(),
        to_port: Some("payload".to_string()),
    };
    data_config.visual_graph = DataConfigVisualGraph {
        nodes: vec![
            graph_node(
                "point-pressure",
                DataConfigGraphNodeKind::Point,
                "pressure",
                "pressure",
                72,
                80,
            ),
            graph_node(
                "route",
                DataConfigGraphNodeKind::Algorithm,
                "条件分支",
                "pressure-route",
                360,
                100,
            ),
            graph_node(
                "matched-a",
                DataConfigGraphNodeKind::Mqtt,
                "命中 A",
                "factory/{edge_id}/{device_id}/matched-a",
                680,
                60,
            ),
            graph_node(
                "matched-b",
                DataConfigGraphNodeKind::Mqtt,
                "命中 B",
                "factory/{edge_id}/{device_id}/matched-b",
                680,
                140,
            ),
            graph_node(
                "unmatched",
                DataConfigGraphNodeKind::Mqtt,
                "未命中",
                "factory/{edge_id}/{device_id}/unmatched",
                680,
                220,
            ),
        ],
        edges: vec![
            DataConfigGraphEdge {
                edge_id: "point-to-route".to_string(),
                from: "point-pressure".to_string(),
                from_port: Some("value".to_string()),
                to: "route".to_string(),
                to_port: Some("input".to_string()),
            },
            graph_edge("matched-to-a", "matched", "matched-a"),
            graph_edge("matched-to-b", "matched", "matched-b"),
            graph_edge("unmatched-to-output", "unmatched", "unmatched"),
        ],
    };
    let package = EdgeConfigPackage::new("edge-dev", "v1")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtts://velamq.local:8883",
            "edge-dev-runtime",
        ))
        .with_algorithm(route)
        .with_data_config(data_config);
    let timestamp = Utc.with_ymd_and_hms(2026, 7, 22, 8, 30, 0).unwrap();
    let samples = vec![
        edge_core::TelemetrySample::new(
            "pump-1",
            "pressure",
            TelemetryValue::Float(12.0),
            DataQuality::Good,
            timestamp,
        ),
        edge_core::TelemetrySample::new(
            "pump-1",
            "pressure.matched",
            TelemetryValue::Float(12.0),
            DataQuality::Good,
            timestamp,
        ),
    ];

    let messages = build_data_config_mqtt_publish_messages(&package, &samples).unwrap();

    assert_eq!(messages.len(), 2);
    assert!(messages
        .iter()
        .any(|message| message.topic.ends_with("/matched-a")));
    assert!(messages
        .iter()
        .any(|message| message.topic.ends_with("/matched-b")));
    assert!(!messages
        .iter()
        .any(|message| message.topic.ends_with("/unmatched")));
    for message in messages {
        let payload: serde_json::Value = serde_json::from_slice(&message.payload).unwrap();
        assert_eq!(payload["values"]["matched"], 12.0);
        assert!(payload["values"].get("pressure").is_none());
        assert!(payload["values"].get("unmatched").is_none());
    }
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

#[tokio::test]
async fn mqtt_outbox_survives_failure_and_replays_multiple_topics_in_order() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("runtime.rocksdb");
    let message = |topic: &str| MqttPublishMessage {
        sink_id: "velamq-main".to_string(),
        broker: "mqtt://127.0.0.1:1883".to_string(),
        client_id: "edge-dev".to_string(),
        topic: topic.to_string(),
        qos: 1,
        payload: format!(r#"{{"topic":"{topic}"}}"#).into_bytes(),
    };

    {
        let store = RocksEdgeRuntimeStore::open(&db_path).unwrap();
        assert_eq!(
            store
                .enqueue_mqtt_message(message("factory/edge-dev/status"))
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .enqueue_mqtt_message(message("factory/edge-dev/telemetry"))
                .unwrap(),
            2
        );

        let mut unavailable = FailFirstPublisher::default();
        let error = flush_mqtt_outbox(&store, &mut unavailable)
            .await
            .expect_err("failed mqtt publish must retain the outbox");
        assert!(error
            .to_string()
            .contains("failed to publish queued mqtt message 1"));
        assert_eq!(store.mqtt_outbox_len().unwrap(), 2);
        let pending = store.pending_mqtt_messages(10).unwrap();
        assert_eq!(pending[0].attempts, 1);
        assert_eq!(
            pending[0].last_error.as_deref(),
            Some("simulated mqtt outage")
        );
        assert_eq!(pending[1].attempts, 0);
    }

    let reopened = RocksEdgeRuntimeStore::open(&db_path).unwrap();
    let mut publisher = RecordingMqttPublisher::default();
    assert_eq!(
        flush_mqtt_outbox(&reopened, &mut publisher).await.unwrap(),
        2
    );
    assert_eq!(
        publisher
            .messages()
            .iter()
            .map(|message| message.topic.as_str())
            .collect::<Vec<_>>(),
        vec!["factory/edge-dev/status", "factory/edge-dev/telemetry"]
    );
    assert_eq!(reopened.mqtt_outbox_len().unwrap(), 0);
    let acknowledgements = reopened.mqtt_publish_acknowledgements(10).unwrap();
    assert_eq!(acknowledgements.len(), 2);
    assert_eq!(acknowledgements[0].sequence, 1);
    assert_eq!(acknowledgements[0].sink_id, "velamq-main");
    assert_eq!(acknowledgements[0].topic, "factory/edge-dev/status");
    assert_eq!(acknowledgements[0].qos, 1);
    assert!(acknowledgements[0].payload_bytes > 0);
    assert_eq!(acknowledgements[1].sequence, 2);
    assert_eq!(acknowledgements[1].topic, "factory/edge-dev/telemetry");
    assert_eq!(
        reopened.mqtt_publish_acknowledgements(1).unwrap(),
        vec![acknowledgements[1].clone()]
    );
}

#[tokio::test]
async fn rumqttc_publisher_waits_for_qos_broker_confirmation() {
    for qos in [0, 1, 2] {
        let (broker, observed) = spawn_test_mqtt_broker(true).await;
        let uplink = MqttUplinkConfig::velamq("velamq-main", broker, format!("edge-qos-{qos}"))
            .with_qos(qos);
        let mut publisher = RumqttcMqttPublisher::connect_from_uplink_with_ack_timeout(
            &uplink,
            Duration::from_secs(2),
        )
        .unwrap();
        let topic = format!("factory/edge-dev/qos/{qos}");

        publisher
            .publish(MqttPublishMessage {
                sink_id: "velamq-main".to_string(),
                broker: uplink.broker.clone(),
                client_id: uplink.client_id.clone(),
                topic: topic.clone(),
                qos,
                payload: b"confirmed".to_vec(),
            })
            .await
            .unwrap();

        assert_eq!(
            observed.await.unwrap(),
            ObservedPublish {
                topic,
                qos,
                payload: b"confirmed".to_vec(),
            }
        );
    }
}

#[tokio::test]
async fn multi_broker_publisher_routes_each_sink_to_its_configured_broker() {
    let (primary_broker, primary_observed) = spawn_test_mqtt_broker(true).await;
    let (archive_broker, archive_observed) = spawn_test_mqtt_broker(true).await;
    let uplinks = vec![
        MqttUplinkConfig::velamq("primary", primary_broker, "edge-primary").with_qos(1),
        MqttUplinkConfig::velamq("archive", archive_broker, "edge-archive").with_qos(1),
    ];
    let mut publisher = MultiBrokerMqttPublisher::connect_from_uplinks_with_ack_timeout(
        &uplinks,
        Duration::from_secs(2),
    )
    .unwrap();

    publisher
        .publish(MqttPublishMessage {
            sink_id: "archive".to_string(),
            broker: uplinks[1].broker.clone(),
            client_id: uplinks[1].client_id.clone(),
            topic: "factory/archive/telemetry".to_string(),
            qos: 1,
            payload: b"archive".to_vec(),
        })
        .await
        .unwrap();
    publisher
        .publish(MqttPublishMessage {
            sink_id: "primary".to_string(),
            broker: uplinks[0].broker.clone(),
            client_id: uplinks[0].client_id.clone(),
            topic: "factory/live/telemetry".to_string(),
            qos: 1,
            payload: b"live".to_vec(),
        })
        .await
        .unwrap();

    assert_eq!(
        archive_observed.await.unwrap(),
        ObservedPublish {
            topic: "factory/archive/telemetry".to_string(),
            qos: 1,
            payload: b"archive".to_vec(),
        }
    );
    assert_eq!(
        primary_observed.await.unwrap(),
        ObservedPublish {
            topic: "factory/live/telemetry".to_string(),
            qos: 1,
            payload: b"live".to_vec(),
        }
    );
}

#[tokio::test]
async fn mqtt_publisher_rejects_messages_for_a_different_route() {
    let (broker, _observed) = spawn_test_mqtt_broker(true).await;
    let uplink = MqttUplinkConfig::velamq("primary", broker, "edge-primary");
    let mut publisher = RumqttcMqttPublisher::connect_from_uplink(&uplink).unwrap();

    let error = publisher
        .publish(MqttPublishMessage {
            sink_id: "archive".to_string(),
            broker: uplink.broker.clone(),
            client_id: uplink.client_id.clone(),
            topic: "factory/archive/telemetry".to_string(),
            qos: 1,
            payload: b"wrong route".to_vec(),
        })
        .await
        .unwrap_err();

    assert!(error.to_string().contains("does not match connected route"));
}

#[tokio::test]
async fn mqtt_tls_transport_builds_with_the_selected_crypto_provider() {
    let uplink =
        MqttUplinkConfig::velamq("tls-sink", "mqtts://127.0.0.1:1", "edge-tls-construction");

    let publisher = RumqttcMqttPublisher::connect_from_uplink(&uplink);

    assert!(publisher.is_ok());
}

#[tokio::test]
async fn dropping_mqtt_publisher_closes_its_background_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let broker = format!("mqtt://{}", listener.local_addr().unwrap());
    let (connected_tx, connected_rx) = oneshot::channel();
    let (closed_tx, closed_rx) = oneshot::channel();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let (connect_header, _) = read_mqtt_packet(&mut stream).await;
        assert_eq!(connect_header >> 4, 1);
        stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
        connected_tx.send(()).ok();

        let mut byte = [0_u8; 1];
        let closed = match stream.read(&mut byte).await {
            Ok(0) => true,
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => true,
            _ => false,
        };
        closed_tx.send(closed).ok();
    });
    let uplink = MqttUplinkConfig::velamq("velamq-main", broker, "edge-drop-test");
    let publisher = RumqttcMqttPublisher::connect_from_uplink(&uplink).unwrap();

    tokio::time::timeout(Duration::from_secs(2), connected_rx)
        .await
        .unwrap()
        .unwrap();
    drop(publisher);

    assert!(tokio::time::timeout(Duration::from_secs(2), closed_rx)
        .await
        .expect("publisher drop should close its MQTT connection")
        .unwrap());
}

#[tokio::test]
async fn broker_ack_timeout_keeps_message_in_rocksdb_outbox() {
    let (broker, observed) = spawn_test_mqtt_broker(false).await;
    let uplink = MqttUplinkConfig::velamq("velamq-main", broker, "edge-timeout").with_qos(1);
    let mut publisher = RumqttcMqttPublisher::connect_from_uplink_with_ack_timeout(
        &uplink,
        Duration::from_millis(100),
    )
    .unwrap();
    let dir = tempdir().unwrap();
    let store = RocksEdgeRuntimeStore::open(dir.path().join("runtime.rocksdb")).unwrap();
    store
        .enqueue_mqtt_message(MqttPublishMessage {
            sink_id: "velamq-main".to_string(),
            broker: uplink.broker.clone(),
            client_id: uplink.client_id.clone(),
            topic: "factory/edge-dev/unconfirmed".to_string(),
            qos: 1,
            payload: b"pending".to_vec(),
        })
        .unwrap();

    let error = flush_mqtt_outbox(&store, &mut publisher)
        .await
        .expect_err("unacknowledged publish must remain queued");

    assert!(error
        .to_string()
        .contains("failed to publish queued mqtt message 1"));
    assert_eq!(
        observed.await.unwrap().topic,
        "factory/edge-dev/unconfirmed"
    );
    assert_eq!(store.mqtt_outbox_len().unwrap(), 1);
    let pending = store.pending_mqtt_messages(1).unwrap();
    assert_eq!(pending[0].attempts, 1);
    assert!(pending[0]
        .last_error
        .as_deref()
        .is_some_and(|error| error.contains("acknowledgement timed out")));
}

#[derive(Debug, PartialEq, Eq)]
struct ObservedPublish {
    topic: String,
    qos: u8,
    payload: Vec<u8>,
}

async fn spawn_test_mqtt_broker(acknowledge: bool) -> (String, oneshot::Receiver<ObservedPublish>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let broker = format!("mqtt://{}", listener.local_addr().unwrap());
    let (observed_tx, observed_rx) = oneshot::channel();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let (connect_header, _) = read_mqtt_packet(&mut stream).await;
        assert_eq!(connect_header >> 4, 1);
        stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();

        let (publish_header, body) = read_mqtt_packet(&mut stream).await;
        assert_eq!(publish_header >> 4, 3);
        let qos = (publish_header >> 1) & 0x03;
        let topic_len = usize::from(u16::from_be_bytes([body[0], body[1]]));
        let topic = String::from_utf8(body[2..2 + topic_len].to_vec()).unwrap();
        let mut payload_start = 2 + topic_len;
        let packet_id = if qos > 0 {
            let packet_id = u16::from_be_bytes([body[payload_start], body[payload_start + 1]]);
            payload_start += 2;
            packet_id
        } else {
            0
        };
        observed_tx
            .send(ObservedPublish {
                topic,
                qos,
                payload: body[payload_start..].to_vec(),
            })
            .ok();

        if !acknowledge {
            tokio::time::sleep(Duration::from_millis(500)).await;
            return;
        }
        match qos {
            0 => {}
            1 => {
                stream
                    .write_all(&[0x40, 0x02, (packet_id >> 8) as u8, packet_id as u8])
                    .await
                    .unwrap();
            }
            2 => {
                stream
                    .write_all(&[0x50, 0x02, (packet_id >> 8) as u8, packet_id as u8])
                    .await
                    .unwrap();
                let (pubrel_header, pubrel_body) = read_mqtt_packet(&mut stream).await;
                assert_eq!(pubrel_header, 0x62);
                assert_eq!(pubrel_body, packet_id.to_be_bytes());
                stream
                    .write_all(&[0x70, 0x02, (packet_id >> 8) as u8, packet_id as u8])
                    .await
                    .unwrap();
            }
            _ => panic!("unexpected qos {qos}"),
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    });

    (broker, observed_rx)
}

async fn read_mqtt_packet(stream: &mut TcpStream) -> (u8, Vec<u8>) {
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
use std::time::Duration;
