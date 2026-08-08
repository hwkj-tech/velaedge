use edge_core::ProtocolType;
use edge_core::TelemetryType;
use edge_runtime::{
    RuntimeCapabilityConfig, RuntimeProtocolCatalog, RuntimeProtocolMaturity,
    RuntimeProtocolTransport, SimulatedSerialDiscovery,
};

#[test]
fn runtime_capability_config_declares_serial_protocols_and_mqtt_uplink() {
    let capabilities = RuntimeCapabilityConfig::serial_mqtt_defaults().capabilities();

    assert!(capabilities.contains(&"protocol:modbus-tcp".to_string()));
    assert!(capabilities.contains(&"protocol:modbus-rtu".to_string()));
    assert!(capabilities.contains(&"protocol:dlt645-2007".to_string()));
    assert!(capabilities.contains(&"protocol:iec60870-5-101-unbalanced".to_string()));
    assert!(capabilities.contains(&"protocol:custom-serial-frame-dsl-v2".to_string()));
    assert!(capabilities.contains(&"protocol:custom-serial-frame-dsl-v1".to_string()));
    assert!(capabilities.contains(&"protocol:opc-ua-client".to_string()));
    assert!(capabilities.contains(&"transport:serial".to_string()));
    assert!(capabilities.contains(&"transport:tcp".to_string()));
    assert!(capabilities.contains(&"uplink:mqtt".to_string()));
    assert!(capabilities.contains(&"local-store:rocksdb".to_string()));
    assert!(!capabilities.contains(&"protocol:mqtt".to_string()));
}

#[test]
fn runtime_protocol_catalog_reports_executable_protocol_capabilities() {
    let modbus_tcp = RuntimeProtocolCatalog::descriptor(ProtocolType::ModbusTcp);
    assert_eq!(modbus_tcp.transport, RuntimeProtocolTransport::Tcp);
    assert_eq!(
        modbus_tcp.maturity,
        RuntimeProtocolMaturity::DeploymentCandidate
    );
    assert!(modbus_tcp.telemetry_read);
    assert!(modbus_tcp.command_write);

    let opc_ua = RuntimeProtocolCatalog::descriptor(ProtocolType::OpcUa);
    assert_eq!(
        opc_ua.maturity,
        RuntimeProtocolMaturity::DeploymentCandidate
    );
    assert!(opc_ua.telemetry_read);
    assert!(opc_ua.command_write);
    assert!(opc_ua.is_executable());

    let iec101 = RuntimeProtocolCatalog::descriptor(ProtocolType::Iec101);
    assert_eq!(iec101.transport, RuntimeProtocolTransport::Serial);
    assert_eq!(
        iec101.maturity,
        RuntimeProtocolMaturity::DeploymentCandidate
    );
    assert!(iec101.telemetry_read);
    assert!(iec101.command_write);

    let iec104 = RuntimeProtocolCatalog::descriptor(ProtocolType::Iec104);
    assert_eq!(
        iec104.maturity,
        RuntimeProtocolMaturity::DeploymentCandidate
    );
    assert!(iec104.telemetry_read);
    assert!(iec104.command_write);

    let bacnet_ip = RuntimeProtocolCatalog::descriptor(ProtocolType::BacnetIp);
    assert_eq!(bacnet_ip.transport, RuntimeProtocolTransport::Udp);
    assert_eq!(
        bacnet_ip.maturity,
        RuntimeProtocolMaturity::DeploymentCandidate
    );
    assert!(bacnet_ip.telemetry_read);
    assert!(bacnet_ip.command_write);

    let dlt645 = RuntimeProtocolCatalog::descriptor(ProtocolType::Dlt645);
    assert_eq!(
        dlt645.maturity,
        RuntimeProtocolMaturity::DeploymentCandidate
    );
    assert!(dlt645.telemetry_read);
    assert!(!dlt645.command_write);

    let siemens_s7 = RuntimeProtocolCatalog::descriptor(ProtocolType::SiemensS7);
    assert_eq!(siemens_s7.transport, RuntimeProtocolTransport::Tcp);
    assert_eq!(
        siemens_s7.maturity,
        RuntimeProtocolMaturity::DeploymentCandidate
    );
    assert!(siemens_s7.telemetry_read);
    assert!(siemens_s7.command_write);
    assert!(siemens_s7.is_executable());

    let omron_fins = RuntimeProtocolCatalog::descriptor(ProtocolType::OmronFins);
    assert_eq!(omron_fins.transport, RuntimeProtocolTransport::TcpUdp);
    assert_eq!(omron_fins.capability_id, "omron-fins");
    assert_eq!(
        omron_fins.maturity,
        RuntimeProtocolMaturity::DeploymentCandidate
    );
    assert!(omron_fins.telemetry_read);
    assert!(omron_fins.command_write);

    let capabilities = RuntimeCapabilityConfig::serial_mqtt_defaults().capabilities();
    assert!(capabilities.contains(&"protocol:opc-ua-client".to_string()));
    assert!(capabilities.contains(&"protocol:iec60870-5-104-client".to_string()));
    assert!(capabilities.contains(&"protocol:siemens-s7".to_string()));
    assert!(capabilities.contains(&"protocol:omron-fins".to_string()));
    assert!(capabilities.contains(&"transport:udp".to_string()));

    let simulated = RuntimeProtocolCatalog::descriptor(ProtocolType::Simulated);
    assert_eq!(simulated.maturity, RuntimeProtocolMaturity::Laboratory);
    assert!(simulated.is_executable());

    let custom_serial = RuntimeProtocolCatalog::descriptor(ProtocolType::CustomSerial);
    assert_eq!(custom_serial.capability_id, "custom-serial-frame-dsl-v2");
    assert_eq!(
        custom_serial.maturity,
        RuntimeProtocolMaturity::DeploymentCandidate
    );
    assert!(custom_serial.is_executable());
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
