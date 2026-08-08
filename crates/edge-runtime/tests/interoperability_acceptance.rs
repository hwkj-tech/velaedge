use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    process::Command,
};

use edge_core::{
    DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint, DataConfigPublish,
    EdgeConfigPackage, PointAddress, ProtocolConnection, ProtocolType, TelemetryType,
};
use edge_runtime::{
    evaluate_field_interoperability, field_protocol_name, FieldInteroperabilityEvidence,
    FieldInteroperabilityPolicy, FieldInteroperabilityStatus, RuntimeProtocolCatalog,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

const DAY_MS: u64 = 86_400_000;

#[test]
fn deployment_policy_exactly_covers_executable_physical_protocols() {
    let policy_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../deploy/field-acceptance-policy.json");
    let policy = FieldInteroperabilityPolicy::from_json_slice(&fs::read(policy_path).unwrap())
        .expect("deployment field policy must be valid");
    let expected = RuntimeProtocolCatalog::executable()
        .into_iter()
        .filter(|descriptor| descriptor.protocol_type != ProtocolType::Simulated)
        .map(|descriptor| field_protocol_name(descriptor.protocol_type).to_string())
        .collect::<BTreeSet<_>>();
    let report = evaluate_field_interoperability(&policy, &[]).unwrap();
    let required = report
        .policy
        .required_protocols
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();

    assert_eq!(required, expected);
    assert_eq!(report.policy.protocol_requirements.len(), expected.len());
    assert!(report
        .policy
        .protocol_requirements
        .iter()
        .all(
            |requirement| requirement.minimum_manufacturers >= 1 && requirement.minimum_models >= 1
        ));
    assert!(report.policy.minimum_duration_ms >= DAY_MS);
    assert!(report.policy.maximum_failure_ratio <= 0.01);
    assert!(report.policy.maximum_progress_gap_ms <= 300_000);
}

#[test]
fn policy_parser_rejects_unknown_schema_duplicate_aliases_and_unsupported_protocols() {
    let schema_error = FieldInteroperabilityPolicy::from_json_slice(
        br#"{"schemaVersion":2,"minimumDurationSeconds":86400,"maximumFailureRatio":0.01,"protocols":[{"protocol":"Modbus TCP","minimumManufacturers":1,"minimumModels":1}]}"#,
    )
    .unwrap_err()
    .to_string();
    assert!(schema_error.contains("unsupported schema 2"));

    let alias_error = FieldInteroperabilityPolicy::from_json_slice(
        br#"{"schemaVersion":1,"minimumDurationSeconds":86400,"maximumFailureRatio":0.01,"protocols":[{"protocol":"Modbus TCP","minimumManufacturers":1,"minimumModels":1},{"protocol":"modbus_tcp","minimumManufacturers":1,"minimumModels":1}]}"#,
    )
    .unwrap_err()
    .to_string();
    assert!(alias_error.contains("duplicate protocol alias Modbus TCP"));

    let unsupported_error = FieldInteroperabilityPolicy::from_json_slice(
        br#"{"schemaVersion":1,"minimumDurationSeconds":86400,"maximumFailureRatio":0.01,"protocols":[{"protocol":"Mystery Bus","minimumManufacturers":1,"minimumModels":1}]}"#,
    )
    .unwrap_err()
    .to_string();
    assert!(unsupported_error.contains("unsupported or non-physical protocol Mystery Bus"));
}

fn evidence(
    source: &str,
    protocol: &str,
    manufacturer: &str,
    model: &str,
    serial: &str,
) -> FieldInteroperabilityEvidence {
    artifact_evidence(
        source,
        valid_report(protocol, manufacturer, model, serial),
        serial,
    )
}

fn artifact_evidence(source: &str, report: Value, serial: &str) -> FieldInteroperabilityEvidence {
    let (report_bytes, package_bytes, broker_receipt_bytes) = artifact_bytes(report, serial);
    let native_broker_audit_bytes = native_broker_audit_bytes(&broker_receipt_bytes);
    FieldInteroperabilityEvidence::from_artifacts(
        source,
        &report_bytes,
        &package_bytes,
        &broker_receipt_bytes,
        &native_broker_audit_bytes,
    )
    .unwrap()
}

fn native_broker_audit_bytes(receipt_bytes: &[u8]) -> Vec<u8> {
    let receipt: Value = serde_json::from_slice(receipt_bytes).unwrap();
    serde_json::to_vec(&json!({
        "schemaVersion": 1,
        "broker": "VelaMQ",
        "brokerInstanceId": "velamq-node-a",
        "auditId": format!("audit-{}", receipt["edgeId"].as_str().unwrap()),
        "exportedAt": "2026-07-19T00:00:01Z",
        "edgeId": receipt["edgeId"],
        "configVersion": receipt["configVersion"],
        "packageSha256": receipt["packageSha256"],
        "firstObservedAt": receipt["firstReceivedAt"],
        "lastObservedAt": receipt["lastReceivedAt"],
        "messageCount": receipt["messageCount"],
        "routes": receipt["routes"]
    }))
    .unwrap()
}

fn artifact_bytes(report: Value, serial: &str) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let connection_id = report["protocols"][0]["connection_id"]
        .as_str()
        .expect("test report has a connection id");
    let protocol = report["protocols"][0]["protocol"]
        .as_str()
        .expect("test report has a protocol");
    let mut connection = ProtocolConnection::simulated(connection_id);
    connection.protocol = protocol_type(protocol);
    let package = EdgeConfigPackage::new(format!("edge-{serial}"), "v1.2.3")
        .with_protocol_connection(connection);
    artifact_bytes_with_package(report, package, serial)
}

