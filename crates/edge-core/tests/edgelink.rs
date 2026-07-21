use edge_core::{
    decode_edgelink_frame, encode_edgelink_frame, DiscoveredPoint, DiscoveryReport,
    DiscoveryRequest, EdgeConfigPackage, EdgeLinkMessage, EdgeLinkMessageKind, EdgeLinkPayload,
    PointAddress, PointMappingSuggestion, TelemetryType,
};

#[test]
fn edgelink_frame_round_trips_hello_message() {
    let message = EdgeLinkMessage::hello(
        "edge-dev",
        "runtime-dev",
        "0.1.0",
        Some("2026.06.26-001".to_string()),
        vec![
            "protocol:modbus-tcp".to_string(),
            "local-store:rocksdb".to_string(),
        ],
    );

    let encoded = encode_edgelink_frame(&message).expect("frame should encode");
    let decoded = decode_edgelink_frame(&encoded).expect("frame should decode");

    assert_eq!(decoded.message_id, message.message_id);
    assert_eq!(decoded.sequence, message.sequence);
    assert_eq!(decoded.kind, EdgeLinkMessageKind::Hello);
    assert_eq!(decoded.edge_id, "edge-dev");
    assert_eq!(decoded.runtime_id.as_deref(), Some("runtime-dev"));

    let EdgeLinkPayload::Hello(hello) = decoded.payload else {
        panic!("expected hello payload");
    };
    assert_eq!(hello.runtime_version, "0.1.0");
    assert_eq!(
        hello.applied_config_version.as_deref(),
        Some("2026.06.26-001")
    );
    assert_eq!(hello.capabilities.len(), 2);
}

#[test]
fn edgelink_discovery_request_round_trips_bounded_scan() {
    let request =
        DiscoveryRequest::modbus_holding_registers("job-2", "meter-rs485-bus-1", 40001, 40008)
            .with_slave_id(7);
    let message = EdgeLinkMessage::discovery_request("edge-dev", "runtime-dev", 5, request.clone());

    let decoded = decode_edgelink_frame(&encode_edgelink_frame(&message).unwrap()).unwrap();

    assert_eq!(decoded.kind, EdgeLinkMessageKind::DiscoveryRequest);
    let EdgeLinkPayload::DiscoveryRequest(payload) = decoded.payload else {
        panic!("expected discovery request payload");
    };
    assert_eq!(payload, request);
    assert_eq!(payload.point_count().unwrap(), 8);
    assert!(payload.validate().is_ok());
}

#[test]
fn edgelink_ack_references_received_message() {
    let hello = EdgeLinkMessage::hello("edge-dev", "runtime-dev", "0.1.0", None, Vec::new());
    let ack = EdgeLinkMessage::ack("edge-dev", "runtime-dev", hello.message_id, hello.sequence);

    assert_eq!(ack.kind, EdgeLinkMessageKind::Ack);
    assert_eq!(ack.edge_id, "edge-dev");
    assert_eq!(ack.runtime_id.as_deref(), Some("runtime-dev"));

    let EdgeLinkPayload::Ack(payload) = ack.payload else {
        panic!("expected ack payload");
    };
    assert_eq!(payload.ack_message_id, hello.message_id);
    assert_eq!(payload.ack_sequence, hello.sequence);
    assert!(payload.accepted);
}

#[test]
fn edgelink_config_deploy_and_report_use_versioned_payloads() {
    let package = EdgeConfigPackage::new("edge-dev", "2026.06.27-010");
    let deploy = EdgeLinkMessage::config_deploy("edge-dev", "runtime-dev", 2, package.clone());

    assert_eq!(deploy.kind, EdgeLinkMessageKind::ConfigDeploy);
    let EdgeLinkPayload::ConfigDeploy(payload) = deploy.payload else {
        panic!("expected config deploy payload");
    };
    assert_eq!(payload.version, package.version);

    let report = EdgeLinkMessage::config_report(
        "edge-dev",
        "runtime-dev",
        3,
        "2026.06.27-010",
        Some("2026.06.27-010".to_string()),
        true,
        None,
    );
    assert_eq!(report.kind, EdgeLinkMessageKind::ConfigReport);
    let EdgeLinkPayload::ConfigReport(payload) = report.payload else {
        panic!("expected config report payload");
    };
    assert_eq!(payload.desired_version, "2026.06.27-010");
    assert_eq!(payload.applied_version.as_deref(), Some("2026.06.27-010"));
    assert!(payload.accepted);
}

#[test]
fn edgelink_decode_rejects_incomplete_frames() {
    let message = EdgeLinkMessage::hello("edge-dev", "runtime-dev", "0.1.0", None, Vec::new());
    let mut encoded = encode_edgelink_frame(&message).expect("frame should encode");
    encoded.truncate(encoded.len() - 1);

    let error = decode_edgelink_frame(&encoded).expect_err("incomplete frame should fail");
    assert!(error.to_string().contains("incomplete EdgeLink frame"));
}

#[test]
fn edgelink_decode_rejects_invalid_json() {
    let mut frame = Vec::new();
    frame.extend_from_slice(&(4_u32.to_be_bytes()));
    frame.extend_from_slice(b"nope");

    let error = decode_edgelink_frame(&frame).expect_err("invalid json should fail");
    assert!(error.to_string().contains("invalid EdgeLink frame JSON"));
}

#[test]
fn edgelink_discovery_report_round_trips_discovered_serial_points() {
    let report = DiscoveryReport::new("job-1", "meter-rs485-bus-1")
        .with_point(
            DiscoveredPoint::new(
                "meter-rs485-bus-1",
                PointAddress::modbus_holding_register(40001),
                TelemetryType::Float,
            )
            .with_sample_values(vec!["220.1".to_string(), "220.3".to_string()])
            .with_confidence(0.72),
        )
        .with_suggestion(
            PointMappingSuggestion::new(
                "meter_voltage_a",
                "meter-1",
                "electric.voltage_a",
                "meter-rs485-bus-1",
                PointAddress::modbus_holding_register(40001),
                TelemetryType::Float,
            )
            .with_unit("V")
            .with_confidence(0.82)
            .with_evidence("数值范围和波动特征符合 A 相电压"),
        );

    let message = EdgeLinkMessage::discovery_report("edge-dev", "runtime-dev", 4, report);
    let encoded = encode_edgelink_frame(&message).expect("frame should encode");
    let decoded = decode_edgelink_frame(&encoded).expect("frame should decode");

    assert_eq!(decoded.kind, EdgeLinkMessageKind::DiscoveryReport);
    let EdgeLinkPayload::DiscoveryReport(payload) = decoded.payload else {
        panic!("expected discovery report payload");
    };
    assert_eq!(payload.job_id, "job-1");
    assert_eq!(payload.discovered_points.len(), 1);
    assert_eq!(payload.suggestions.len(), 1);
    assert_eq!(payload.suggestions[0].point_id, "meter_voltage_a");
    assert_eq!(payload.suggestions[0].confidence, 0.82);
}
