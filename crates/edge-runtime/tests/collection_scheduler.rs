use edge_core::{
    CollectionTask, DeviceInstance, EdgeConfigPackage, PointAddress, ProtocolConnection,
    SerialConnectionSettings, TelemetryPointMapping, TelemetryType, TelemetryValue,
};
use edge_runtime::{
    append_modbus_rtu_crc, CollectionRunStats, CollectionSchedule, ConfiguredEdgeRuntime,
    ScriptedSerialBus, ScriptedSerialBusFactory,
};

fn package() -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", "2026.06.28-scheduler")
        .with_device(DeviceInstance::new("meter-1", "power-meter"))
        .with_protocol_connection(ProtocolConnection::modbus_rtu_serial(
            "meter-rs485-bus-1",
            SerialConnectionSettings::new("/dev/ttyUSB0", 9600),
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "voltage",
            "meter-1",
            "voltage",
            "meter-rs485-bus-1",
            PointAddress::modbus_holding_register(40001),
            TelemetryType::Integer,
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "current",
            "meter-1",
            "current",
            "meter-rs485-bus-1",
            PointAddress::modbus_holding_register(40002),
            TelemetryType::Integer,
        ))
        .with_collection_task(CollectionTask::interval(
            "voltage-fast",
            "meter-1",
            vec!["voltage".to_string()],
            1000,
        ))
        .with_collection_task(CollectionTask::interval(
            "current-slow",
            "meter-1",
            vec!["current".to_string()],
            5000,
        ))
}

fn two_connection_package() -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", "2026.06.28-resilient")
        .with_device(DeviceInstance::new("meter-1", "power-meter"))
        .with_device(DeviceInstance::new("meter-2", "power-meter"))
        .with_protocol_connection(ProtocolConnection::modbus_rtu_serial(
            "rs485-bad",
            SerialConnectionSettings::new("/dev/ttyUSB0", 9600),
        ))
        .with_protocol_connection(ProtocolConnection::modbus_rtu_serial(
            "rs485-good",
            SerialConnectionSettings::new("/dev/ttyUSB1", 9600),
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "voltage",
            "meter-1",
            "voltage",
            "rs485-bad",
            PointAddress::modbus_holding_register(40001),
            TelemetryType::Integer,
        ))
        .with_point_mapping(TelemetryPointMapping::new(
            "current",
            "meter-2",
            "current",
            "rs485-good",
            PointAddress::modbus_holding_register(40001),
            TelemetryType::Integer,
        ))
        .with_collection_task(CollectionTask::interval(
            "bad-task",
            "meter-1",
            vec!["voltage".to_string()],
            1000,
        ))
        .with_collection_task(CollectionTask::interval(
            "good-task",
            "meter-2",
            vec!["current".to_string()],
            1000,
        ))
}

#[tokio::test]
async fn configured_runtime_collects_only_points_in_selected_task() {
    let bus = ScriptedSerialBus::new(vec![response(1, &[220])]);
    let observed_bus = bus.clone();
    let factory = ScriptedSerialBusFactory::new(vec![("meter-rs485-bus-1".to_string(), bus)]);
    let mut runtime = ConfiguredEdgeRuntime::new(package(), factory).unwrap();

    let report = runtime.collect_task_once("voltage-fast").await.unwrap();

    assert_eq!(report.samples_collected, 1);
    assert_eq!(
        runtime.shadow("meter-1").unwrap().latest_value("voltage"),
        Some(&TelemetryValue::Integer(220))
    );
    assert_eq!(
        runtime.shadow("meter-1").unwrap().latest_value("current"),
        None
    );
    assert_eq!(observed_bus.requests().len(), 1);
    assert_eq!(&observed_bus.requests()[0][..6], &[1, 0x03, 0, 0, 0, 1]);
}

#[test]
fn collection_schedule_tracks_due_tasks_by_interval() {
    let mut schedule = CollectionSchedule::from_package(&package()).unwrap();

    assert_eq!(
        schedule.due_task_ids(0),
        vec!["voltage-fast", "current-slow"]
    );
    schedule.mark_ran("voltage-fast", 0).unwrap();
    schedule.mark_ran("current-slow", 0).unwrap();

    assert!(schedule.due_task_ids(999).is_empty());
    assert_eq!(schedule.due_task_ids(1000), vec!["voltage-fast"]);
    assert_eq!(
        schedule.due_task_ids(5000),
        vec!["voltage-fast", "current-slow"]
    );
}

#[tokio::test]
async fn configured_runtime_runs_due_tasks_and_advances_schedule() {
    let bus = ScriptedSerialBus::new(vec![
        response(1, &[220]),
        response(1, &[7]),
        response(1, &[221]),
    ]);
    let factory = ScriptedSerialBusFactory::new(vec![("meter-rs485-bus-1".to_string(), bus)]);
    let package = package();
    let mut schedule = CollectionSchedule::from_package(&package).unwrap();
    let mut runtime = ConfiguredEdgeRuntime::new(package, factory).unwrap();

    let first = runtime
        .collect_due_tasks_once(&mut schedule, 0)
        .await
        .unwrap();
    assert_eq!(first.tasks_run, 2);
    assert_eq!(first.samples_collected, 2);
    assert!(schedule.due_task_ids(999).is_empty());

    let second = runtime
        .collect_due_tasks_once(&mut schedule, 1000)
        .await
        .unwrap();
    assert_eq!(second.tasks_run, 1);
    assert_eq!(second.samples_collected, 1);
    assert_eq!(schedule.due_task_ids(1001), Vec::<&str>::new());
}

#[tokio::test]
async fn configured_runtime_continues_due_tasks_after_one_task_fails() {
    let package = two_connection_package();
    let bad_bus = ScriptedSerialBus::new(Vec::new());
    let good_bus = ScriptedSerialBus::new(vec![response(1, &[9])]);
    let factory = ScriptedSerialBusFactory::new(vec![
        ("rs485-bad".to_string(), bad_bus),
        ("rs485-good".to_string(), good_bus),
    ]);
    let mut schedule = CollectionSchedule::from_package(&package).unwrap();
    let mut runtime = ConfiguredEdgeRuntime::new(package, factory).unwrap();

    let report = runtime
        .collect_due_tasks_resilient_once(&mut schedule, 0)
        .await
        .unwrap();

    assert_eq!(report.tasks_run, 2);
    assert_eq!(report.tasks_succeeded, 1);
    assert_eq!(report.tasks_failed, 1);
    assert_eq!(report.samples_collected, 1);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].task_id, "bad-task");
    assert_eq!(
        runtime.shadow("meter-2").unwrap().latest_value("current"),
        Some(&TelemetryValue::Integer(9))
    );
    assert!(schedule.due_task_ids(999).is_empty());
}

#[test]
fn collection_run_stats_converts_resilient_reports_to_runtime_metrics() {
    let mut stats = CollectionRunStats::new(2);
    stats.record_tick(2, 1, 1, 17);
    stats.record_tick(1, 1, 0, 23);

    let metrics = stats.metrics();

    assert_eq!(metrics.active_task_count, 2);
    assert_eq!(metrics.success_rate, 2.0 / 3.0);
    assert_eq!(metrics.average_latency_ms, 20);
    assert_eq!(metrics.bad_point_count, 1);
}

fn response(slave_id: u8, registers: &[u16]) -> Vec<u8> {
    let mut frame = vec![slave_id, 0x03, (registers.len() * 2) as u8];
    for register in registers {
        frame.extend(register.to_be_bytes());
    }
    append_modbus_rtu_crc(&mut frame);
    frame
}