fn artifact_bytes_with_package(
    mut report: Value,
    mut package: EdgeConfigPackage,
    serial: &str,
) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    if package.data_configs.is_empty() {
        let connection_id = package.protocol_connections[0].connection_id.clone();
        package = package.with_data_config(field_data_config("field-acceptance", &connection_id));
    }
    let package_bytes = serde_json::to_vec(&package).unwrap();
    let package_sha256 = format!("{:x}", Sha256::digest(&package_bytes));
    report["packageSha256"] = json!(package_sha256.clone());
    let broker_receipt = json!({
        "schemaVersion": 1,
        "edgeId": format!("edge-{serial}"),
        "configVersion": "v1.2.3",
        "packageSha256": package_sha256,
        "firstReceivedAt": "2026-07-18T00:00:00Z",
        "lastReceivedAt": "2026-07-19T00:00:00Z",
        "messageCount": report["mqtt"]["publishSuccessCount"],
        "routes": [{
            "broker": "mqtt://velamq.example:1883",
            "consumerId": "field-audit-consumer",
            "messageCount": report["mqtt"]["publishSuccessCount"],
            "topics": [format!("field/{serial}/telemetry")]
        }]
    });
    (
        serde_json::to_vec(&report).unwrap(),
        package_bytes,
        serde_json::to_vec(&broker_receipt).unwrap(),
    )
}

fn field_data_config(config_id: &str, connection_id: &str) -> DataConfig {
    field_data_config_with_sink(config_id, connection_id, "field-sink")
}

fn field_data_config_with_sink(config_id: &str, connection_id: &str, sink_id: &str) -> DataConfig {
    let point_id = format!("{config_id}-value");
    DataConfig::new(
        config_id,
        "Field acceptance",
        "field-device",
        connection_id,
        DataConfigCollection::new(1_000),
        DataConfigPublish::new(
            sink_id,
            "field/{edge_id}/{device_id}/telemetry",
            DataConfigPayload::object(),
        ),
    )
    .with_point(DataConfigPoint::new(
        point_id.clone(),
        format!("field.{point_id}"),
        PointAddress::simulated(point_id),
        TelemetryType::Float,
        "value",
    ))
}

fn protocol_type(protocol: &str) -> ProtocolType {
    match protocol.trim().to_ascii_lowercase().as_str() {
        "modbus tcp" | "modbus-tcp" | "modbus_tcp" => ProtocolType::ModbusTcp,
        "modbus rtu" | "modbus-rtu" | "modbus_rtu" => ProtocolType::ModbusRtu,
        "dlt645" | "dlt645-2007" | "dl/t645" | "dl/t 645-2007" => ProtocolType::Dlt645,
        "iec-101" | "iec 60870-5-101" | "iec60870-5-101-unbalanced" => ProtocolType::Iec101,
        "iec-104" => ProtocolType::Iec104,
        "custom serial" | "custom-serial" | "custom_serial" => ProtocolType::CustomSerial,
        "opc ua" | "opc-ua-client" => ProtocolType::OpcUa,
        "bacnet/ip" | "bacnet ip" | "bacnet-ip" => ProtocolType::BacnetIp,
        "siemens s7" | "siemens-s7" | "s7" => ProtocolType::SiemensS7,
        "omron fins" | "omron-fins" | "fins" => ProtocolType::OmronFins,
        other => panic!("unsupported interoperability test protocol {other}"),
    }
}

fn valid_report(protocol: &str, manufacturer: &str, model: &str, serial: &str) -> Value {
    json!({
        "schemaVersion": 4,
        "status": "passed",
        "mode": "physical_field_endurance",
        "physicalDeviceExercised": true,
        "physicalDevice": {
            "siteId": "station-a",
            "operator": "field-engineer",
            "connectionId": "primary",
            "manufacturer": manufacturer,
            "model": model,
            "serialNumber": serial
        },
        "edgeId": format!("edge-{serial}"),
        "configVersion": "v1.2.3",
        "packageSha256": "a".repeat(64),
        "configuredDurationMs": DAY_MS,
        "observedDurationMs": DAY_MS + 1_000,
        "cycles": {
            "attempted": 86_400,
            "failureRatio": 0.001
        },
        "protocols": [{
            "connection_id": "primary",
            "protocol": protocol,
            "connected": true,
            "collection_attempt_count": 86_400,
            "collection_success_count": 86_390,
            "circuit_state": "Closed",
            "write_attempt_count": 0,
            "write_success_count": 0
        }],
        "protocolAcceptance": [{
            "connectionId": "primary",
            "protocol": protocol,
            "connectedAtFinish": true,
            "circuitStateAtFinish": "Closed",
            "collectionAttemptCount": 86_400,
            "collectionSuccessCount": 86_390,
            "collectionFailureCount": 10,
            "failureRatio": 10.0 / 86_400.0,
            "activityObserved": true,
            "failureRatioWithinLimit": true,
            "maximumObservedSuccessGapMs": 1_100,
            "maximumAllowedSuccessGapMs": 300_000,
            "counterResetObserved": false,
            "continuousActivity": true,
            "passed": true
        }],
        "mqtt": {
            "exercised": true,
            "connectedSinkCount": 1,
            "publishSuccessCount": 86_400,
            "publishFailureCount": 0,
            "pendingOutboxMessages": 0,
            "sinks": [{
                "sink_id": "field-sink",
                "broker": "mqtt://velamq.example:1883",
                "connected": true,
                "publish_success_count": 86_400,
                "last_topic": format!("field/{serial}/telemetry")
            }],
            "sinkAcceptance": [{
                "sinkId": "field-sink",
                "publishSuccessCount": 86_400,
                "maximumObservedSuccessGapMs": 1_100,
                "maximumAllowedSuccessGapMs": 300_000,
                "counterResetObserved": false,
                "continuousActivity": true,
                "passed": true
            }]
        },
        "criteria": {
            "configuredDurationMet": true,
            "minimumCyclesMet": true,
            "failureRatioWithinLimit": true,
            "allConfiguredPointsObserved": true,
            "changingPointsObserved": true,
            "protocolsConnectedAtFinish": true,
            "protocolActivityObserved": true,
            "protocolsIndividuallyHealthy": true,
            "mqttSinksContinuouslyPublishing": true,
            "recoveryObserved": true,
            "mqttPubackComplete": true,
            "mqttSinksConnected": true,
            "outboxDrained": true,
            "physicalIdentityComplete": true,
            "productionProtocolsOnly": true
        }
    })
}

fn policy() -> FieldInteroperabilityPolicy {
    FieldInteroperabilityPolicy::default()
}

