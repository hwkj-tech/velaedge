use edge_core::TelemetryType;
use edge_runtime::{RuntimeCapabilityConfig, SimulatedSerialDiscovery};

#[test]
fn runtime_capability_config_declares_serial_protocols_and_mqtt_uplink() {
    let capabilities = RuntimeCapabilityConfig::serial_mqtt_defaults().capabilities();

    assert!(capabilities.contains(&"protocol:modbus-tcp".to_string()));
    assert!(capabilities.contains(&"protocol:modbus-rtu".to_string()));
    assert!(capabilities.contains(&"protocol:dlt645-2007".to_string()));
    assert!(capabilities.contains(&"protocol:iec60870-5-101-unbalanced".to_string()));
    assert!(capabilities.contains(&"protocol:custom-serial-frame-dsl-v1".to_string()));
    assert!(capabilities.contains(&"transport:serial".to_string()));
    assert!(capabilities.contains(&"transport:tcp".to_string()));
    assert!(capabilities.contains(&"uplink:mqtt".to_string()));
    assert!(capabilities.contains(&"local-store:rocksdb".to_string()));
    assert!(!capabilities.contains(&"protocol:mqtt".to_string()));
}

#[test]
fn simulated_serial_discovery_generates_mapping_suggestion() {
    let report = SimulatedSerialDiscovery::new("job-1", "meter-rs485-bus-1").run();

    assert_eq!(report.job_id, "job-1");
    assert_eq!(report.protocol_connection_id, "meter-rs485-bus-1");
    assert_eq!(report.discovered_points.len(), 1);
    assert_eq!(report.discovered_points[0].value_type, TelemetryType::Float);
    assert_eq!(report.suggestions.len(), 1);
    assert_eq!(report.suggestions[0].point_id, "meter_voltage_a");
    assert_eq!(report.suggestions[0].semantic_id, "electric.voltage_a");
    assert_eq!(report.suggestions[0].confidence, 0.82);
}
