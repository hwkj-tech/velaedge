use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use edge_core::{
    AlgorithmDsl, AlgorithmInputBinding, AlgorithmKind, AlgorithmOutput, AlgorithmReportMode,
    AlgorithmReportPolicy, AlgorithmSpec, AlgorithmStep, AlgorithmTrigger, CollectionTask,
    DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint, DataConfigPublish,
    DataQuality, DataQualityCode, DeviceInstance, EdgeConfigPackage, MqttUplinkConfig, NumberRange,
    PointAddress, ProtocolCircuitBreakerConfig, ProtocolCircuitState, ProtocolConnection,
    SerialConnectionSettings, TelemetryPointMapping, TelemetryType, TelemetryValue,
};
use edge_runtime::{
    append_modbus_rtu_crc, ConfiguredEdgeRuntime, ProtocolCircuitBreakerRegistry,
    RecordingMqttPublisher, ScriptedSerialBus, ScriptedSerialBusFactory, SerialBus,
    SerialBusFactory,
};

#[derive(Clone)]
struct TimeoutThenResponseBus {
    attempts: Arc<AtomicUsize>,
    response: Vec<u8>,
}

#[async_trait]
impl SerialBus for TimeoutThenResponseBus {
    async fn transact(&mut self, _request: &[u8]) -> Result<Vec<u8>> {
        if self.attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        Ok(self.response.clone())
    }
}

struct TimeoutThenResponseFactory {
    connection_id: String,
    bus: TimeoutThenResponseBus,
}

impl SerialBusFactory for TimeoutThenResponseFactory {
    fn open(&mut self, connection: &ProtocolConnection) -> Result<Box<dyn SerialBus>> {
        anyhow::ensure!(connection.connection_id == self.connection_id);
        Ok(Box::new(self.bus.clone()))
    }
}

fn modbus_package() -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", "2026.06.28-modbus")
        .with_device(DeviceInstance::new("meter-1", "power-meter"))
        .with_protocol_connection(ProtocolConnection::modbus_rtu_serial(
            "meter-rs485-bus-1",
            SerialConnectionSettings::new("/dev/ttyUSB0", 9600),
        ))
        .with_mqtt_uplink(
            MqttUplinkConfig::velamq("velamq-main", "mqtt://velamq.local:1883", "edge-dev")
                .with_topic_template("velamq/{edge_id}/{device_id}/{telemetry_id}"),
        )
        .with_point_mapping(TelemetryPointMapping::new(
            "voltage",
            "meter-1",
            "voltage",
            "meter-rs485-bus-1",
            PointAddress::modbus_holding_register(40001),
            TelemetryType::Integer,
        ))
        .with_collection_task(CollectionTask::interval(
            "meter-main",
            "meter-1",
            vec!["voltage".to_string()],
            1000,
        ))
}

#[tokio::test]
async fn configured_runtime_collects_modbus_rtu_points_from_cloud_package() {
    let bus = ScriptedSerialBus::new(vec![response(1, &[220])]);
    let observed_bus = bus.clone();
    let factory = ScriptedSerialBusFactory::new(vec![("meter-rs485-bus-1".to_string(), bus)]);
    let mut runtime = ConfiguredEdgeRuntime::new(modbus_package(), factory).unwrap();

    let report = runtime.collect_once().await.unwrap();

    assert_eq!(report.samples_collected, 1);
    assert_eq!(
        runtime.shadow("meter-1").unwrap().latest_value("voltage"),
        Some(&TelemetryValue::Integer(220))
    );
    assert_eq!(&observed_bus.requests()[0][..6], &[1, 0x03, 0, 0, 0, 1]);
    let protocol = runtime.protocol_runtime_metrics();
    assert_eq!(protocol.len(), 1);
    assert!(protocol[0].connected);
    assert!(protocol[0].latency_ms >= 1);
    assert_eq!(protocol[0].error_count, 0);
    assert_eq!(protocol[0].last_quality_code, Some(DataQualityCode::Good));
    assert_eq!(protocol[0].good_value_count, 1);
}