#[test]
fn accepts_two_physical_manufacturers_for_each_required_protocol() {
    let evidence = vec![
        evidence(
            "dlt645-a.json",
            "DL/T645",
            "Vendor G",
            "Meter-645-A",
            "G-001",
        ),
        evidence(
            "dlt645-b.json",
            "dlt645-2007",
            "Vendor H",
            "Meter-645-B",
            "H-001",
        ),
        evidence(
            "iec101-a.json",
            "iec60870-5-101-unbalanced",
            "Vendor E",
            "RTU-101-A",
            "E-001",
        ),
        evidence(
            "iec101-b.json",
            "IEC 60870-5-101",
            "Vendor F",
            "RTU-101-B",
            "F-001",
        ),
        evidence("iec-a.json", "IEC-104", "Vendor A", "RTU-1", "A-001"),
        evidence("iec-b.json", "IEC-104", "Vendor B", "RTU-2", "B-001"),
        evidence("opc-a.json", "OPC UA", "Vendor C", "Server-1", "C-001"),
        evidence(
            "opc-b.json",
            "opc-ua-client",
            "Vendor D",
            "Server-2",
            "D-001",
        ),
    ];

    let report = evaluate_field_interoperability(&policy(), &evidence).unwrap();

    assert_eq!(report.status, FieldInteroperabilityStatus::Passed);
    assert_eq!(report.summary.accepted_evidence_count, 8);
    assert_eq!(report.summary.satisfied_protocol_count, 4);
    assert!(report
        .policy
        .required_protocols
        .contains(&"DL/T 645-2007".to_string()));
    assert!(report.protocols.iter().all(|protocol| protocol.satisfied));
    assert!(report
        .protocols
        .iter()
        .all(|protocol| protocol.observed_model_count == 2));
    assert!(report.policy.require_configuration_package);
    assert!(report.policy.require_broker_consumer_receipt);
    assert!(report.policy.require_native_broker_audit);
    assert!(report.protocols.iter().all(|protocol| {
        protocol
            .accepted_runs
            .iter()
            .all(|run| run.broker_routes.len() == 1 && run.native_broker_audit_sha256.len() == 64)
    }));
}

#[test]
fn applies_protocol_specific_manufacturer_and_model_requirements() {
    let evidence = vec![
        evidence("s7-1200.json", "S7", "Siemens", "S7-1200", "S7-001"),
        evidence("s7-1500.json", "Siemens S7", "Siemens", "S7-1500", "S7-002"),
        evidence(
            "modbus-tcp.json",
            "modbus_tcp",
            "Acme Controls",
            "MC-100",
            "MB-001",
        ),
    ];
    let policy = FieldInteroperabilityPolicy {
        required_protocols: ["Siemens S7".to_string(), "Modbus TCP".to_string()]
            .into_iter()
            .collect(),
        minimum_manufacturers_per_protocol: 1,
        minimum_models_per_protocol: 1,
        minimum_manufacturers_by_protocol: BTreeMap::from([
            ("S7".to_string(), 1),
            ("modbus-tcp".to_string(), 1),
        ]),
        minimum_models_by_protocol: BTreeMap::from([
            ("Siemens S7".to_string(), 2),
            ("Modbus TCP".to_string(), 1),
        ]),
        minimum_duration_ms: DAY_MS,
        maximum_failure_ratio: 0.01,
        maximum_progress_gap_ms: 300_000,
    };

    let report = evaluate_field_interoperability(&policy, &evidence).unwrap();

    assert_eq!(report.status, FieldInteroperabilityStatus::Passed);
    assert_eq!(report.schema_version, 4);
    let s7 = report
        .protocols
        .iter()
        .find(|protocol| protocol.protocol == "Siemens S7")
        .unwrap();
    assert_eq!(s7.required_manufacturer_count, 1);
    assert_eq!(s7.observed_manufacturer_count, 1);
    assert_eq!(s7.required_model_count, 2);
    assert_eq!(s7.observed_model_count, 2);
    assert!(s7.satisfied);
}

#[test]
fn rejects_a_protocol_that_does_not_meet_its_model_requirement() {
    let evidence = vec![
        evidence(
            "s7-1200-a.json",
            "Siemens S7",
            "Siemens",
            "S7-1200",
            "S7-001",
        ),
        evidence(
            "s7-1200-b.json",
            "Siemens S7",
            "Siemens",
            "S7-1200",
            "S7-002",
        ),
    ];
    let policy = FieldInteroperabilityPolicy {
        required_protocols: ["Siemens S7".to_string()].into_iter().collect(),
        minimum_manufacturers_per_protocol: 1,
        minimum_models_per_protocol: 1,
        minimum_manufacturers_by_protocol: BTreeMap::new(),
        minimum_models_by_protocol: BTreeMap::from([("S7".to_string(), 2)]),
        minimum_duration_ms: DAY_MS,
        maximum_failure_ratio: 0.01,
        maximum_progress_gap_ms: 300_000,
    };

    let report = evaluate_field_interoperability(&policy, &evidence).unwrap();

    assert_eq!(report.status, FieldInteroperabilityStatus::Failed);
    assert_eq!(report.protocols[0].observed_model_count, 1);
    assert!(!report.protocols[0].satisfied);
}

#[test]
fn duplicate_report_and_device_cannot_inflate_vendor_coverage() {
    let first = evidence("iec-a.json", "IEC-104", "Vendor A", "RTU-1", "A-001");
    let duplicate_content = evidence("iec-a-copy.json", "IEC-104", "Vendor A", "RTU-1", "A-001");
    let same_device = evidence("iec-a-rerun.json", "IEC-104", "Vendor A", "RTU-1", "A-001");

    let report = evaluate_field_interoperability(
        &FieldInteroperabilityPolicy {
            required_protocols: ["IEC-104".to_string()].into_iter().collect(),
            ..policy()
        },
        &[first, duplicate_content, same_device],
    )
    .unwrap();

    assert_eq!(report.status, FieldInteroperabilityStatus::Failed);
    assert_eq!(report.protocols[0].observed_manufacturer_count, 1);
    assert!(!report.rejected_evidence.is_empty());
}

