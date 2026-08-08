use std::{fs, process::Command};

use edge_core::{
    DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint, DataConfigPublish,
    DeviceInstance, EdgeConfigPackage, MqttUplinkConfig, PointAddress, ProtocolConnection,
    TelemetryPointMapping, TelemetryType,
};
use edge_runtime::{
    evaluate_field_campaign_site_status, FieldCampaignExecutionStatus, FieldCampaignSiteStatus,
    FieldInteroperabilityPolicy,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

const SITE_ID: &str = "WO-2026-0042";
const OPERATOR: &str = "operator-a";
const MANUFACTURER: &str = "Vendor A";
const MODEL: &str = "Pump 100";
const SERIAL: &str = "ASSET-001";
const TOPIC: &str = "field/edge-a/pump-1/telemetry";
const BROKER: &str = "mqtt://192.0.2.20:1883";

#[test]
fn pending_site_is_valid_without_loading_runtime_secret_values() {
    let temp = tempdir().unwrap();
    let package_path = temp.path().join("package.json");
    let output_dir = temp.path().join("campaign-output");
    let audit_path = temp.path().join("broker-inbox/audit.json");
    let secret_name = "VELAEDGE_FIELD_STATUS_TEST_SECRET_8428F82C";
    assert!(std::env::var_os(secret_name).is_none());
    write_package(&package_path, Some(secret_name));
    let policy = policy();

    let report = evaluate_field_campaign_site_status(
        &plan_bytes(&package_path, &output_dir, &audit_path, MANUFACTURER),
        &policy,
        "policy-digest",
    )
    .unwrap();

    assert_eq!(report.status, FieldCampaignSiteStatus::Pending);
    assert_eq!(report.summary.pending_count, 1);
    assert_eq!(report.summary.running_count, 0);
    assert_eq!(report.summary.passed_count, 0);
    assert_eq!(
        report.campaigns[0].status,
        FieldCampaignExecutionStatus::Pending
    );
    assert!(report.plan_validation.passed());
    assert!(!output_dir.exists());
}

#[test]
fn running_manifest_is_reported_without_becoming_acceptance_evidence() {
    let temp = tempdir().unwrap();
    let package_path = temp.path().join("package.json");
    let output_dir = temp.path().join("campaign-output");
    let audit_path = temp.path().join("broker-inbox/audit.json");
    write_package(&package_path, None);
    fs::create_dir_all(&output_dir).unwrap();
    fs::write(
        output_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&json!({
            "schemaVersion": 3,
            "status": "running",
            "phase": "runtime_endurance",
            "edgeId": "edge-a",
            "configVersion": "v1.0.0",
            "startedAt": "2026-07-18T00:00:00Z",
            "finishedAt": "2026-07-18T01:00:00Z",
            "package": null,
            "runtimeReport": null,
            "brokerReceipt": null,
            "nativeBrokerAudit": null,
            "nativeBrokerAuditRequired": true,
            "errors": []
        }))
        .unwrap(),
    )
    .unwrap();

    let report = evaluate_field_campaign_site_status(
        &plan_bytes(&package_path, &output_dir, &audit_path, MANUFACTURER),
        &policy(),
        "policy-digest",
    )
    .unwrap();

    assert_eq!(report.status, FieldCampaignSiteStatus::Running);
    assert_eq!(report.summary.running_count, 1);
    assert_eq!(report.interoperability.summary.accepted_evidence_count, 0);
    assert_eq!(
        report.campaigns[0].status,
        FieldCampaignExecutionStatus::Running
    );
    assert_eq!(
        report.campaigns[0].phase.as_deref(),
        Some("runtime_endurance")
    );
}

#[test]
fn completed_hash_bound_campaign_passes_and_is_bound_to_the_plan() {
    let temp = tempdir().unwrap();
    let package_path = temp.path().join("package.json");
    let output_dir = temp.path().join("campaign-output");
    let audit_path = temp.path().join("broker-inbox/audit.json");
    let package_bytes = write_package(&package_path, None);
    write_completed_campaign(&output_dir, &package_bytes);
    let policy = policy();

    let report = evaluate_field_campaign_site_status(
        &plan_bytes(&package_path, &output_dir, &audit_path, MANUFACTURER),
        &policy,
        "policy-digest",
    )
    .unwrap();

    assert_eq!(report.status, FieldCampaignSiteStatus::Passed);
    assert_eq!(report.summary.passed_count, 1);
    assert_eq!(report.summary.satisfied_protocol_count, 1);
    assert_eq!(
        report.campaigns[0].status,
        FieldCampaignExecutionStatus::Passed
    );
    assert!(report.errors.is_empty());

    let mismatched = evaluate_field_campaign_site_status(
        &plan_bytes(&package_path, &output_dir, &audit_path, "Different Vendor"),
        &policy,
        "policy-digest",
    )
    .unwrap();
    assert_eq!(mismatched.status, FieldCampaignSiteStatus::Failed);
    assert_eq!(mismatched.summary.failed_count, 1);
    assert!(mismatched.campaigns[0]
        .reasons
        .iter()
        .any(|reason| reason.contains("completed manufacturer")));
}