#[tokio::test]
async fn configured_runtime_marks_modbus_values_outside_cloud_range_as_uncertain() {
    let mut package = modbus_package();
    package.point_mappings[0].range = Some(NumberRange::new(0.0, 220.0));
    let bus = ScriptedSerialBus::new(vec![response(1, &[221])]);
    let factory = ScriptedSerialBusFactory::new(vec![("meter-rs485-bus-1".to_string(), bus)]);
    let mut runtime = ConfiguredEdgeRuntime::new(package, factory).unwrap();

    runtime.collect_once().await.unwrap();

    let sample = runtime
        .shadow("meter-1")
        .unwrap()
        .latest("voltage")
        .unwrap();
    assert_eq!(sample.quality, DataQuality::Uncertain);
    assert_eq!(
        sample.quality_code,
        Some(DataQualityCode::UncertainOutOfRange)
    );
    let protocol = runtime.protocol_runtime_metrics();
    assert_eq!(
        protocol[0].last_quality_code,
        Some(DataQualityCode::UncertainOutOfRange)
    );
    assert_eq!(protocol[0].uncertain_value_count, 1);
    assert_eq!(protocol[0].bad_value_count, 0);
}

#[test]
fn configured_runtime_rejects_collection_task_with_unknown_point() {
    let package = modbus_package().with_collection_task(CollectionTask::interval(
        "broken-task",
        "meter-1",
        vec!["missing_voltage".to_string()],
        1000,
    ));
    let factory = ScriptedSerialBusFactory::new(Vec::new());

    let error = match ConfiguredEdgeRuntime::new(package, factory) {
        Ok(_) => panic!("invalid runtime config rejected"),
        Err(error) => error,
    };

    assert!(error
        .to_string()
        .contains("collection task broken-task references missing point missing_voltage"));
}

#[tokio::test]
async fn configured_runtime_publishes_modbus_samples_to_mqtt_uplink() {
    let bus = ScriptedSerialBus::new(vec![response(1, &[220])]);
    let factory = ScriptedSerialBusFactory::new(vec![("meter-rs485-bus-1".to_string(), bus)]);
    let mut runtime = ConfiguredEdgeRuntime::new(modbus_package(), factory).unwrap();
    let mut publisher = RecordingMqttPublisher::default();

    let report = runtime
        .collect_once_and_publish_mqtt(&mut publisher)
        .await
        .unwrap();

    assert_eq!(report.collection.samples_collected, 1);
    assert_eq!(report.mqtt_messages_published, 1);
    assert_eq!(
        publisher.messages()[0].topic,
        "velamq/edge-dev/meter-1/voltage"
    );
}

#[tokio::test]
async fn configured_runtime_publishes_one_mqtt_message_per_data_config() {
    let package = package_with_two_modbus_data_configs();
    let bus_factory = ScriptedSerialBusFactory::new(vec![(
        "meter-rs485-bus-1".to_string(),
        ScriptedSerialBus::new(vec![
            response(1, &[220, 1, 61]),
            response(1, &[1290, 19, 7]),
        ]),
    )]);
    let mut runtime = ConfiguredEdgeRuntime::new(package, bus_factory).unwrap();
    let mut publisher = RecordingMqttPublisher::default();

    let report = runtime
        .collect_data_configs_once_and_publish_mqtt(&mut publisher)
        .await
        .unwrap();

    assert_eq!(report.collection.samples_collected, 6);
    assert_eq!(report.mqtt_messages_published, 2);
    assert_eq!(publisher.messages().len(), 2);
    assert_eq!(
        publisher.messages()[0].topic,
        "velamq/edge-dev/meter-1/status"
    );
    assert_eq!(
        publisher.messages()[1].topic,
        "velamq/edge-dev/meter-1/energy"
    );

    let status_payload: serde_json::Value =
        serde_json::from_slice(&publisher.messages()[0].payload).unwrap();
    assert_eq!(status_payload["values"]["voltage"], 220);
    assert_eq!(status_payload["values"]["running"], true);
}

