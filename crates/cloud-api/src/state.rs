use std::sync::{Arc, Mutex};

use cloud_control::{CloudControlStore, EdgeNode, ReleaseService};
use edge_core::{
    CollectionTask, DeviceInstance, EdgeConfigPackage, PointAddress, ProtocolConnection,
    ProtocolType, TelemetryPointMapping, TelemetryType,
};

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Mutex<CloudControlStore>>,
}

impl Default for AppState {
    fn default() -> Self {
        let mut store = CloudControlStore::default();

        store.register_edge(
            EdgeNode::new("edge-dev", "研发实验室边端")
                .at_site("研发/实验室")
                .with_capability("protocol:modbus-tcp")
                .with_capability("local-store:jsonl"),
        );

        let package = EdgeConfigPackage::new("edge-dev", "2026.06.26-001")
            .with_device(DeviceInstance::new("pump-1", "pump"))
            .with_protocol_connection(ProtocolConnection {
                connection_id: "modbus-line-a".to_string(),
                protocol: ProtocolType::ModbusTcp,
                endpoint: Some("10.12.0.20:502".to_string()),
            })
            .with_point_mapping(
                TelemetryPointMapping::new(
                    "pressure",
                    "pump-1",
                    "pump.pressure",
                    "modbus-line-a",
                    PointAddress::modbus_holding_register(40001),
                    TelemetryType::Float,
                )
                .with_unit("MPa")
                .with_interval_ms(1000),
            )
            .with_point_mapping(
                TelemetryPointMapping::new(
                    "running",
                    "pump-1",
                    "pump.running",
                    "modbus-line-a",
                    PointAddress {
                        kind: "coil".to_string(),
                        value: "00001".to_string(),
                    },
                    TelemetryType::Boolean,
                )
                .with_interval_ms(1000),
            )
            .with_collection_task(CollectionTask::interval(
                "pump-main",
                "pump-1",
                vec!["pressure".to_string(), "running".to_string()],
                1000,
            ));

        let release = ReleaseService::create_release(&mut store, package)
            .expect("demo config package should be valid");
        ReleaseService::mark_reported(&mut store, release.release_id, "2026.06.26-001")
            .expect("demo release should be reportable");

        Self {
            store: Arc::new(Mutex::new(store)),
        }
    }
}