#[test]
fn cli_allows_observation_but_can_require_final_completion() {
    let temp = tempdir().unwrap();
    let package_path = temp.path().join("package.json");
    let output_dir = temp.path().join("campaign-output");
    let audit_path = temp.path().join("broker-inbox/audit.json");
    let plan_path = temp.path().join("plan.json");
    let policy_path = temp.path().join("policy.json");
    let report_path = temp.path().join("site-status.json");
    write_package(&package_path, None);
    fs::write(
        &plan_path,
        plan_bytes(&package_path, &output_dir, &audit_path, MANUFACTURER),
    )
    .unwrap();
    fs::write(&policy_path, policy_bytes()).unwrap();

    let observation = Command::new(env!("CARGO_BIN_EXE_field-campaign-status"))
        .arg("--plan")
        .arg(&plan_path)
        .arg("--policy")
        .arg(&policy_path)
        .arg("--output")
        .arg(&report_path)
        .output()
        .unwrap();
    assert!(
        observation.status.success(),
        "{}",
        String::from_utf8_lossy(&observation.stderr)
    );
    let report: Value = serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
    assert_eq!(report["status"], "pending");

    let final_gate = Command::new(env!("CARGO_BIN_EXE_field-campaign-status"))
        .arg("--plan")
        .arg(&plan_path)
        .arg("--policy")
        .arg(&policy_path)
        .arg("--output")
        .arg(&report_path)
        .arg("--require-complete")
        .output()
        .unwrap();
    assert!(!final_gate.status.success());
    assert!(String::from_utf8_lossy(&final_gate.stderr).contains("is not complete"));
    let retained: Value = serde_json::from_slice(&fs::read(report_path).unwrap()).unwrap();
    assert_eq!(retained["status"], "pending");
    assert!(fs::read_dir(temp.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".site-status.json.tmp-")
    }));
}

#[test]
fn deployment_periodically_observes_the_plan_and_requires_completion_at_release() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let service =
        fs::read_to_string(root.join("deploy/systemd/edgeops-field-campaign-status.service"))
            .unwrap();
    let timer = fs::read_to_string(root.join("deploy/systemd/edgeops-field-campaign-status.timer"))
        .unwrap();
    let release = fs::read_to_string(root.join("scripts/run-release-gates.sh")).unwrap();

    assert!(service.contains("Type=oneshot"));
    assert!(service.contains(
        "ExecStart=/opt/edgeops/bin/field-campaign-status --plan /etc/edgeops/field-campaign/site-plan.json"
    ));
    assert!(service.contains("UMask=0077"));
    assert!(timer.contains("OnUnitActiveSec=1min"));
    assert!(timer.contains("Persistent=true"));
    assert!(release.contains("EDGEOPS_FIELD_CAMPAIGN_PLAN"));
    assert!(release.contains("--bin field-campaign-status"));
    assert!(release.contains("--require-complete"));
    assert!(
        release.contains("EDGEOPS_FIELD_CAMPAIGN_PLAN is required for the site release profile")
    );
}

fn write_package(path: &std::path::Path, secret_name: Option<&str>) -> Vec<u8> {
    let address = PointAddress::modbus_holding_register(40001);
    let mut uplink = MqttUplinkConfig::velamq("field-main", BROKER, "mqtt-edge-a").with_qos(1);
    if let Some(secret_name) = secret_name {
        uplink = uplink.with_credentials_env("runtime", secret_name);
    }
    let package = EdgeConfigPackage::new("edge-a", "v1.0.0")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::modbus_tcp(
            "modbus-main",
            "192.0.2.10:502",
        ))
        .with_mqtt_uplink(uplink)
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
                DataConfigCollection::new(1_000),
                DataConfigPublish::new(
                    "field-main",
                    "field/{edge_id}/{device_id}/telemetry",
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
        );
    let bytes = serde_json::to_vec_pretty(&package).unwrap();
    fs::write(path, &bytes).unwrap();
    bytes
}

fn policy() -> FieldInteroperabilityPolicy {
    FieldInteroperabilityPolicy::from_json_slice(&policy_bytes()).unwrap()
}

fn policy_bytes() -> Vec<u8> {
    serde_json::to_vec_pretty(&json!({
        "schemaVersion": 1,
        "minimumDurationSeconds": 86_400,
        "maximumFailureRatio": 0.01,
        "maximumProgressGapSeconds": 300,
        "protocols": [{
            "protocol": "Modbus TCP",
            "minimumManufacturers": 1,
            "minimumModels": 1
        }]
    }))
    .unwrap()
}