#[tokio::test]
async fn data_config_timeout_reopens_modbus_connection_and_recovers() {
    let mut package = package_with_two_modbus_data_configs();
    package.data_configs.truncate(1);
    package.data_configs[0].collection = DataConfigCollection::new(1000)
        .with_timeout_ms(5)
        .with_retry_count(1);
    let attempts = Arc::new(AtomicUsize::new(0));
    let factory = TimeoutThenResponseFactory {
        connection_id: "meter-rs485-bus-1".to_string(),
        bus: TimeoutThenResponseBus {
            attempts: attempts.clone(),
            response: response(1, &[220, 1, 61]),
        },
    };
    let mut runtime = ConfiguredEdgeRuntime::new(package, factory).unwrap();
    let mut publisher = RecordingMqttPublisher::default();

    let report = runtime
        .collect_data_configs_once_and_publish_mqtt(&mut publisher)
        .await
        .unwrap();

    assert_eq!(report.collection.samples_collected, 3);
    assert_eq!(report.mqtt_messages_published, 1);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    let metrics = runtime.protocol_runtime_metrics();
    assert!(metrics[0].connected);
    assert_eq!(metrics[0].error_count, 1);
    assert_eq!(metrics[0].timeout_count, 1);
    assert_eq!(metrics[0].reconnect_count, 1);
}

#[tokio::test]
async fn protocol_circuit_breaker_suppresses_io_and_recovers_with_a_half_open_probe() {
    let mut package = modbus_package();
    package.protocol_connections[0].circuit_breaker = ProtocolCircuitBreakerConfig {
        enabled: true,
        failure_threshold: 1,
        open_duration_ms: 100,
        half_open_success_threshold: 1,
    };
    let bus = ScriptedSerialBus::new(vec![Vec::new(), response(1, &[221])]);
    let observed_bus = bus.clone();
    let factory = ScriptedSerialBusFactory::new(vec![("meter-rs485-bus-1".to_string(), bus)]);
    let mut runtime = ConfiguredEdgeRuntime::new(package, factory).unwrap();

    assert!(runtime.collect_once().await.is_err());
    let first_metrics = runtime.protocol_runtime_metrics();
    assert_eq!(first_metrics[0].circuit_state, ProtocolCircuitState::Open);
    assert_eq!(first_metrics[0].circuit_open_count, 1);

    let rejected = runtime.collect_once().await.unwrap_err();
    assert!(rejected.to_string().contains("circuit breaker is open"));
    assert_eq!(observed_bus.requests().len(), 1);
    assert_eq!(
        runtime.protocol_runtime_metrics()[0].circuit_rejected_count,
        1
    );

    tokio::time::sleep(Duration::from_millis(110)).await;
    let report = runtime.collect_once().await.unwrap();

    assert_eq!(report.samples_collected, 1);
    assert_eq!(observed_bus.requests().len(), 2);
    let recovered = runtime.protocol_runtime_metrics();
    assert!(recovered[0].connected);
    assert_eq!(recovered[0].circuit_state, ProtocolCircuitState::Closed);
    assert_eq!(recovered[0].consecutive_failure_count, 0);
    assert_eq!(
        runtime.shadow("meter-1").unwrap().latest_value("voltage"),
        Some(&TelemetryValue::Integer(221))
    );
}

#[tokio::test]
async fn protocol_circuit_breaker_state_is_shared_across_rebuilt_runtimes() {
    let mut package = modbus_package();
    package.protocol_connections[0].circuit_breaker = ProtocolCircuitBreakerConfig {
        enabled: true,
        failure_threshold: 1,
        open_duration_ms: 100,
        half_open_success_threshold: 1,
    };
    let circuit_breakers = ProtocolCircuitBreakerRegistry::default();
    let failing_bus = ScriptedSerialBus::new(vec![Vec::new()]);
    let mut first_runtime = ConfiguredEdgeRuntime::new_with_circuit_breakers(
        package.clone(),
        ScriptedSerialBusFactory::new(vec![("meter-rs485-bus-1".to_string(), failing_bus)]),
        circuit_breakers.clone(),
    )
    .unwrap();

    assert!(first_runtime.collect_once().await.is_err());

    let recovering_bus = ScriptedSerialBus::new(vec![response(1, &[222])]);
    let observed_recovering_bus = recovering_bus.clone();
    let mut rebuilt_runtime = ConfiguredEdgeRuntime::new_with_circuit_breakers(
        package,
        ScriptedSerialBusFactory::new(vec![("meter-rs485-bus-1".to_string(), recovering_bus)]),
        circuit_breakers,
    )
    .unwrap();

    assert_eq!(
        rebuilt_runtime.protocol_runtime_metrics()[0].circuit_state,
        ProtocolCircuitState::Open
    );
    let rejected = rebuilt_runtime.collect_once().await.unwrap_err();
    assert!(rejected.to_string().contains("circuit breaker is open"));
    assert!(observed_recovering_bus.requests().is_empty());

    tokio::time::sleep(Duration::from_millis(110)).await;
    assert_eq!(
        rebuilt_runtime
            .collect_once()
            .await
            .unwrap()
            .samples_collected,
        1
    );
    assert_eq!(observed_recovering_bus.requests().len(), 1);
    assert_eq!(
        rebuilt_runtime.protocol_runtime_metrics()[0].circuit_state,
        ProtocolCircuitState::Closed
    );
}

