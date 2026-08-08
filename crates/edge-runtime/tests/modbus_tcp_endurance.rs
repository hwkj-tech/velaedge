use std::time::Duration;

use edge_runtime::{
    run_modbus_tcp_endurance_acceptance, DynamicFloatPoint, ModbusTcpEnduranceOptions,
    ModbusTcpEnduranceStatus, ModbusTcpSimulator, ModbusTcpSimulatorOptions,
};
use tempfile::tempdir;

#[tokio::test]
async fn endurance_runner_collects_changing_values_over_real_tcp_cycles() {
    let mut simulator_options = ModbusTcpSimulatorOptions::new("127.0.0.1:0".parse().unwrap());
    simulator_options.dynamic_holding_floats.insert(
        10,
        DynamicFloatPoint::new(2.4, 0.2, Duration::from_millis(80)),
    );
    simulator_options.dynamic_holding_floats.insert(
        12,
        DynamicFloatPoint::new(2.6, 0.1, Duration::from_millis(100)),
    );
    simulator_options.coils.insert(0, true);
    simulator_options.coils.insert(6, false);
    simulator_options.input_registers.insert(0, 36);
    let simulator = ModbusTcpSimulator::bind(simulator_options).await.unwrap();
    let endpoint = simulator.local_addr().unwrap().to_string();
    let simulator_task = tokio::spawn(simulator.run());
    let directory = tempdir().unwrap();
    let mut options =
        ModbusTcpEnduranceOptions::laboratory(endpoint, directory.path().join("runtime.rocksdb"));
    options.duration = Duration::from_millis(180);
    options.interval = Duration::from_millis(20);
    options.minimum_cycles = 4;
    options.maximum_failure_ratio = 0.0;
    options.require_recovery = false;

    let report = run_modbus_tcp_endurance_acceptance(options).await.unwrap();

    assert_eq!(report.status, ModbusTcpEnduranceStatus::Passed);
    assert!(report.cycles.attempted >= 4);
    assert_eq!(report.cycles.failed, 0);
    assert!(report.cycles.samples_collected >= report.cycles.succeeded * 5);
    assert!(report.points["pressure"].distinct_values >= 2);
    assert!(report.points["flow"].distinct_values >= 2);
    assert!(report.protocol.connected_at_finish);
    assert_eq!(report.protocol.bad_value_count, 0);
    assert!(!report.mqtt.broker_exercised);
    assert!(report.mqtt.connected_at_finish.is_none());
    assert!(report.criteria.mqtt_puback_complete.is_none());
    assert!(!report.physical_device_exercised);
    assert!(report.limitation.is_some());
    simulator_task.abort();
}

#[tokio::test]
async fn endurance_runner_retains_failed_evidence_for_unreachable_endpoint() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = listener.local_addr().unwrap().to_string();
    drop(listener);
    let directory = tempdir().unwrap();
    let mut options =
        ModbusTcpEnduranceOptions::laboratory(endpoint, directory.path().join("runtime.rocksdb"));
    options.duration = Duration::from_millis(40);
    options.interval = Duration::from_millis(10);
    options.minimum_cycles = 1;
    options.maximum_failure_ratio = 0.0;
    options.require_dynamic_values = false;
    options.require_recovery = false;

    let report = run_modbus_tcp_endurance_acceptance(options).await.unwrap();

    assert_eq!(report.status, ModbusTcpEnduranceStatus::Failed);
    assert_eq!(report.cycles.succeeded, 0);
    assert!(report.cycles.failed >= 1);
    assert!(!report.protocol.connected_at_finish);
    assert!(!report.recent_errors.is_empty());
    assert!(!report.criteria.failure_ratio_within_limit);
}