fn plan_bytes(
    package: &std::path::Path,
    output: &std::path::Path,
    audit: &std::path::Path,
    manufacturer: &str,
) -> Vec<u8> {
    serde_json::to_vec_pretty(&json!({
        "schemaVersion": 1,
        "siteId": SITE_ID,
        "physicalDeviceConfirmed": true,
        "campaigns": [{
            "campaignId": "modbus-a",
            "operator": OPERATOR,
            "configPath": package,
            "outputDir": output,
            "nativeBrokerAuditPath": audit,
            "physicalDevice": {
                "connectionId": "modbus-main",
                "manufacturer": manufacturer,
                "model": MODEL,
                "serialNumber": SERIAL
            },
            "durationSeconds": 86_400,
            "schedulerIntervalMs": 100,
            "maximumFailureRatio": 0.01,
            "maximumProgressGapSeconds": 300,
            "changingPoints": ["pump-1/pressure"]
        }]
    }))
    .unwrap()
}

fn write_completed_campaign(output: &std::path::Path, package: &[u8]) {
    fs::create_dir_all(output).unwrap();
    let package_sha256 = format!("{:x}", Sha256::digest(package));
    let report = serde_json::to_vec_pretty(&valid_report(&package_sha256)).unwrap();
    let receipt = serde_json::to_vec_pretty(&json!({
        "schemaVersion": 1,
        "edgeId": "edge-a",
        "configVersion": "v1.0.0",
        "packageSha256": package_sha256,
        "firstReceivedAt": "2026-07-18T00:00:00Z",
        "lastReceivedAt": "2026-07-19T00:00:00Z",
        "messageCount": 86_400,
        "routes": [{
            "broker": BROKER,
            "consumerId": "field-audit-consumer",
            "messageCount": 86_400,
            "topics": [TOPIC]
        }]
    }))
    .unwrap();
    let audit = serde_json::to_vec_pretty(&json!({
        "schemaVersion": 1,
        "broker": "VelaMQ",
        "brokerInstanceId": "velamq-node-a",
        "auditId": "audit-edge-a",
        "exportedAt": "2026-07-19T00:00:01Z",
        "edgeId": "edge-a",
        "configVersion": "v1.0.0",
        "packageSha256": package_sha256,
        "firstObservedAt": "2026-07-18T00:00:00Z",
        "lastObservedAt": "2026-07-19T00:00:00Z",
        "messageCount": 86_400,
        "routes": [{
            "broker": BROKER,
            "consumerId": "field-audit-consumer",
            "messageCount": 86_400,
            "topics": [TOPIC]
        }]
    }))
    .unwrap();
    fs::write(output.join("configuration-package.json"), package).unwrap();
    fs::write(output.join("runtime-report.json"), &report).unwrap();
    fs::write(output.join("broker-receipt.json"), &receipt).unwrap();
    fs::write(output.join("native-broker-audit.json"), &audit).unwrap();
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&json!({
            "schemaVersion": 3,
            "status": "passed",
            "phase": "complete",
            "edgeId": "edge-a",
            "configVersion": "v1.0.0",
            "startedAt": "2026-07-18T00:00:00Z",
            "finishedAt": "2026-07-19T00:00:00Z",
            "package": artifact("configuration-package.json", package),
            "runtimeReport": artifact("runtime-report.json", &report),
            "brokerReceipt": artifact("broker-receipt.json", &receipt),
            "nativeBrokerAudit": artifact("native-broker-audit.json", &audit),
            "nativeBrokerAuditRequired": true,
            "errors": []
        }))
        .unwrap(),
    )
    .unwrap();
}

fn artifact(file: &str, bytes: &[u8]) -> Value {
    json!({
        "file": file,
        "sha256": format!("{:x}", Sha256::digest(bytes))
    })
}

fn valid_report(package_sha256: &str) -> Value {
    json!({
        "schemaVersion": 4,
        "status": "passed",
        "mode": "physical_field_endurance",
        "physicalDeviceExercised": true,
        "physicalDevice": {
            "siteId": SITE_ID,
            "operator": OPERATOR,
            "connectionId": "modbus-main",
            "manufacturer": MANUFACTURER,
            "model": MODEL,
            "serialNumber": SERIAL
        },
        "edgeId": "edge-a",
        "configVersion": "v1.0.0",
        "packageSha256": package_sha256,
        "configuredDurationMs": 86_400_000,
        "observedDurationMs": 86_401_000,
        "cycles": {
            "attempted": 86_400,
            "failureRatio": 0.0
        },
        "protocols": [{
            "connection_id": "modbus-main",
            "protocol": "Modbus TCP",
            "connected": true,
            "collection_attempt_count": 86_400,
            "collection_success_count": 86_400,
            "circuit_state": "Closed"
        }],
        "protocolAcceptance": [{
            "connectionId": "modbus-main",
            "protocol": "Modbus TCP",
            "connectedAtFinish": true,
            "circuitStateAtFinish": "Closed",
            "collectionAttemptCount": 86_400,
            "collectionSuccessCount": 86_400,
            "collectionFailureCount": 0,
            "failureRatio": 0.0,
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
                "sink_id": "field-main",
                "broker": BROKER,
                "connected": true,
                "publish_success_count": 86_400,
                "last_topic": TOPIC
            }],
            "sinkAcceptance": [{
                "sinkId": "field-main",
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