#[tokio::test]
async fn configured_runtime_publishes_algorithm_virtual_points_to_mqtt_uplink() {
    let package = EdgeConfigPackage::new("edge-dev", "2026.06.28-dsl")
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
        .with_collection_task(CollectionTask::interval(
            "pump-main",
            "pump-1",
            vec!["pressure".to_string()],
            1000,
        ))
        .with_algorithm(AlgorithmSpec::dsl(
            "pressure-change",
            "v1",
            AlgorithmKind::ChangeReport,
            AlgorithmDsl {
                inputs: vec![AlgorithmInputBinding::new("p", "pressure")],
                trigger: AlgorithmTrigger::on_sample(),
                steps: vec![AlgorithmStep::change_filter("p", 0.1)],
                outputs: vec![AlgorithmOutput::virtual_point("p", "pressure.reported")],
                report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnChange, "velamq-main"),
            },
        ));
    let mut runtime = ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(vec![]))
        .expect("runtime builds");
    let mut publisher = RecordingMqttPublisher::default();

    let report = runtime
        .collect_once_and_publish_mqtt(&mut publisher)
        .await
        .unwrap();

    assert_eq!(report.collection.samples_collected, 2);
    assert_eq!(report.mqtt_messages_published, 2);
    assert_eq!(
        publisher.messages()[1].topic,
        "velamq/edge-dev/pump-1/pressure.reported"
    );
    let algorithms = runtime.algorithm_runtime_metrics();
    assert_eq!(algorithms.len(), 1);
    assert_eq!(algorithms[0].algorithm_id, "pressure-change");
    assert!(algorithms[0].healthy);
    assert!(algorithms[0].last_run_latency_ms >= 1);
}

#[tokio::test]
async fn configured_runtime_publishes_data_config_with_algorithm_outputs() {
    let package = EdgeConfigPackage::new("edge-dev", "2026.07.01-data-dsl")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_mqtt_uplink(
            MqttUplinkConfig::velamq("velamq-main", "mqtt://velamq.local:1883", "edge-dev")
                .with_topic_template("unused/{edge_id}/{device_id}/{telemetry_id}"),
        )
        .with_point_mapping(TelemetryPointMapping::new(
            "pressure",
            "pump-1",
            "pressure",
            "sim-main",
            PointAddress::simulated("pressure"),
            TelemetryType::Float,
        ))
        .with_data_config(
            DataConfig::new(
                "pump_status",
                "泵运行状态上报",
                "pump-1",
                "sim-main",
                DataConfigCollection::new(1000),
                DataConfigPublish::new(
                    "velamq-main",
                    "velamq/{edge_id}/{device_id}/status",
                    DataConfigPayload::object(),
                ),
            )
            .with_point(DataConfigPoint::new(
                "pressure",
                "pump.pressure",
                PointAddress::simulated("pressure"),
                TelemetryType::Float,
                "pressure",
            ))
            .with_algorithm("pressure-change"),
        )
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
        ));
    let mut runtime = ConfiguredEdgeRuntime::new(package, ScriptedSerialBusFactory::new(vec![]))
        .expect("runtime builds");
    let mut publisher = RecordingMqttPublisher::default();

    let report = runtime
        .collect_data_configs_once_and_publish_mqtt(&mut publisher)
        .await
        .unwrap();

    assert_eq!(report.collection.samples_collected, 2);
    assert_eq!(report.mqtt_messages_published, 1);
    let payload: serde_json::Value =
        serde_json::from_slice(&publisher.messages()[0].payload).unwrap();
    let pressure = payload["values"]["pressure"]
        .as_f64()
        .expect("simulated pressure should be numeric");
    assert!((2.22..=2.58).contains(&pressure));
    assert_eq!(
        payload["values"]["pressureReported"].as_f64(),
        Some(pressure)
    );
}

