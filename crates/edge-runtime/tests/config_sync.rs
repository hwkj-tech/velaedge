use async_trait::async_trait;
use edge_core::{
    CollectionTask, DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint,
    DataConfigPublish, DeviceInstance, EdgeConfigPackage, EdgeRuntimeEvent,
    EdgeRuntimeMetricsSnapshot, MqttUplinkConfig, PointAddress, ProtocolConnection,
    TelemetryPointMapping, TelemetryType,
};
use edge_runtime::{
    sync_and_report_once, sync_and_report_with_mqtt_publisher_once, sync_once,
    EdgeConfigSyncClient, EdgeDesiredConfig, RecordingMqttPublisher, RuntimeStatusReporter,
};

fn package() -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", "2026.06.26-002")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
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

struct MemorySyncClient {
    desired: EdgeDesiredConfig,
    reported: Vec<(String, String)>,
}

#[derive(Default)]
struct MemoryRuntimeReporter {
    metrics: Vec<EdgeRuntimeMetricsSnapshot>,
}

#[async_trait]
impl EdgeConfigSyncClient for MemorySyncClient {
    async fn fetch_desired_config(&mut self, edge_id: &str) -> anyhow::Result<EdgeDesiredConfig> {
        assert_eq!(edge_id, "edge-dev");
        Ok(self.desired.clone())
    }

    async fn report_applied_version(&mut self, edge_id: &str, version: &str) -> anyhow::Result<()> {
        self.reported
            .push((edge_id.to_string(), version.to_string()));
        Ok(())
    }
}

#[async_trait]
impl RuntimeStatusReporter for MemoryRuntimeReporter {
    async fn report_metrics(&mut self, snapshot: EdgeRuntimeMetricsSnapshot) -> anyhow::Result<()> {
        self.metrics.push(snapshot);
        Ok(())
    }

    async fn report_event(&mut self, _event: EdgeRuntimeEvent) -> anyhow::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn sync_once_applies_desired_config_collects_and_reports_version() {
    let mut client = MemorySyncClient {
        desired: EdgeDesiredConfig {
            desired_version: "2026.06.26-002".to_string(),
            package: package(),
        },
        reported: Vec::new(),
    };

    let report = sync_once("edge-dev", &mut client).await.unwrap();

    assert_eq!(report.applied_version, "2026.06.26-002");
    assert_eq!(report.samples_collected, 1);
    assert_eq!(
        client.reported,
        vec![("edge-dev".to_string(), "2026.06.26-002".to_string())]
    );
}

#[tokio::test]
async fn sync_and_report_once_applies_config_reports_version_and_runtime_metrics() {
    let mut client = MemorySyncClient {
        desired: EdgeDesiredConfig {
            desired_version: "2026.06.26-002".to_string(),
            package: package(),
        },
        reported: Vec::new(),
    };
    let mut reporter = MemoryRuntimeReporter::default();

    let report = sync_and_report_once("edge-dev", "runtime-sync", &mut client, &mut reporter)
        .await
        .unwrap();

    assert_eq!(report.applied_version, "2026.06.26-002");
    assert_eq!(report.samples_collected, 1);
    assert_eq!(
        client.reported,
        vec![("edge-dev".to_string(), "2026.06.26-002".to_string())]
    );
    assert_eq!(reporter.metrics.len(), 1);
    assert_eq!(reporter.metrics[0].edge_id, "edge-dev");
    assert_eq!(reporter.metrics[0].runtime_id, "runtime-sync");
    assert_eq!(reporter.metrics[0].config_version, "2026.06.26-002");
}

#[tokio::test]
async fn sync_and_report_with_mqtt_publisher_uploads_collected_samples() {
    let mut package = package();
    package.mqtt_uplinks.push(
        MqttUplinkConfig::velamq("velamq-main", "mqtt://velamq.local:1883", "edge-dev")
            .with_topic_template("velamq/{edge_id}/{device_id}/{telemetry_id}"),
    );
    let mut client = MemorySyncClient {
        desired: EdgeDesiredConfig {
            desired_version: "2026.06.26-002".to_string(),
            package,
        },
        reported: Vec::new(),
    };
    let mut reporter = MemoryRuntimeReporter::default();
    let mut mqtt = RecordingMqttPublisher::default();

    let report = sync_and_report_with_mqtt_publisher_once(
        "edge-dev",
        "runtime-sync",
        &mut client,
        &mut reporter,
        &mut mqtt,
    )
    .await
    .unwrap();

    assert_eq!(report.applied_version, "2026.06.26-002");
    assert_eq!(report.samples_collected, 1);
    assert_eq!(report.mqtt_messages_published, 1);
    assert_eq!(mqtt.messages().len(), 1);
    assert_eq!(mqtt.messages()[0].topic, "velamq/edge-dev/pump-1/pressure");
    assert_eq!(reporter.metrics.len(), 1);
}

#[tokio::test]
async fn sync_and_report_with_mqtt_publisher_prefers_data_config_json_topics() {
    let mut package = package();
    package.mqtt_uplinks.push(
        MqttUplinkConfig::velamq("velamq-main", "mqtt://velamq.local:1883", "edge-dev")
            .with_topic_template("legacy/{edge_id}/{device_id}/{telemetry_id}"),
    );
    package.data_configs.push(
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
    );
    let mut client = MemorySyncClient {
        desired: EdgeDesiredConfig {
            desired_version: "2026.06.26-002".to_string(),
            package,
        },
        reported: Vec::new(),
    };
    let mut reporter = MemoryRuntimeReporter::default();
    let mut mqtt = RecordingMqttPublisher::default();

    let report = sync_and_report_with_mqtt_publisher_once(
        "edge-dev",
        "runtime-sync",
        &mut client,
        &mut reporter,
        &mut mqtt,
    )
    .await
    .unwrap();

    assert_eq!(report.applied_version, "2026.06.26-002");
    assert_eq!(report.samples_collected, 1);
    assert_eq!(report.mqtt_messages_published, 1);
    assert_eq!(mqtt.messages().len(), 1);
    assert_eq!(mqtt.messages()[0].topic, "factory/edge-dev/pump-1/status");
    let payload: serde_json::Value = serde_json::from_slice(&mqtt.messages()[0].payload).unwrap();
    assert_eq!(payload["config_id"], "pump_status");
    assert_eq!(payload["values"]["pressure"], 1.0);
    assert_eq!(reporter.metrics.len(), 1);
}