#[test]
fn rejects_short_non_physical_or_incomplete_mqtt_evidence() {
    let mut short = valid_report("IEC-104", "Vendor A", "RTU-1", "A-001");
    short["observedDurationMs"] = json!(60_000);
    short["physicalDeviceExercised"] = json!(false);
    short["mqtt"]["publishSuccessCount"] = json!(0);
    short["criteria"]["mqttPubackComplete"] = json!(false);
    let evidence = artifact_evidence("short.json", short, "A-001");

    let report = evaluate_field_interoperability(
        &FieldInteroperabilityPolicy {
            required_protocols: ["IEC-104".to_string()].into_iter().collect(),
            ..policy()
        },
        &[evidence],
    )
    .unwrap();

    assert_eq!(report.status, FieldInteroperabilityStatus::Failed);
    let reasons = report.rejected_evidence[0].reasons.join("; ");
    assert!(reasons.contains("not physical"));
    assert!(reasons.contains("duration"));
    assert!(reasons.contains("MQTT PUBACK"));
}

#[test]
fn rejects_legacy_field_report_schema_v3() {
    let mut report = valid_report("IEC-104", "Vendor A", "RTU-1", "A-001");
    report["schemaVersion"] = json!(3);
    let evidence = artifact_evidence("legacy-v3.json", report, "A-001");

    let result = evaluate_field_interoperability(
        &FieldInteroperabilityPolicy {
            required_protocols: ["IEC-104".to_string()].into_iter().collect(),
            ..policy()
        },
        &[evidence],
    )
    .unwrap();

    assert_eq!(result.status, FieldInteroperabilityStatus::Failed);
    assert!(result.rejected_evidence[0]
        .reasons
        .iter()
        .any(|reason| reason.contains("version 4 is required")));
}

#[test]
fn rejects_protocol_or_mqtt_progress_stalls() {
    let mut report = valid_report("IEC-104", "Vendor A", "RTU-1", "A-001");
    report["protocolAcceptance"][0]["maximumObservedSuccessGapMs"] = json!(300_001);
    report["protocolAcceptance"][0]["continuousActivity"] = json!(false);
    report["protocolAcceptance"][0]["passed"] = json!(false);
    report["mqtt"]["sinkAcceptance"][0]["maximumObservedSuccessGapMs"] = json!(300_001);
    report["mqtt"]["sinkAcceptance"][0]["continuousActivity"] = json!(false);
    report["mqtt"]["sinkAcceptance"][0]["passed"] = json!(false);
    report["criteria"]["protocolsIndividuallyHealthy"] = json!(false);
    report["criteria"]["mqttSinksContinuouslyPublishing"] = json!(false);
    let evidence = artifact_evidence("stalled.json", report, "A-001");

    let result = evaluate_field_interoperability(
        &FieldInteroperabilityPolicy {
            required_protocols: ["IEC-104".to_string()].into_iter().collect(),
            ..policy()
        },
        &[evidence],
    )
    .unwrap();

    let reasons = result.rejected_evidence[0].reasons.join("; ");
    assert!(reasons.contains("did not maintain continuous collection progress"));
    assert!(reasons.contains("did not maintain continuous publish progress"));
}

#[test]
fn rejects_evidence_that_relaxes_the_policy_progress_gap() {
    let mut report = valid_report("IEC-104", "Vendor A", "RTU-1", "A-001");
    report["protocolAcceptance"][0]["maximumAllowedSuccessGapMs"] = json!(600_000);
    report["mqtt"]["sinkAcceptance"][0]["maximumAllowedSuccessGapMs"] = json!(600_000);
    let evidence = artifact_evidence("relaxed-gap.json", report, "A-001");

    let result = evaluate_field_interoperability(
        &FieldInteroperabilityPolicy {
            required_protocols: ["IEC-104".to_string()].into_iter().collect(),
            maximum_progress_gap_ms: 300_000,
            ..policy()
        },
        &[evidence],
    )
    .unwrap();

    let reasons = result.rejected_evidence[0].reasons.join("; ");
    assert!(reasons.contains("allows a success gap of 600000 ms"));
}

#[test]
fn rejects_connected_protocol_without_successful_collection_activity() {
    let mut report = valid_report("IEC-104", "Vendor A", "RTU-1", "A-001");
    report["protocols"][0]["collection_attempt_count"] = json!(100);
    report["protocols"][0]["collection_success_count"] = json!(0);
    report["criteria"]["protocolActivityObserved"] = json!(false);
    let evidence = artifact_evidence("inactive.json", report, "A-001");

    let result = evaluate_field_interoperability(
        &FieldInteroperabilityPolicy {
            required_protocols: ["IEC-104".to_string()].into_iter().collect(),
            ..policy()
        },
        &[evidence],
    )
    .unwrap();

    assert_eq!(result.status, FieldInteroperabilityStatus::Failed);
    let reasons = result.rejected_evidence[0].reasons.join("; ");
    assert!(reasons.contains("no successful collection activity"));
    assert!(reasons.contains("mandatory field endurance criteria"));
}

#[test]
fn rejects_unhealthy_bound_connection_even_when_global_failure_ratio_is_low() {
    let mut report = valid_report("IEC-104", "Vendor A", "RTU-1", "A-001");
    report["protocols"][0]["collection_attempt_count"] = json!(100);
    report["protocols"][0]["collection_success_count"] = json!(50);
    report["protocolAcceptance"][0]["collectionAttemptCount"] = json!(100);
    report["protocolAcceptance"][0]["collectionSuccessCount"] = json!(50);
    report["protocolAcceptance"][0]["collectionFailureCount"] = json!(50);
    report["protocolAcceptance"][0]["failureRatio"] = json!(0.5);
    let evidence = artifact_evidence("diluted-ratio.json", report, "A-001");

    let result = evaluate_field_interoperability(
        &FieldInteroperabilityPolicy {
            required_protocols: ["IEC-104".to_string()].into_iter().collect(),
            ..policy()
        },
        &[evidence],
    )
    .unwrap();

    assert_eq!(result.status, FieldInteroperabilityStatus::Failed);
    let reasons = result.rejected_evidence[0].reasons.join("; ");
    assert!(reasons.contains("protocol connection primary failure ratio 0.5 exceeds 0.01"));
}

