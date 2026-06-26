use std::sync::{Arc, Mutex};

use chrono::Utc;
use cloud_control::{CloudControlStore, EdgeNode, ReleaseService};
use edge_core::{
    AlgorithmRuntime, AlgorithmSpec, CloudSyncMetrics, CollectionRuntimeMetrics, CollectionTask,
    CommandRisk, CommandSpec, DeviceInstance, DeviceSpec, EdgeConfigPackage, EdgeHealth,
    EdgeRuntimeMetricsSnapshot, EventSeverity, EventSpec, LocalStoreMetrics, NumberRange,
    PointAddress, ProtocolConnection, ProtocolRuntimeMetrics, ProtocolType, SystemRuntimeMetrics,
    TelemetryPoint, TelemetryPointMapping, TelemetryType,
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

        let pump_model = DeviceSpec::new("pump", "v1")
            .with_telemetry(vec![
                TelemetryPoint::new("pressure", TelemetryType::Float)
                    .with_unit("MPa")
                    .with_range(NumberRange::new(0.0, 20.0))
                    .with_description("泵出口压力"),
                TelemetryPoint::new("running", TelemetryType::Boolean)
                    .with_description("设备运行布尔量"),
            ])
            .with_commands(vec![CommandSpec::new("start", CommandRisk::Medium)])
            .with_events(vec![EventSpec {
                id: "pressure_high".to_string(),
                severity: EventSeverity::Warning,
            }]);

        let mut package = EdgeConfigPackage::new("edge-dev", "2026.06.26-001")
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
        package.device_models.push(pump_model.clone());
        package.algorithms.push(AlgorithmSpec {
            id: "pump-anomaly-v1".to_string(),
            version: "1.0.0".to_string(),
            runtime: AlgorithmRuntime::Onnx,
            inputs: vec!["pressure".to_string(), "running".to_string()],
            outputs: vec!["pump.anomaly_score".to_string()],
        });
        store.upsert_device_model(pump_model);

        let release = ReleaseService::create_release(&mut store, package)
            .expect("demo config package should be valid");
        ReleaseService::mark_reported(&mut store, release.release_id, "2026.06.26-001")
            .expect("demo release should be reportable");

        store.upsert_runtime_metrics(EdgeRuntimeMetricsSnapshot {
            edge_id: "edge-dev".to_string(),
            runtime_id: "runtime-dev".to_string(),
            config_version: "2026.06.26-001".to_string(),
            timestamp: Utc::now(),
            health: EdgeHealth::Healthy,
            system: SystemRuntimeMetrics {
                cpu_percent: 18.5,
                memory_percent: 42.0,
                disk_percent: 61.0,
                process_uptime_seconds: 3600,
            },
            collection: CollectionRuntimeMetrics {
                active_task_count: 1,
                success_rate: 0.995,
                average_latency_ms: 24,
                bad_point_count: 0,
            },
            protocols: vec![ProtocolRuntimeMetrics {
                connection_id: "modbus-line-a".to_string(),
                protocol: "Modbus TCP".to_string(),
                connected: true,
                latency_ms: 12,
                timeout_count: 0,
                error_count: 0,
                reconnect_count: 0,
            }],
            local_store: LocalStoreMetrics {
                backend: "jsonl".to_string(),
                buffered_records: 0,
                oldest_buffer_age_seconds: 0,
                disk_usage_percent: 35.0,
            },
            algorithms: Vec::new(),
            cloud_sync: CloudSyncMetrics {
                connected: true,
                last_sync_seconds_ago: 8,
                pending_uploads: 0,
                desired_version: "2026.06.26-001".to_string(),
                reported_version: "2026.06.26-001".to_string(),
            },
        });

        Self {
            store: Arc::new(Mutex::new(store)),
        }
    }
}