fn response(slave_id: u8, registers: &[u16]) -> Vec<u8> {
    let mut frame = vec![slave_id, 0x03, (registers.len() * 2) as u8];
    for register in registers {
        frame.extend(register.to_be_bytes());
    }
    append_modbus_rtu_crc(&mut frame);
    frame
}

fn package_with_two_modbus_data_configs() -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", "2026.06.30-data-config")
        .with_device(DeviceInstance::new("meter-1", "power-meter"))
        .with_protocol_connection(ProtocolConnection::modbus_rtu_serial(
            "meter-rs485-bus-1",
            SerialConnectionSettings::new("/dev/ttyUSB0", 9600),
        ))
        .with_mqtt_uplink(
            MqttUplinkConfig::velamq("velamq-main", "mqtt://velamq.local:1883", "edge-dev")
                .with_topic_template("unused/{edge_id}/{device_id}/{telemetry_id}"),
        )
        .with_point_mapping(TelemetryPointMapping::new(
            "voltage",
            "meter-1",
            "meter.voltage",
            "meter-rs485-bus-1",
            PointAddress::modbus_holding_register(40001),
            TelemetryType::Integer,
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "running",
            "meter-1",
            "meter.running",
            "meter-rs485-bus-1",
            PointAddress::modbus_holding_register(40002),
            TelemetryType::Boolean,
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "load",
            "meter-1",
            "meter.load",
            "meter-rs485-bus-1",
            PointAddress::modbus_holding_register(40003),
            TelemetryType::Integer,
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "energy_total",
            "meter-1",
            "meter.energy_total",
            "meter-rs485-bus-1",
            PointAddress::modbus_holding_register(40101),
            TelemetryType::Integer,
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "current_a",
            "meter-1",
            "meter.current_a",
            "meter-rs485-bus-1",
            PointAddress::modbus_holding_register(40102),
            TelemetryType::Integer,
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "current_b",
            "meter-1",
            "meter.current_b",
            "meter-rs485-bus-1",
            PointAddress::modbus_holding_register(40103),
            TelemetryType::Integer,
        ))
        .with_data_config(
            DataConfig::new(
                "meter_status",
                "电表状态",
                "meter-1",
                "meter-rs485-bus-1",
                DataConfigCollection::new(1000),
                DataConfigPublish::new(
                    "velamq-main",
                    "velamq/{edge_id}/{device_id}/status",
                    DataConfigPayload::object(),
                ),
            )
            .with_point(DataConfigPoint::new(
                "voltage",
                "meter.voltage",
                PointAddress::modbus_holding_register(40001),
                TelemetryType::Integer,
                "voltage",
            ))
            .with_point(DataConfigPoint::new(
                "running",
                "meter.running",
                PointAddress::modbus_holding_register(40002),
                TelemetryType::Boolean,
                "running",
            ))
            .with_point(DataConfigPoint::new(
                "load",
                "meter.load",
                PointAddress::modbus_holding_register(40003),
                TelemetryType::Integer,
                "load",
            )),
        )
        .with_data_config(
            DataConfig::new(
                "meter_energy",
                "电表能耗",
                "meter-1",
                "meter-rs485-bus-1",
                DataConfigCollection::new(5000),
                DataConfigPublish::new(
                    "velamq-main",
                    "velamq/{edge_id}/{device_id}/energy",
                    DataConfigPayload::object(),
                ),
            )
            .with_point(DataConfigPoint::new(
                "energy_total",
                "meter.energy_total",
                PointAddress::modbus_holding_register(40101),
                TelemetryType::Integer,
                "energyTotal",
            ))
            .with_point(DataConfigPoint::new(
                "current_a",
                "meter.current_a",
                PointAddress::modbus_holding_register(40102),
                TelemetryType::Integer,
                "currentA",
            ))
            .with_point(DataConfigPoint::new(
                "current_b",
                "meter.current_b",
                PointAddress::modbus_holding_register(40103),
                TelemetryType::Integer,
                "currentB",
            )),
        )
}