#[test]
fn rejects_report_that_omits_an_enabled_package_connection() {
    let report = valid_report("IEC-104", "Vendor A", "RTU-1", "A-001");
    let mut primary = ProtocolConnection::simulated("primary");
    primary.protocol = ProtocolType::Iec104;
    let mut secondary = ProtocolConnection::simulated("secondary");
    secondary.protocol = ProtocolType::ModbusTcp;
    let package = EdgeConfigPackage::new("edge-A-001", "v1.2.3")
        .with_protocol_connection(primary)
        .with_protocol_connection(secondary)
        .with_data_config(field_data_config("primary-flow", "primary"))
        .with_data_config(field_data_config("secondary-flow", "secondary"));
    let (report_bytes, package_bytes, receipt_bytes) =
        artifact_bytes_with_package(report, package, "A-001");
    let audit_bytes = native_broker_audit_bytes(&receipt_bytes);
    let evidence = FieldInteroperabilityEvidence::from_artifacts(
        "omitted-connection.json",
        &report_bytes,
        &package_bytes,
        &receipt_bytes,
        &audit_bytes,
    )
    .unwrap();

    let result = evaluate_field_interoperability(
        &FieldInteroperabilityPolicy {
            required_protocols: ["IEC-104".to_string()].into_iter().collect(),
            ..policy()
        },
        &[evidence],
    )
    .unwrap();

    assert_eq!(result.status, FieldInteroperabilityStatus::Failed);
    let reasons = result.rejected_evidence[0].reasons.join("; ");
    assert!(reasons.contains("do not match enabled package connections"));
}

#[test]
fn physical_identity_counts_only_its_bound_protocol_connection() {
    let mut report = valid_report("DL/T645", "Vendor G", "Meter-645-A", "G-001");
    report["protocols"].as_array_mut().unwrap().push(json!({
        "connection_id": "iec101-secondary",
        "protocol": "IEC-101",
        "connected": true,
        "collection_attempt_count": 86_400,
        "collection_success_count": 86_390,
        "circuit_state": "Closed",
        "write_attempt_count": 0,
        "write_success_count": 0
    }));
    let evidence = artifact_evidence("multi-protocol.json", report, "G-001");
    let policy = FieldInteroperabilityPolicy {
        required_protocols: ["DL/T 645-2007".to_string(), "IEC-101".to_string()]
            .into_iter()
            .collect(),
        ..policy()
    };

    let result = evaluate_field_interoperability(&policy, &[evidence]).unwrap();

    assert_eq!(result.status, FieldInteroperabilityStatus::Failed);
    assert_eq!(result.summary.accepted_evidence_count, 1);
    assert!(result.rejected_evidence.is_empty());
    let dlt645 = result
        .protocols
        .iter()
        .find(|protocol| protocol.protocol == "DL/T 645-2007")
        .unwrap();
    let iec101 = result
        .protocols
        .iter()
        .find(|protocol| protocol.protocol == "IEC-101")
        .unwrap();
    assert_eq!(dlt645.observed_manufacturer_count, 1);
    assert_eq!(dlt645.accepted_runs[0].connection_id, "primary");
    assert_eq!(iec101.observed_manufacturer_count, 0);
}

#[test]
fn rejects_protocol_metrics_that_disagree_with_the_bound_package_connection() {
    let report = valid_report("DL/T645", "Vendor G", "Meter-645-A", "G-001");
    let mut connection = ProtocolConnection::simulated("primary");
    connection.protocol = ProtocolType::Iec101;
    let package =
        EdgeConfigPackage::new("edge-G-001", "v1.2.3").with_protocol_connection(connection);
    let (report_bytes, package_bytes, receipt_bytes) =
        artifact_bytes_with_package(report, package, "G-001");
    let audit_bytes = native_broker_audit_bytes(&receipt_bytes);
    let evidence = FieldInteroperabilityEvidence::from_artifacts(
        "protocol-mismatch.json",
        &report_bytes,
        &package_bytes,
        &receipt_bytes,
        &audit_bytes,
    )
    .unwrap();

    let result = evaluate_field_interoperability(
        &FieldInteroperabilityPolicy {
            required_protocols: ["DL/T 645-2007".to_string()].into_iter().collect(),
            ..policy()
        },
        &[evidence],
    )
    .unwrap();

    assert_eq!(result.status, FieldInteroperabilityStatus::Failed);
    assert!(result.rejected_evidence[0]
        .reasons
        .iter()
        .any(|reason| reason.contains("does not match bound package protocol IEC-101")));
}

