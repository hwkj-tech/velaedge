use std::{collections::BTreeSet, time::Duration};

use edge_core::{
    DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint, DataConfigPublish,
    DeviceInstance, EdgeConfigPackage, MqttUplinkConfig, PointAddress, ProtocolConnection,
    TelemetryPointMapping, TelemetryType,
};
use edge_runtime::{
    run_field_endurance_acceptance, DynamicFloatPoint, FieldDeviceIdentity, FieldEnduranceOptions,
    FieldEnduranceStatus, ModbusTcpSimulator, ModbusTcpSimulatorOptions,
};
use tempfile::tempdir;

fn simulated_package() -> EdgeConfigPackage {
    let address = PointAddress::simulated("pressure");
    EdgeConfigPackage::new("field-lab-edge", "acceptance-v1")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_mqtt_uplink(
            MqttUplinkConfig::velamq("recording-main", "mqtt://127.0.0.1:1883", "field-lab-edge")
                .with_qos(1),
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
                    "recording-main",
                    "acceptance/{edge_id}/{device_id}/telemetry",
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

fn modbus_tcp_package(endpoint: String) -> EdgeConfigPackage {
    let address = PointAddress::modbus_holding_register(40011);
    EdgeConfigPackage::new("field-tcp-edge", "acceptance-v1")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::modbus_tcp("modbus-main", endpoint))
        .with_mqtt_uplink(
            MqttUplinkConfig::velamq("recording-main", "mqtt://127.0.0.1:1883", "field-tcp-edge")
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
                DataConfigCollection::new(10),
                DataConfigPublish::new(
                    "recording-main",
                    "acceptance/{edge_id}/{device_id}/telemetry",
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

#[tokio::test]
async fn laboratory_runner_executes_released_schedule_and_retains_point_evidence() {
    let directory = tempdir().unwrap();
    let mut options = FieldEnduranceOptions::laboratory(
        simulated_package(),
        directory.path().join("runtime.rocksdb"),
    );
    options.duration = Duration::from_millis(45);
    options.scheduler_interval = Duration::from_millis(2);
    options.minimum_cycles = 3;
    options.maximum_failure_ratio = 0.0;

    let report = run_field_endurance_acceptance(options).await.unwrap();

    assert_eq!(report.schema_version, 4);
    assert_eq!(report.status, FieldEnduranceStatus::Passed);
    assert!(report.cycles.attempted >= 3);
    assert_eq!(report.cycles.failed, 0);
    assert!(report.cycles.samples_collected >= 3);
    assert!(report.points["pump-1/pressure"].observations >= 3);
    assert!(report.criteria.all_configured_points_observed);
    assert!(report.criteria.protocols_connected_at_finish);
    assert!(report.criteria.protocol_activity_observed);
    assert!(report.criteria.protocols_individually_healthy);
    assert_eq!(report.protocol_acceptance.len(), 1);
    assert!(report.protocol_acceptance[0].passed);
    assert!(report.protocol_acceptance[0].continuous_activity);
    assert!(!report.protocol_acceptance[0].counter_reset_observed);
    assert_eq!(report.protocol_acceptance[0].collection_failure_count, 0);
    assert!(report.protocols[0].collection_attempt_count >= 3);
    assert!(report.protocols[0].collection_success_count >= 3);
    assert!(!report.mqtt.exercised);
    assert!(report.mqtt.sink_acceptance.is_empty());
    assert!(report.criteria.mqtt_puback_complete.is_none());
    assert!(report.criteria.mqtt_sinks_continuously_publishing.is_none());
    assert!(!report.physical_device_exercised);
    assert!(!report.limitations.is_empty());
}

#[tokio::test]
async fn runner_rejects_collection_that_exceeds_the_continuity_gap() {
    let directory = tempdir().unwrap();
    let mut options = FieldEnduranceOptions::laboratory(
        simulated_package(),
        directory.path().join("runtime.rocksdb"),
    );
    options.duration = Duration::from_secs(1);
    options.scheduler_interval = Duration::from_millis(2);
    options.minimum_cycles = 3;
    options.maximum_failure_ratio = 0.0;
    options.maximum_progress_gap = Duration::from_millis(1);

    let report = run_field_endurance_acceptance(options).await.unwrap();

    assert_eq!(report.status, FieldEnduranceStatus::Failed);
    assert!(!report.criteria.protocols_individually_healthy);
    assert!(!report.protocol_acceptance[0].continuous_activity);
    assert!(report.protocol_acceptance[0].maximum_observed_success_gap_ms > 1);
    assert!(report.observed_duration_ms < report.configured_duration_ms);
    assert!(report
        .recent_errors
        .iter()
        .any(|error| error.contains("successful progress stalled")));
    assert!(report
        .recent_errors
        .iter()
        .any(|error| error.contains("protocol connection sim-main")));
}

#[tokio::test]
async fn configured_duration_is_a_hard_upper_bound_when_minimum_cycles_are_unreachable() {
    let directory = tempdir().unwrap();
    let mut options = FieldEnduranceOptions::laboratory(
        simulated_package(),
        directory.path().join("runtime.rocksdb"),
    );
    options.duration = Duration::from_millis(35);
    options.scheduler_interval = Duration::from_millis(2);
    options.minimum_cycles = 10_000;
    options.maximum_progress_gap = Duration::from_secs(1);

    let report = tokio::time::timeout(
        Duration::from_millis(250),
        run_field_endurance_acceptance(options),
    )
    .await
    .expect("configured duration must stop the run even when minimum cycles cannot be met")
    .unwrap();

    assert_eq!(report.status, FieldEnduranceStatus::Failed);
    assert!(report.criteria.configured_duration_met);
    assert!(!report.criteria.minimum_cycles_met);
    assert!(report.cycles.attempted < 10_000);
}

#[tokio::test]
async fn runner_uses_the_production_adapter_against_an_independent_tcp_endpoint() {
    let mut simulator_options = ModbusTcpSimulatorOptions::new("127.0.0.1:0".parse().unwrap());
    simulator_options.dynamic_holding_floats.insert(
        10,
        DynamicFloatPoint::new(2.4, 0.2, Duration::from_millis(8)),
    );
    let simulator = ModbusTcpSimulator::bind(simulator_options).await.unwrap();
    let endpoint = simulator.local_addr().unwrap().to_string();
    let simulator_task = tokio::spawn(simulator.run());
    let directory = tempdir().unwrap();
    let mut options = FieldEnduranceOptions::laboratory(
        modbus_tcp_package(endpoint),
        directory.path().join("runtime.rocksdb"),
    );
    options.duration = Duration::from_millis(100);
    options.scheduler_interval = Duration::from_millis(2);
    options.minimum_cycles = 4;
    options.maximum_failure_ratio = 0.0;
    options.changing_points = BTreeSet::from(["pump-1/pressure".to_string()]);

    let report = run_field_endurance_acceptance(options).await.unwrap();

    assert_eq!(report.status, FieldEnduranceStatus::Passed);
    assert!(report.points["pump-1/pressure"].distinct_values >= 2);
    assert_eq!(report.protocols.len(), 1);
    assert_eq!(report.protocols[0].protocol, "Modbus TCP");
    assert!(report.protocols[0].connected);
    assert!(report.protocols[0].collection_attempt_count >= 4);
    assert!(report.protocols[0].collection_success_count >= 4);
    assert!(report.criteria.protocol_activity_observed);
    assert!(report.criteria.protocols_individually_healthy);
    assert_eq!(report.protocol_acceptance.len(), 1);
    assert_eq!(report.protocol_acceptance[0].protocol, "Modbus TCP");
    assert!(report.protocol_acceptance[0].passed);
    simulator_task.abort();
}

#[tokio::test]
async fn physical_runner_rejects_simulated_protocol_even_with_complete_identity() {
    let directory = tempdir().unwrap();
    let mut options = FieldEnduranceOptions::laboratory(
        simulated_package(),
        directory.path().join("runtime.rocksdb"),
    );
    options.physical_device_exercised = true;
    options.exercise_mqtt = true;
    options.physical_device = Some(FieldDeviceIdentity {
        site_id: "work-order-42".to_string(),
        operator: "operator-a".to_string(),
        connection_id: "sim-main".to_string(),
        manufacturer: "VelaEdge Lab".to_string(),
        model: "Simulator".to_string(),
        serial_number: "SIM-001".to_string(),
    });

    let error = run_field_endurance_acceptance(options)
        .await
        .unwrap_err()
        .to_string();

    assert!(error.contains("cannot use the simulated protocol"));
}

#[tokio::test]
async fn physical_runner_rejects_identity_for_an_unused_connection_before_network_io() {
    let directory = tempdir().unwrap();
    let mut options = FieldEnduranceOptions::laboratory(
        modbus_tcp_package("127.0.0.1:9".to_string()),
        directory.path().join("runtime.rocksdb"),
    );
    options.physical_device_exercised = true;
    options.exercise_mqtt = true;
    options.physical_device = Some(FieldDeviceIdentity {
        site_id: "work-order-42".to_string(),
        operator: "operator-a".to_string(),
        connection_id: "missing".to_string(),
        manufacturer: "Vendor A".to_string(),
        model: "Meter-1".to_string(),
        serial_number: "ASSET-001".to_string(),
    });

    let error = run_field_endurance_acceptance(options)
        .await
        .unwrap_err()
        .to_string();

    assert!(error.contains(
        "physical device connection missing is not used by an enabled data configuration"
    ));
}

#[tokio::test]
async fn runner_rejects_a_changing_point_outside_the_released_package() {
    let directory = tempdir().unwrap();
    let mut options = FieldEnduranceOptions::laboratory(
        simulated_package(),
        directory.path().join("runtime.rocksdb"),
    );
    options.changing_points = BTreeSet::from(["pump-1/missing".to_string()]);

    let error = run_field_endurance_acceptance(options)
        .await
        .unwrap_err()
        .to_string();

    assert!(error.contains("required changing point is not configured"));
}
