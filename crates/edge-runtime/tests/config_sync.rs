use async_trait::async_trait;
use edge_core::{
    CollectionTask, DeviceInstance, EdgeConfigPackage, PointAddress, ProtocolConnection,
    TelemetryPointMapping, TelemetryType,
};
use edge_runtime::{sync_once, EdgeConfigSyncClient, EdgeDesiredConfig};

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