#[test]
fn malformed_field_report_is_rejected_before_evaluation() {
    let error = FieldInteroperabilityEvidence::from_slice("bad.json", br#"{"status":"passed"}"#)
        .unwrap_err()
        .to_string();

    assert!(error.contains("decode field endurance evidence bad.json"));
}

#[test]
fn rejects_a_configuration_package_whose_bytes_do_not_match_the_report_digest() {
    let (report_bytes, mut package_bytes, receipt_bytes) = artifact_bytes(
        valid_report("IEC-104", "Vendor A", "RTU-1", "A-001"),
        "A-001",
    );
    package_bytes.push(b'\n');
    let audit_bytes = native_broker_audit_bytes(&receipt_bytes);
    let evidence = FieldInteroperabilityEvidence::from_artifacts(
        "tampered-package.json",
        &report_bytes,
        &package_bytes,
        &receipt_bytes,
        &audit_bytes,
    )
    .unwrap();

    let result = evaluate_field_interoperability(
        &FieldInteroperabilityPolicy {
            required_protocols: ["IEC-104".to_string()].into_iter().collect(),
            ..policy()
        },
        &[evidence],
    )
    .unwrap();

    let reasons = result.rejected_evidence[0].reasons.join("; ");
    assert!(reasons.contains("package digest does not match"));
}

#[test]
fn rejects_broker_receipt_with_wrong_count_or_missing_runtime_route() {
    let (report_bytes, package_bytes, receipt_bytes) = artifact_bytes(
        valid_report("IEC-104", "Vendor A", "RTU-1", "A-001"),
        "A-001",
    );
    let mut receipt: Value = serde_json::from_slice(&receipt_bytes).unwrap();
    receipt["messageCount"] = json!(10);
    receipt["routes"][0]["messageCount"] = json!(10);
    receipt["routes"][0]["topics"] = json!(["field/other/telemetry"]);
    let receipt_bytes = serde_json::to_vec(&receipt).unwrap();
    let audit_bytes = native_broker_audit_bytes(&receipt_bytes);
    let evidence = FieldInteroperabilityEvidence::from_artifacts(
        "bad-broker-receipt.json",
        &report_bytes,
        &package_bytes,
        &receipt_bytes,
        &audit_bytes,
    )
    .unwrap();

    let result = evaluate_field_interoperability(
        &FieldInteroperabilityPolicy {
            required_protocols: ["IEC-104".to_string()].into_iter().collect(),
            ..policy()
        },
        &[evidence],
    )
    .unwrap();

    let reasons = result.rejected_evidence[0].reasons.join("; ");
    assert!(reasons.contains("message count 10 does not match Runtime publish success count"));
    assert!(reasons.contains("does not contain Runtime route"));
}

#[test]
fn rejects_unstructured_or_cross_campaign_native_broker_audit() {
    let (report_bytes, package_bytes, receipt_bytes) = artifact_bytes(
        valid_report("IEC-104", "Vendor A", "RTU-1", "A-001"),
        "A-001",
    );
    let legacy_error = FieldInteroperabilityEvidence::from_artifacts(
        "legacy-native-audit.json",
        &report_bytes,
        &package_bytes,
        &receipt_bytes,
        br#"{"event":"message_delivered","signed":true}"#,
    )
    .unwrap_err()
    .to_string();
    assert!(legacy_error.contains("decode structured native broker audit"));

    let mut audit: Value =
        serde_json::from_slice(&native_broker_audit_bytes(&receipt_bytes)).unwrap();
    audit["edgeId"] = json!("edge-from-another-campaign");
    audit["messageCount"] = json!(1);
    audit["routes"][0]["messageCount"] = json!(1);
    audit["routes"][0]["topics"] = json!(["field/other/telemetry"]);
    let evidence = FieldInteroperabilityEvidence::from_artifacts(
        "cross-campaign-native-audit.json",
        &report_bytes,
        &package_bytes,
        &receipt_bytes,
        &serde_json::to_vec(&audit).unwrap(),
    )
    .unwrap();

    let result = evaluate_field_interoperability(
        &FieldInteroperabilityPolicy {
            required_protocols: ["IEC-104".to_string()].into_iter().collect(),
            ..policy()
        },
        &[evidence],
    )
    .unwrap();
    let reasons = result.rejected_evidence[0].reasons.join("; ");
    assert!(reasons.contains("edge id or version does not match the broker receipt"));
    assert!(reasons.contains("message count 1 does not match broker receipt count 86400"));
    assert!(reasons.contains("routes, topics or counts do not match the broker receipt"));
}

#[test]
fn rejects_aggregate_mqtt_counts_without_matching_sink_details() {
    let mut report = valid_report("IEC-104", "Vendor A", "RTU-1", "A-001");
    report["mqtt"]["sinks"] = json!([]);
    let evidence = artifact_evidence("missing-sinks.json", report, "A-001");

    let result = evaluate_field_interoperability(
        &FieldInteroperabilityPolicy {
            required_protocols: ["IEC-104".to_string()].into_iter().collect(),
            ..policy()
        },
        &[evidence],
    )
    .unwrap();

    let reasons = result.rejected_evidence[0].reasons.join("; ");
    assert!(reasons.contains("sink-level evidence is required"));
}

#[test]
fn retains_multiple_broker_routes_for_a_multi_output_runtime() {
    let mut source_report = valid_report("IEC-104", "Vendor A", "RTU-1", "A-001");
    source_report["mqtt"]["publishSuccessCount"] = json!(100);
    source_report["mqtt"]["connectedSinkCount"] = json!(2);
    source_report["mqtt"]["sinks"] = json!([
        {
            "sink_id": "field-sink",
            "broker": "mqtt://velamq-a.example:1883",
            "connected": true,
            "publish_success_count": 40,
            "last_topic": "factory/a/telemetry"
        },
        {
            "sink_id": "secondary-sink",
            "broker": "mqtts://velamq-b.example:8883",
            "connected": true,
            "publish_success_count": 60,
            "last_topic": "factory/b/telemetry"
        }
    ]);
    source_report["mqtt"]["sinkAcceptance"] = json!([
        {
            "sinkId": "field-sink",
            "publishSuccessCount": 40,
            "maximumObservedSuccessGapMs": 2_000,
            "maximumAllowedSuccessGapMs": 300_000,
            "counterResetObserved": false,
            "continuousActivity": true,
            "passed": true
        },
        {
            "sinkId": "secondary-sink",
            "publishSuccessCount": 60,
            "maximumObservedSuccessGapMs": 2_000,
            "maximumAllowedSuccessGapMs": 300_000,
            "counterResetObserved": false,
            "continuousActivity": true,
            "passed": true
        }
    ]);
    let mut connection = ProtocolConnection::simulated("primary");
    connection.protocol = ProtocolType::Iec104;
    let package = EdgeConfigPackage::new("edge-A-001", "v1.2.3")
        .with_protocol_connection(connection)
        .with_data_config(field_data_config("field-primary", "primary"))
        .with_data_config(field_data_config_with_sink(
            "field-secondary",
            "primary",
            "secondary-sink",
        ));
    let (report_bytes, package_bytes, receipt_bytes) =
        artifact_bytes_with_package(source_report, package, "A-001");
    let mut receipt: Value = serde_json::from_slice(&receipt_bytes).unwrap();
    receipt["routes"] = json!([
        {
            "broker": "mqtt://velamq-a.example:1883",
            "consumerId": "audit-a",
            "messageCount": 40,
            "topics": ["factory/a/telemetry"]
        },
        {
            "broker": "mqtts://velamq-b.example:8883",
            "consumerId": "audit-b",
            "messageCount": 60,
            "topics": ["factory/b/telemetry"]
        }
    ]);
    let receipt_bytes = serde_json::to_vec(&receipt).unwrap();
    let audit_bytes = native_broker_audit_bytes(&receipt_bytes);
    let evidence = FieldInteroperabilityEvidence::from_artifacts(
        "multi-output.json",
        &report_bytes,
        &package_bytes,
        &receipt_bytes,
        &audit_bytes,
    )
    .unwrap();

    let result = evaluate_field_interoperability(
        &FieldInteroperabilityPolicy {
            required_protocols: ["IEC-104".to_string()].into_iter().collect(),
            ..policy()
        },
        &[evidence],
    )
    .unwrap();

    assert!(result.rejected_evidence.is_empty());
    let run = &result.protocols[0].accepted_runs[0];
    assert_eq!(run.broker_message_count, 100);
    assert_eq!(run.broker_routes.len(), 2);
    assert_eq!(run.native_broker, "VelaMQ");
    assert_eq!(run.native_broker_instance_id, "velamq-node-a");
    assert_eq!(run.native_broker_audit_id, "audit-edge-A-001");
}

#[test]
fn cli_reads_reports_and_writes_a_machine_verifiable_matrix() {
    let directory = tempdir().unwrap();
    let artifacts = [
        (
            "dlt645-a.json",
            "DL/T645",
            "Vendor G",
            "Meter-645-A",
            "G-001",
        ),
        (
            "dlt645-b.json",
            "DL/T 645-2007",
            "Vendor H",
            "Meter-645-B",
            "H-001",
        ),
        ("iec101-a.json", "IEC-101", "Vendor E", "RTU-101-A", "E-001"),
        (
            "iec101-b.json",
            "IEC 60870-5-101",
            "Vendor F",
            "RTU-101-B",
            "F-001",
        ),
        ("iec-a.json", "IEC-104", "Vendor A", "RTU-1", "A-001"),
        ("iec-b.json", "IEC-104", "Vendor B", "RTU-2", "B-001"),
        ("opc-a.json", "OPC UA", "Vendor C", "Server-1", "C-001"),
        ("opc-b.json", "OPC UA", "Vendor D", "Server-2", "D-001"),
    ]
    .into_iter()
    .map(|(file, protocol, manufacturer, model, serial)| {
        let report_path = directory.path().join(file);
        let package_path = directory.path().join(format!("{file}.package.json"));
        let receipt_path = directory.path().join(format!("{file}.broker-receipt.json"));
        let audit_path = directory.path().join(format!("{file}.broker-audit.json"));
        let (report_bytes, package_bytes, receipt_bytes) =
            artifact_bytes(valid_report(protocol, manufacturer, model, serial), serial);
        fs::write(&report_path, report_bytes).unwrap();
        fs::write(&package_path, package_bytes).unwrap();
        let audit_bytes = native_broker_audit_bytes(&receipt_bytes);
        fs::write(&receipt_path, receipt_bytes).unwrap();
        fs::write(&audit_path, audit_bytes).unwrap();
        (report_path, package_path, receipt_path, audit_path)
    })
    .collect::<Vec<_>>();
    let output = directory.path().join("matrix.json");
    let mut command = Command::new(env!("CARGO_BIN_EXE_field-interoperability-gate"));
    for (report, package, receipt, audit) in &artifacts {
        command
            .arg("--report")
            .arg(report)
            .arg("--package")
            .arg(package)
            .arg("--broker-receipt")
            .arg(receipt)
            .arg("--native-broker-audit")
            .arg(audit);
    }
    let result = command.arg("--output").arg(&output).output().unwrap();

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let matrix: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(matrix["status"], "passed");
    assert_eq!(matrix["summary"]["acceptedEvidenceCount"], 8);
    assert_eq!(matrix["summary"]["satisfiedProtocolCount"], 4);
}

#[test]
fn cli_loads_a_versioned_per_protocol_policy() {
    let directory = tempdir().unwrap();
    let report_path = directory.path().join("modbus-tcp.json");
    let package_path = directory.path().join("modbus-tcp.package.json");
    let receipt_path = directory.path().join("modbus-tcp.broker-receipt.json");
    let audit_path = directory.path().join("modbus-tcp.broker-audit.json");
    let policy_path = directory.path().join("field-policy.json");
    let output = directory.path().join("matrix.json");
    let (report, package, receipt) = artifact_bytes(
        valid_report("Modbus TCP", "Acme Controls", "MC-100", "MB-001"),
        "MB-001",
    );
    fs::write(&report_path, report).unwrap();
    fs::write(&package_path, package).unwrap();
    let audit = native_broker_audit_bytes(&receipt);
    fs::write(&receipt_path, receipt).unwrap();
    fs::write(&audit_path, audit).unwrap();
    fs::write(
        &policy_path,
        serde_json::to_vec_pretty(&json!({
            "schemaVersion": 1,
            "minimumDurationSeconds": 86_400,
            "maximumFailureRatio": 0.01,
            "protocols": [{
                "protocol": "modbus_tcp",
                "minimumManufacturers": 1,
                "minimumModels": 1
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_field-interoperability-gate"))
        .arg("--report")
        .arg(&report_path)
        .arg("--package")
        .arg(&package_path)
        .arg("--broker-receipt")
        .arg(&receipt_path)
        .arg("--native-broker-audit")
        .arg(&audit_path)
        .arg("--policy")
        .arg(&policy_path)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let matrix: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(matrix["schemaVersion"], 4);
    assert_eq!(matrix["status"], "passed");
    assert_eq!(matrix["protocols"][0]["protocol"], "Modbus TCP");
    assert_eq!(matrix["protocols"][0]["requiredManufacturerCount"], 1);
    assert_eq!(matrix["protocols"][0]["requiredModelCount"], 1);
}

#[test]
fn cli_rejects_mixing_a_policy_file_with_legacy_threshold_arguments() {
    let result = Command::new(env!("CARGO_BIN_EXE_field-interoperability-gate"))
        .arg("--policy")
        .arg("deploy/field-acceptance-policy.json")
        .arg("--minimum-models-per-protocol")
        .arg("2")
        .output()
        .unwrap();

    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("cannot be used with"));
    assert!(stderr.contains("--policy"));
    assert!(stderr.contains("--minimum-models-per-protocol"));
}

#[test]
fn cli_reads_hash_bound_campaign_directories() {
    let directory = tempdir().unwrap();
    let campaigns = [
        ("dlt645-a", "DL/T645", "Vendor G", "Meter-645-A", "G-001"),
        (
            "dlt645-b",
            "DL/T 645-2007",
            "Vendor H",
            "Meter-645-B",
            "H-001",
        ),
        ("iec101-a", "IEC-101", "Vendor E", "RTU-101-A", "E-001"),
        (
            "iec101-b",
            "IEC 60870-5-101",
            "Vendor F",
            "RTU-101-B",
            "F-001",
        ),
        ("iec104-a", "IEC-104", "Vendor A", "RTU-1", "A-001"),
        ("iec104-b", "IEC-104", "Vendor B", "RTU-2", "B-001"),
        ("opcua-a", "OPC UA", "Vendor C", "Server-1", "C-001"),
        ("opcua-b", "OPC UA", "Vendor D", "Server-2", "D-001"),
    ]
    .into_iter()
    .map(|(name, protocol, manufacturer, model, serial)| {
        write_campaign_directory(
            directory.path(),
            name,
            protocol,
            manufacturer,
            model,
            serial,
        )
    })
    .collect::<Vec<_>>();
    let output = directory.path().join("campaign-matrix.json");
    let mut command = Command::new(env!("CARGO_BIN_EXE_field-interoperability-gate"));
    for campaign in &campaigns {
        command.arg("--campaign-dir").arg(campaign);
    }
    let result = command.arg("--output").arg(&output).output().unwrap();

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let matrix: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(matrix["status"], "passed");
    assert_eq!(matrix["summary"]["acceptedEvidenceCount"], 8);
}

#[test]
fn cli_rejects_a_campaign_artifact_tampered_after_manifest_creation() {
    let directory = tempdir().unwrap();
    let campaign = write_campaign_directory(
        directory.path(),
        "iec104-a",
        "IEC-104",
        "Vendor A",
        "RTU-1",
        "A-001",
    );
    fs::write(campaign.join("broker-receipt.json"), b"{}").unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_field-interoperability-gate"))
        .arg("--campaign-dir")
        .arg(&campaign)
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("digest does not match manifest"));
}

#[test]
fn cli_rejects_a_native_broker_audit_tampered_after_manifest_creation() {
    let directory = tempdir().unwrap();
    let campaign = write_campaign_directory(
        directory.path(),
        "iec104-a",
        "IEC-104",
        "Vendor A",
        "RTU-1",
        "A-001",
    );
    fs::write(campaign.join("native-broker-audit.json"), b"tampered").unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_field-interoperability-gate"))
        .arg("--campaign-dir")
        .arg(&campaign)
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("digest does not match manifest"));
}

#[test]
fn cli_rejects_legacy_campaign_manifest_without_bound_native_audit() {
    let directory = tempdir().unwrap();
    let campaign = write_campaign_directory(
        directory.path(),
        "iec104-a",
        "IEC-104",
        "Vendor A",
        "RTU-1",
        "A-001",
    );
    let manifest_path = campaign.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["schemaVersion"] = json!(1);
    manifest
        .as_object_mut()
        .unwrap()
        .remove("nativeBrokerAudit");
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_field-interoperability-gate"))
        .arg("--campaign-dir")
        .arg(&campaign)
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unsupported manifest schema 1"));
}

fn write_campaign_directory(
    root: &std::path::Path,
    name: &str,
    protocol: &str,
    manufacturer: &str,
    model: &str,
    serial: &str,
) -> std::path::PathBuf {
    let campaign = root.join(name);
    fs::create_dir_all(&campaign).unwrap();
    let (report, package, receipt) =
        artifact_bytes(valid_report(protocol, manufacturer, model, serial), serial);
    let audit = native_broker_audit_bytes(&receipt);
    fs::write(campaign.join("runtime-report.json"), &report).unwrap();
    fs::write(campaign.join("configuration-package.json"), &package).unwrap();
    fs::write(campaign.join("broker-receipt.json"), &receipt).unwrap();
    fs::write(campaign.join("native-broker-audit.json"), &audit).unwrap();
    let manifest = json!({
        "schemaVersion": 3,
        "status": "passed",
        "phase": "complete",
        "edgeId": format!("edge-{serial}"),
        "configVersion": "v1.2.3",
        "startedAt": "2026-07-18T00:00:00Z",
        "finishedAt": "2026-07-19T00:00:00Z",
        "package": {
            "file": "configuration-package.json",
            "sha256": format!("{:x}", Sha256::digest(&package))
        },
        "runtimeReport": {
            "file": "runtime-report.json",
            "sha256": format!("{:x}", Sha256::digest(&report))
        },
        "brokerReceipt": {
            "file": "broker-receipt.json",
            "sha256": format!("{:x}", Sha256::digest(&receipt))
        },
        "nativeBrokerAudit": {
            "file": "native-broker-audit.json",
            "sha256": format!("{:x}", Sha256::digest(&audit))
        },
        "nativeBrokerAuditRequired": true,
        "errors": []
    });
    fs::write(
        campaign.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    campaign
}

#[test]
fn cli_rejects_artifact_lists_that_are_not_position_matched() {
    let directory = tempdir().unwrap();
    let (report_bytes, package_bytes, receipt_bytes) = artifact_bytes(
        valid_report("IEC-104", "Vendor A", "RTU-1", "A-001"),
        "A-001",
    );
    let report = directory.path().join("field.json");
    let package = directory.path().join("package.json");
    let receipt = directory.path().join("receipt.json");
    fs::write(&report, report_bytes).unwrap();
    fs::write(&package, package_bytes).unwrap();
    fs::write(&receipt, receipt_bytes).unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_field-interoperability-gate"))
        .arg("--report")
        .arg(&report)
        .arg("--package")
        .arg(&package)
        .arg("--broker-receipt")
        .arg(&receipt)
        .arg("--broker-receipt")
        .arg(&receipt)
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("position-matched"));
}
