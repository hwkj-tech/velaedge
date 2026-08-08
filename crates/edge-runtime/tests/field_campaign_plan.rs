use std::{fs, process::Command};

use edge_core::{
    DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint, DataConfigPublish,
    DeviceInstance, EdgeConfigPackage, MqttUplinkConfig, PointAddress, ProtocolConnection,
    TelemetryPointMapping, TelemetryType,
};
use edge_runtime::{
    evaluate_field_campaign_plan, FieldCampaignPlanStatus, FieldInteroperabilityPolicy,
};
use serde_json::{json, Value};
use tempfile::tempdir;

#[test]
fn complete_physical_inventory_passes_without_opening_sessions() {
    let temp = tempdir().unwrap();
    let package_path = temp.path().join("modbus-a.json");
    fs::write(
        &package_path,
        serde_json::to_vec_pretty(&physical_package("edge-a", "mqtt-edge-a")).unwrap(),
    )
    .unwrap();
    let output_dir = temp.path().join("evidence-a");
    let audit_path = temp.path().join("broker-inbox").join("audit-a.json");
    let plan = plan_bytes(vec![campaign(
        "modbus-a",
        &package_path,
        &output_dir,
        &audit_path,
        "Vendor A",
        "Pump 100",
        "ASSET-001",
    )]);
    let policy_bytes = policy_bytes(1, 1);
    let policy = FieldInteroperabilityPolicy::from_json_slice(&policy_bytes).unwrap();

    let report = evaluate_field_campaign_plan(&plan, &policy, "policy-digest").unwrap();

    assert_eq!(report.status, FieldCampaignPlanStatus::Passed);
    assert_eq!(report.summary.ready_campaign_count, 1);
    assert_eq!(report.summary.covered_protocol_count, 1);
    assert_eq!(report.campaigns[0].protocol.as_deref(), Some("Modbus TCP"));
    assert_eq!(report.campaigns[0].mqtt_routes.len(), 1);
    assert_eq!(report.campaigns[0].mqtt_routes[0].qos, 1);
    assert_eq!(
        report.campaigns[0].service_instance,
        "edgeops-field-campaign@modbus-a.service"
    );
    assert_eq!(
        report.campaigns[0].systemd_environment["EDGEOPS_FIELD_CAMPAIGN_PHYSICAL_DEVICE_CONFIRMED"],
        "1"
    );
    assert!(!output_dir.exists());
    assert!(!audit_path.exists());
}

#[test]
fn duplicate_assets_paths_and_insufficient_models_are_rejected() {
    let temp = tempdir().unwrap();
    let package_a = temp.path().join("modbus-a.json");
    let package_b = temp.path().join("modbus-b.json");
    fs::write(
        &package_a,
        serde_json::to_vec(&physical_package("edge-a", "mqtt-edge-a")).unwrap(),
    )
    .unwrap();
    fs::write(
        &package_b,
        serde_json::to_vec(&physical_package("edge-b", "mqtt-edge-b")).unwrap(),
    )
    .unwrap();
    let shared_output = temp.path().join("evidence-shared");
    let plan = plan_bytes(vec![
        campaign(
            "modbus-a",
            &package_a,
            &shared_output,
            &temp.path().join("audit-a.json"),
            "Vendor A",
            "Pump 100",
            "ASSET-001",
        ),
        campaign(
            "modbus-b",
            &package_b,
            &shared_output,
            &shared_output.join("audit-b.json"),
            "Vendor A",
            "Pump 100",
            "ASSET-001",
        ),
    ]);
    let policy = FieldInteroperabilityPolicy::from_json_slice(&policy_bytes(1, 2)).unwrap();

    let report = evaluate_field_campaign_plan(&plan, &policy, "policy-digest").unwrap();

    assert_eq!(report.status, FieldCampaignPlanStatus::Failed);
    assert_eq!(report.summary.ready_campaign_count, 0);
    assert!(report.campaigns.iter().all(|campaign| campaign
        .reasons
        .iter()
        .any(|reason| reason.contains("duplicate physical manufacturer/model/serial"))));
    assert!(report.campaigns.iter().all(|campaign| campaign
        .reasons
        .iter()
        .any(|reason| reason.contains("duplicate outputDir"))));
    assert!(report.campaigns[1]
        .reasons
        .iter()
        .any(|reason| reason.contains("outside every campaign outputDir")));
    assert!(report
        .errors
        .iter()
        .any(|error| error.contains("policy requires 1/2")));
}

#[test]
fn cli_retains_machine_readable_report() {
    let temp = tempdir().unwrap();
    let package_path = temp.path().join("modbus-a.json");
    let policy_path = temp.path().join("policy.json");
    let plan_path = temp.path().join("plan.json");
    let report_path = temp.path().join("report.json");
    fs::write(
        &package_path,
        serde_json::to_vec(&physical_package("edge-cli", "mqtt-edge-cli")).unwrap(),
    )
    .unwrap();
    fs::write(&policy_path, policy_bytes(1, 1)).unwrap();
    fs::write(
        &plan_path,
        plan_bytes(vec![campaign(
            "modbus-cli",
            &package_path,
            &temp.path().join("evidence-cli"),
            &temp.path().join("audit-cli.json"),
            "Vendor CLI",
            "Pump CLI",
            "ASSET-CLI",
        )]),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_field-campaign-plan"))
        .arg("--plan")
        .arg(&plan_path)
        .arg("--policy")
        .arg(&policy_path)
        .arg("--output")
        .arg(&report_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&fs::read(report_path).unwrap()).unwrap();
    assert_eq!(report["status"], "passed");
    assert_eq!(report["summary"]["readyCampaignCount"], 1);
    assert_eq!(
        report["campaigns"][0]["packageSha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
}

fn physical_package(edge_id: &str, client_id: &str) -> EdgeConfigPackage {
    let address = PointAddress::modbus_holding_register(40001);
    EdgeConfigPackage::new(edge_id, "v1.0.0")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::modbus_tcp(
            "modbus-main",
            "192.0.2.10:502",
        ))
        .with_mqtt_uplink(
            MqttUplinkConfig::velamq("field-main", "mqtt://192.0.2.20:1883", client_id).with_qos(1),
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
        )
}

fn policy_bytes(minimum_manufacturers: usize, minimum_models: usize) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schemaVersion": 1,
        "minimumDurationSeconds": 86400,
        "maximumFailureRatio": 0.01,
        "maximumProgressGapSeconds": 300,
        "protocols": [{
            "protocol": "Modbus TCP",
            "minimumManufacturers": minimum_manufacturers,
            "minimumModels": minimum_models
        }]
    }))
    .unwrap()
}

fn plan_bytes(campaigns: Vec<Value>) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schemaVersion": 1,
        "siteId": "WO-2026-0042",
        "physicalDeviceConfirmed": true,
        "campaigns": campaigns
    }))
    .unwrap()
}

fn campaign(
    campaign_id: &str,
    package: &std::path::Path,
    output: &std::path::Path,
    audit: &std::path::Path,
    manufacturer: &str,
    model: &str,
    serial: &str,
) -> Value {
    json!({
        "campaignId": campaign_id,
        "operator": "operator-a",
        "configPath": package,
        "outputDir": output,
        "nativeBrokerAuditPath": audit,
        "physicalDevice": {
            "connectionId": "modbus-main",
            "manufacturer": manufacturer,
            "model": model,
            "serialNumber": serial
        },
        "durationSeconds": 86400,
        "schedulerIntervalMs": 100,
        "maximumFailureRatio": 0.01,
        "maximumProgressGapSeconds": 300,
        "changingPoints": ["pump-1/pressure"]
    })
}
