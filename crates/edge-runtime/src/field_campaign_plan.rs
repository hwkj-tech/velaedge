use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use edge_core::{EdgeConfigPackage, ProtocolType};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    configured_data_mqtt_output_routes, field_protocol_name, validate_field_endurance_options,
    validate_field_interoperability_policy, validate_mqtt_uplink_config,
    validate_mqtt_uplink_runtime_environment, ConfiguredEdgeRuntime, DataConfigSchedule,
    FieldDeviceIdentity, FieldEnduranceOptions, FieldInteroperabilityPolicy, TokioSerialBusFactory,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldCampaignDeploymentPlan {
    pub schema_version: u32,
    pub site_id: String,
    pub physical_device_confirmed: bool,
    pub campaigns: Vec<FieldCampaignPlanEntry>,
}

impl FieldCampaignDeploymentPlan {
    pub fn from_json_slice(bytes: &[u8]) -> Result<Self> {
        let plan = serde_json::from_slice::<Self>(bytes).context("decode field campaign plan")?;
        if plan.schema_version != 1 {
            bail!(
                "field campaign plan uses unsupported schema {}; version 1 is required",
                plan.schema_version
            );
        }
        Ok(plan)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldCampaignPlanEntry {
    pub campaign_id: String,
    pub operator: String,
    pub config_path: PathBuf,
    pub output_dir: PathBuf,
    pub native_broker_audit_path: PathBuf,
    #[serde(default)]
    pub rocksdb_path: Option<PathBuf>,
    pub physical_device: FieldCampaignPlanDevice,
    #[serde(default = "default_duration_seconds")]
    pub duration_seconds: u64,
    #[serde(default = "default_scheduler_interval_ms")]
    pub scheduler_interval_ms: u64,
    #[serde(default)]
    pub minimum_cycles: Option<u64>,
    #[serde(default = "default_maximum_failure_ratio")]
    pub maximum_failure_ratio: f64,
    #[serde(default = "default_maximum_progress_gap_seconds")]
    pub maximum_progress_gap_seconds: u64,
    #[serde(default = "default_receipt_startup_timeout_seconds")]
    pub receipt_startup_timeout_seconds: u64,
    #[serde(default = "default_receipt_post_run_grace_seconds")]
    pub receipt_post_run_grace_seconds: u64,
    #[serde(default = "default_native_broker_audit_wait_seconds")]
    pub native_broker_audit_wait_seconds: u64,
    #[serde(default)]
    pub require_recovery: bool,
    #[serde(default)]
    pub changing_points: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldCampaignPlanDevice {
    pub connection_id: String,
    pub manufacturer: String,
    pub model: String,
    pub serial_number: String,
}

const fn default_duration_seconds() -> u64 {
    86_400
}

const fn default_scheduler_interval_ms() -> u64 {
    100
}

fn default_maximum_failure_ratio() -> f64 {
    0.01
}

const fn default_maximum_progress_gap_seconds() -> u64 {
    300
}

const fn default_receipt_startup_timeout_seconds() -> u64 {
    30
}

const fn default_receipt_post_run_grace_seconds() -> u64 {
    60
}

const fn default_native_broker_audit_wait_seconds() -> u64 {
    300
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldCampaignPlanStatus {
    Passed,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldCampaignPlanReport {
    pub schema_version: u32,
    pub status: FieldCampaignPlanStatus,
    pub mode: &'static str,
    pub plan_sha256: String,
    pub policy_sha256: String,
    pub site_id: String,
    pub physical_device_confirmed: bool,
    pub summary: FieldCampaignPlanSummary,
    pub coverage: Vec<FieldCampaignProtocolCoverage>,
    pub campaigns: Vec<FieldCampaignPlanEntryReport>,
    pub errors: Vec<String>,
}

impl FieldCampaignPlanReport {
    pub fn passed(&self) -> bool {
        self.status == FieldCampaignPlanStatus::Passed
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldCampaignPlanSummary {
    pub campaign_count: usize,
    pub ready_campaign_count: usize,
    pub invalid_campaign_count: usize,
    pub required_protocol_count: usize,
    pub covered_protocol_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldCampaignProtocolCoverage {
    pub protocol: String,
    pub minimum_manufacturers: usize,
    pub observed_manufacturers: usize,
    pub minimum_models: usize,
    pub observed_models: usize,
    pub campaign_ids: Vec<String>,
    pub passed: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldCampaignPlanEntryReport {
    pub campaign_id: String,
    pub service_instance: String,
    pub operator: String,
    pub config_path: String,
    pub output_dir: String,
    pub native_broker_audit_path: String,
    pub rocksdb_path: String,
    pub physical_device: FieldCampaignPlanDevice,
    pub package_sha256: Option<String>,
    pub edge_id: Option<String>,
    pub config_version: Option<String>,
    pub protocol: Option<String>,
    pub used_connection_ids: Vec<String>,
    pub mqtt_client_ids: Vec<String>,
    pub mqtt_routes: Vec<FieldCampaignPlanMqttRoute>,
    pub required_secret_environment: Vec<String>,
    pub systemd_environment: BTreeMap<String, String>,
    pub ready: bool,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldCampaignPlanMqttRoute {
    pub sink_id: String,
    pub broker: String,
    pub topic: String,
    pub qos: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FieldCampaignPlanEvaluationMode {
    DeploymentPreflight,
    SiteObservation,
}

/// Validates a complete physical-site campaign inventory without opening any
/// southbound or MQTT sessions. Only campaigns that pass every local and
/// cross-campaign invariant count toward the policy coverage matrix.
pub fn evaluate_field_campaign_plan(
    plan_bytes: &[u8],
    policy: &FieldInteroperabilityPolicy,
    policy_sha256: impl Into<String>,
) -> Result<FieldCampaignPlanReport> {
    evaluate_field_campaign_plan_with_mode(
        plan_bytes,
        policy,
        policy_sha256,
        FieldCampaignPlanEvaluationMode::DeploymentPreflight,
    )
}

/// Revalidates the immutable plan and package contracts after campaigns may
/// already have created evidence and native-audit files. It intentionally does
/// not read Runtime credential values; completed evidence is verified
/// separately by the site-status gate.
pub fn evaluate_field_campaign_plan_for_site_status(
    plan_bytes: &[u8],
    policy: &FieldInteroperabilityPolicy,
    policy_sha256: impl Into<String>,
) -> Result<FieldCampaignPlanReport> {
    evaluate_field_campaign_plan_with_mode(
        plan_bytes,
        policy,
        policy_sha256,
        FieldCampaignPlanEvaluationMode::SiteObservation,
    )
}

fn evaluate_field_campaign_plan_with_mode(
    plan_bytes: &[u8],
    policy: &FieldInteroperabilityPolicy,
    policy_sha256: impl Into<String>,
    mode: FieldCampaignPlanEvaluationMode,
) -> Result<FieldCampaignPlanReport> {
    validate_field_interoperability_policy(policy)?;
    let plan = FieldCampaignDeploymentPlan::from_json_slice(plan_bytes)?;
    let plan_sha256 = format!("{:x}", Sha256::digest(plan_bytes));
    let mut errors = Vec::new();
    if plan.site_id.trim().is_empty() {
        errors.push("field campaign plan siteId is required".to_string());
    }
    if !plan.physical_device_confirmed {
        errors.push("field campaign plan requires physicalDeviceConfirmed=true".to_string());
    }
    if plan.campaigns.is_empty() {
        errors.push("field campaign plan requires at least one campaign".to_string());
    }

    let mut campaigns = plan
        .campaigns
        .iter()
        .map(|entry| evaluate_campaign_entry(&plan.site_id, entry, policy, mode))
        .collect::<Vec<_>>();
    validate_cross_campaign_invariants(&mut campaigns);

    for campaign in &mut campaigns {
        campaign.ready = campaign.reasons.is_empty();
    }

    let mut coverage = Vec::with_capacity(policy.required_protocols.len());
    for protocol in &policy.required_protocols {
        let minimum_manufacturers = policy
            .minimum_manufacturers_by_protocol
            .get(protocol)
            .copied()
            .unwrap_or(policy.minimum_manufacturers_per_protocol);
        let minimum_models = policy
            .minimum_models_by_protocol
            .get(protocol)
            .copied()
            .unwrap_or(policy.minimum_models_per_protocol);
        let accepted = campaigns
            .iter()
            .filter(|campaign| campaign.ready && campaign.protocol.as_deref() == Some(protocol))
            .collect::<Vec<_>>();
        let manufacturers = accepted
            .iter()
            .map(|campaign| normalize(&campaign.physical_device.manufacturer))
            .collect::<BTreeSet<_>>();
        let models = accepted
            .iter()
            .map(|campaign| {
                format!(
                    "{}|{}",
                    normalize(&campaign.physical_device.manufacturer),
                    normalize(&campaign.physical_device.model)
                )
            })
            .collect::<BTreeSet<_>>();
        let passed = manufacturers.len() >= minimum_manufacturers && models.len() >= minimum_models;
        if !passed {
            errors.push(format!(
                "protocol {protocol} plans {} manufacturer(s)/{} model(s), but policy requires {minimum_manufacturers}/{}",
                manufacturers.len(),
                models.len(),
                minimum_models
            ));
        }
        coverage.push(FieldCampaignProtocolCoverage {
            protocol: protocol.clone(),
            minimum_manufacturers,
            observed_manufacturers: manufacturers.len(),
            minimum_models,
            observed_models: models.len(),
            campaign_ids: accepted
                .iter()
                .map(|campaign| campaign.campaign_id.clone())
                .collect(),
            passed,
        });
    }

    let ready_campaign_count = campaigns.iter().filter(|campaign| campaign.ready).count();
    let covered_protocol_count = coverage.iter().filter(|item| item.passed).count();
    if ready_campaign_count != campaigns.len() {
        errors.push(format!(
            "{} campaign(s) failed deployment preflight",
            campaigns.len() - ready_campaign_count
        ));
    }
    let status = if errors.is_empty() {
        FieldCampaignPlanStatus::Passed
    } else {
        FieldCampaignPlanStatus::Failed
    };

    Ok(FieldCampaignPlanReport {
        schema_version: 1,
        status,
        mode: match mode {
            FieldCampaignPlanEvaluationMode::DeploymentPreflight => "physical_field_campaign_plan",
            FieldCampaignPlanEvaluationMode::SiteObservation => {
                "physical_field_campaign_site_observation"
            }
        },
        plan_sha256,
        policy_sha256: policy_sha256.into(),
        site_id: plan.site_id,
        physical_device_confirmed: plan.physical_device_confirmed,
        summary: FieldCampaignPlanSummary {
            campaign_count: campaigns.len(),
            ready_campaign_count,
            invalid_campaign_count: campaigns.len() - ready_campaign_count,
            required_protocol_count: coverage.len(),
            covered_protocol_count,
        },
        coverage,
        campaigns,
        errors,
    })
}

fn evaluate_campaign_entry(
    site_id: &str,
    entry: &FieldCampaignPlanEntry,
    policy: &FieldInteroperabilityPolicy,
    mode: FieldCampaignPlanEvaluationMode,
) -> FieldCampaignPlanEntryReport {
    let rocksdb_path = entry
        .rocksdb_path
        .clone()
        .unwrap_or_else(|| entry.output_dir.join("runtime.rocksdb"));
    let mut report = FieldCampaignPlanEntryReport {
        campaign_id: entry.campaign_id.trim().to_string(),
        service_instance: format!(
            "edgeops-field-campaign@{}.service",
            entry.campaign_id.trim()
        ),
        operator: entry.operator.trim().to_string(),
        config_path: entry.config_path.display().to_string(),
        output_dir: entry.output_dir.display().to_string(),
        native_broker_audit_path: entry.native_broker_audit_path.display().to_string(),
        rocksdb_path: rocksdb_path.display().to_string(),
        physical_device: FieldCampaignPlanDevice {
            connection_id: entry.physical_device.connection_id.trim().to_string(),
            manufacturer: entry.physical_device.manufacturer.trim().to_string(),
            model: entry.physical_device.model.trim().to_string(),
            serial_number: entry.physical_device.serial_number.trim().to_string(),
        },
        package_sha256: None,
        edge_id: None,
        config_version: None,
        protocol: None,
        used_connection_ids: Vec::new(),
        mqtt_client_ids: Vec::new(),
        mqtt_routes: Vec::new(),
        required_secret_environment: Vec::new(),
        systemd_environment: systemd_environment(site_id, entry, &rocksdb_path),
        ready: false,
        reasons: Vec::new(),
    };

    if !valid_campaign_id(&report.campaign_id) {
        report.reasons.push(
            "campaignId must contain only ASCII letters, digits, '.', '_' or '-'".to_string(),
        );
    }
    if report.operator.is_empty() {
        report.reasons.push("operator is required".to_string());
    }
    for (label, value) in [
        (
            "connectionId",
            report.physical_device.connection_id.as_str(),
        ),
        ("manufacturer", report.physical_device.manufacturer.as_str()),
        ("model", report.physical_device.model.as_str()),
        (
            "serialNumber",
            report.physical_device.serial_number.as_str(),
        ),
    ] {
        if value.is_empty() {
            report
                .reasons
                .push(format!("physicalDevice.{label} is required"));
        }
    }
    for (label, path) in [
        ("configPath", &entry.config_path),
        ("outputDir", &entry.output_dir),
        ("nativeBrokerAuditPath", &entry.native_broker_audit_path),
        ("rocksdbPath", &rocksdb_path),
    ] {
        if !normalized_absolute_path(path) {
            report
                .reasons
                .push(format!("{label} must be a normalized absolute path"));
        }
    }
    if entry
        .native_broker_audit_path
        .starts_with(&entry.output_dir)
    {
        report
            .reasons
            .push("native Broker audit source must be outside outputDir".to_string());
    }
    if mode == FieldCampaignPlanEvaluationMode::DeploymentPreflight {
        validate_artifact_paths(entry, &mut report.reasons);
    }

    if entry.duration_seconds.saturating_mul(1_000) < policy.minimum_duration_ms {
        report.reasons.push(format!(
            "durationSeconds {} is below policy minimum {}",
            entry.duration_seconds,
            policy.minimum_duration_ms / 1_000
        ));
    }
    if entry.maximum_failure_ratio > policy.maximum_failure_ratio {
        report.reasons.push(format!(
            "maximumFailureRatio {} exceeds policy maximum {}",
            entry.maximum_failure_ratio, policy.maximum_failure_ratio
        ));
    }
    if entry.maximum_progress_gap_seconds.saturating_mul(1_000) > policy.maximum_progress_gap_ms {
        report.reasons.push(format!(
            "maximumProgressGapSeconds {} exceeds policy maximum {}",
            entry.maximum_progress_gap_seconds,
            policy.maximum_progress_gap_ms / 1_000
        ));
    }

    let package_bytes = match fs::read(&entry.config_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            report.reasons.push(format!(
                "cannot read configuration package {}: {error}",
                entry.config_path.display()
            ));
            return report;
        }
    };
    report.package_sha256 = Some(format!("{:x}", Sha256::digest(&package_bytes)));
    let package = match serde_json::from_slice::<EdgeConfigPackage>(&package_bytes) {
        Ok(package) => package,
        Err(error) => {
            report
                .reasons
                .push(format!("cannot decode configuration package: {error}"));
            return report;
        }
    };
    report.edge_id = Some(package.edge_id.clone());
    report.config_version = Some(package.version.clone());

    if let Err(error) = ConfiguredEdgeRuntime::new(package.clone(), TokioSerialBusFactory) {
        report
            .reasons
            .push(format!("Runtime configuration is invalid: {error:#}"));
    }
    if let Err(error) = DataConfigSchedule::from_package(&package) {
        report
            .reasons
            .push(format!("collection schedule is invalid: {error:#}"));
    }

    let used_connection_ids = package
        .data_configs
        .iter()
        .filter(|config| config.enabled)
        .map(|config| config.protocol_connection_id.trim().to_string())
        .collect::<BTreeSet<_>>();
    report.used_connection_ids = used_connection_ids.iter().cloned().collect();
    match package
        .protocol_connections
        .iter()
        .find(|connection| connection.connection_id.trim() == report.physical_device.connection_id)
    {
        Some(connection) => {
            let protocol = field_protocol_name(connection.protocol).to_string();
            report.protocol = Some(protocol.clone());
            if connection.protocol == ProtocolType::Simulated {
                report
                    .reasons
                    .push("physical campaign cannot use Simulated protocol".to_string());
            }
            if !used_connection_ids.contains(&report.physical_device.connection_id) {
                report.reasons.push(format!(
                    "physical connection {} is not used by an enabled data config",
                    report.physical_device.connection_id
                ));
            }
            if !policy.required_protocols.contains(&protocol) {
                report.reasons.push(format!(
                    "physical connection uses protocol {protocol}, which is not required by the deployment policy"
                ));
            }
        }
        None => report.reasons.push(format!(
            "physical connection {} does not exist in the configuration package",
            report.physical_device.connection_id
        )),
    }

    match configured_data_mqtt_output_routes(&package) {
        Ok(routes) => {
            if routes.is_empty() {
                report
                    .reasons
                    .push("package requires at least one enabled MQTT output route".to_string());
            }
            report.mqtt_routes = routes
                .into_iter()
                .map(|route| FieldCampaignPlanMqttRoute {
                    sink_id: route.sink_id,
                    broker: route.broker,
                    topic: route.topic,
                    qos: route.qos,
                })
                .collect();
            for route in &report.mqtt_routes {
                if route.qos != 1 {
                    report.reasons.push(format!(
                        "MQTT route {} / {} must use QoS 1 for field acceptance",
                        route.sink_id, route.topic
                    ));
                }
            }
        }
        Err(error) => report
            .reasons
            .push(format!("MQTT output routes are invalid: {error:#}")),
    }

    let used_sink_ids = report
        .mqtt_routes
        .iter()
        .map(|route| route.sink_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut secret_environment = BTreeSet::new();
    let mut client_ids = BTreeSet::new();
    for uplink in package
        .mqtt_uplinks
        .iter()
        .filter(|uplink| used_sink_ids.contains(uplink.sink_id.as_str()))
    {
        client_ids.insert(uplink.client_id.clone());
        if let Some(name) = uplink.password_env.as_deref() {
            secret_environment.insert(name.to_string());
        }
        let validation = match mode {
            FieldCampaignPlanEvaluationMode::DeploymentPreflight => {
                validate_mqtt_uplink_runtime_environment(uplink)
            }
            FieldCampaignPlanEvaluationMode::SiteObservation => validate_mqtt_uplink_config(uplink),
        };
        if let Err(error) = validation {
            report.reasons.push(format!(
                "MQTT sink {} configuration is invalid: {error:#}",
                uplink.sink_id
            ));
        }
    }
    report.mqtt_client_ids = client_ids.into_iter().collect();
    report.required_secret_environment = secret_environment.into_iter().collect();

    let duration = Duration::from_secs(entry.duration_seconds);
    let minimum_period_ms = package
        .data_configs
        .iter()
        .filter(|config| config.enabled)
        .map(|config| config.collection.period_ms)
        .min()
        .unwrap_or(1_000);
    let derived_minimum_cycles = duration
        .as_millis()
        .div_ceil(u128::from(minimum_period_ms.max(1)))
        .saturating_mul(9)
        / 10;
    let options = FieldEnduranceOptions {
        package,
        package_sha256: report.package_sha256.clone(),
        duration,
        scheduler_interval: Duration::from_millis(entry.scheduler_interval_ms),
        minimum_cycles: entry
            .minimum_cycles
            .unwrap_or_else(|| u64::try_from(derived_minimum_cycles.max(1)).unwrap_or(u64::MAX)),
        maximum_failure_ratio: entry.maximum_failure_ratio,
        maximum_progress_gap: Duration::from_secs(entry.maximum_progress_gap_seconds),
        require_recovery: entry.require_recovery,
        changing_points: entry.changing_points.iter().cloned().collect(),
        exercise_mqtt: true,
        physical_device_exercised: true,
        physical_device: Some(FieldDeviceIdentity {
            site_id: site_id.trim().to_string(),
            operator: report.operator.clone(),
            connection_id: report.physical_device.connection_id.clone(),
            manufacturer: report.physical_device.manufacturer.clone(),
            model: report.physical_device.model.clone(),
            serial_number: report.physical_device.serial_number.clone(),
        }),
        rocksdb_path,
    };
    if let Err(error) = validate_field_endurance_options(&options) {
        report
            .reasons
            .push(format!("field endurance options are invalid: {error:#}"));
    }
    report
}

fn validate_artifact_paths(entry: &FieldCampaignPlanEntry, reasons: &mut Vec<String>) {
    if entry.output_dir.exists() {
        if !entry.output_dir.is_dir() {
            reasons.push("outputDir exists and is not a directory".to_string());
        } else {
            match fs::read_dir(&entry.output_dir) {
                Ok(mut entries) => {
                    if entries.next().is_some() {
                        reasons.push("outputDir must be absent or empty".to_string());
                    }
                }
                Err(error) => reasons.push(format!("cannot inspect outputDir: {error}")),
            }
        }
    }
    if entry.native_broker_audit_path.exists() {
        reasons.push("nativeBrokerAuditPath must not exist before the campaign".to_string());
    }
}

fn validate_cross_campaign_invariants(campaigns: &mut [FieldCampaignPlanEntryReport]) {
    apply_duplicate_reason(campaigns, "campaignId", |campaign| {
        normalize(&campaign.campaign_id)
    });
    apply_duplicate_reason(
        campaigns,
        "physical manufacturer/model/serial",
        |campaign| {
            format!(
                "{}|{}|{}",
                normalize(&campaign.physical_device.manufacturer),
                normalize(&campaign.physical_device.model),
                normalize(&campaign.physical_device.serial_number)
            )
        },
    );
    apply_optional_duplicate_reason(campaigns, "package edgeId", |campaign| {
        campaign.edge_id.as_ref().map(|value| normalize(value))
    });
    apply_duplicate_reason(campaigns, "outputDir", |campaign| {
        campaign.output_dir.clone()
    });
    apply_duplicate_reason(campaigns, "nativeBrokerAuditPath", |campaign| {
        campaign.native_broker_audit_path.clone()
    });
    apply_duplicate_reason(campaigns, "rocksdbPath", |campaign| {
        campaign.rocksdb_path.clone()
    });

    let mut clients = BTreeMap::<String, Vec<usize>>::new();
    for (index, campaign) in campaigns.iter().enumerate() {
        for client_id in &campaign.mqtt_client_ids {
            clients.entry(normalize(client_id)).or_default().push(index);
        }
    }
    append_duplicate_groups(campaigns, "MQTT clientId", clients);

    for left in 0..campaigns.len() {
        for right in (left + 1)..campaigns.len() {
            let left_output = Path::new(&campaigns[left].output_dir);
            let right_output = Path::new(&campaigns[right].output_dir);
            if left_output.starts_with(right_output) || right_output.starts_with(left_output) {
                campaigns[left]
                    .reasons
                    .push("outputDir overlaps another campaign outputDir".to_string());
                campaigns[right]
                    .reasons
                    .push("outputDir overlaps another campaign outputDir".to_string());
            }
        }
    }
    let output_dirs = campaigns
        .iter()
        .map(|campaign| PathBuf::from(&campaign.output_dir))
        .collect::<Vec<_>>();
    for campaign in campaigns.iter_mut() {
        let audit = Path::new(&campaign.native_broker_audit_path);
        if output_dirs.iter().any(|output| audit.starts_with(output)) {
            campaign
                .reasons
                .push("nativeBrokerAuditPath must be outside every campaign outputDir".to_string());
        }
    }
}

fn apply_duplicate_reason(
    campaigns: &mut [FieldCampaignPlanEntryReport],
    label: &str,
    key: impl Fn(&FieldCampaignPlanEntryReport) -> String,
) {
    let mut groups = BTreeMap::<String, Vec<usize>>::new();
    for (index, campaign) in campaigns.iter().enumerate() {
        groups.entry(key(campaign)).or_default().push(index);
    }
    append_duplicate_groups(campaigns, label, groups);
}

fn apply_optional_duplicate_reason(
    campaigns: &mut [FieldCampaignPlanEntryReport],
    label: &str,
    key: impl Fn(&FieldCampaignPlanEntryReport) -> Option<String>,
) {
    let mut groups = BTreeMap::<String, Vec<usize>>::new();
    for (index, campaign) in campaigns.iter().enumerate() {
        if let Some(key) = key(campaign) {
            groups.entry(key).or_default().push(index);
        }
    }
    append_duplicate_groups(campaigns, label, groups);
}

fn append_duplicate_groups(
    campaigns: &mut [FieldCampaignPlanEntryReport],
    label: &str,
    groups: BTreeMap<String, Vec<usize>>,
) {
    for (value, indexes) in groups.into_iter().filter(|(_, indexes)| indexes.len() > 1) {
        for index in indexes {
            campaigns[index]
                .reasons
                .push(format!("duplicate {label}: {value}"));
        }
    }
}

fn systemd_environment(
    site_id: &str,
    entry: &FieldCampaignPlanEntry,
    rocksdb_path: &Path,
) -> BTreeMap<String, String> {
    let mut environment = BTreeMap::from([
        (
            "EDGEOPS_FIELD_CAMPAIGN_CONFIG".to_string(),
            entry.config_path.display().to_string(),
        ),
        (
            "EDGEOPS_FIELD_CAMPAIGN_OUTPUT_DIR".to_string(),
            entry.output_dir.display().to_string(),
        ),
        (
            "EDGEOPS_FIELD_CAMPAIGN_NATIVE_BROKER_AUDIT".to_string(),
            entry.native_broker_audit_path.display().to_string(),
        ),
        (
            "EDGEOPS_FIELD_CAMPAIGN_SITE_ID".to_string(),
            site_id.trim().to_string(),
        ),
        (
            "EDGEOPS_FIELD_CAMPAIGN_OPERATOR".to_string(),
            entry.operator.trim().to_string(),
        ),
        (
            "EDGEOPS_FIELD_CAMPAIGN_DEVICE_CONNECTION_ID".to_string(),
            entry.physical_device.connection_id.trim().to_string(),
        ),
        (
            "EDGEOPS_FIELD_CAMPAIGN_DEVICE_MANUFACTURER".to_string(),
            entry.physical_device.manufacturer.trim().to_string(),
        ),
        (
            "EDGEOPS_FIELD_CAMPAIGN_DEVICE_MODEL".to_string(),
            entry.physical_device.model.trim().to_string(),
        ),
        (
            "EDGEOPS_FIELD_CAMPAIGN_DEVICE_SERIAL".to_string(),
            entry.physical_device.serial_number.trim().to_string(),
        ),
        (
            "EDGEOPS_FIELD_CAMPAIGN_PHYSICAL_DEVICE_CONFIRMED".to_string(),
            "1".to_string(),
        ),
        (
            "EDGEOPS_FIELD_CAMPAIGN_DURATION_SECONDS".to_string(),
            entry.duration_seconds.to_string(),
        ),
        (
            "EDGEOPS_FIELD_CAMPAIGN_SCHEDULER_INTERVAL_MS".to_string(),
            entry.scheduler_interval_ms.to_string(),
        ),
        (
            "EDGEOPS_FIELD_CAMPAIGN_MAXIMUM_FAILURE_RATIO".to_string(),
            entry.maximum_failure_ratio.to_string(),
        ),
        (
            "EDGEOPS_FIELD_CAMPAIGN_MAXIMUM_PROGRESS_GAP_SECONDS".to_string(),
            entry.maximum_progress_gap_seconds.to_string(),
        ),
        (
            "EDGEOPS_FIELD_CAMPAIGN_RECEIPT_STARTUP_TIMEOUT_SECONDS".to_string(),
            entry.receipt_startup_timeout_seconds.to_string(),
        ),
        (
            "EDGEOPS_FIELD_CAMPAIGN_RECEIPT_POST_RUN_GRACE_SECONDS".to_string(),
            entry.receipt_post_run_grace_seconds.to_string(),
        ),
        (
            "EDGEOPS_FIELD_CAMPAIGN_NATIVE_BROKER_AUDIT_WAIT_SECONDS".to_string(),
            entry.native_broker_audit_wait_seconds.to_string(),
        ),
        (
            "EDGEOPS_FIELD_CAMPAIGN_REQUIRE_RECOVERY".to_string(),
            if entry.require_recovery { "1" } else { "0" }.to_string(),
        ),
        (
            "EDGEOPS_FIELD_CAMPAIGN_ROCKSDB_PATH".to_string(),
            rocksdb_path.display().to_string(),
        ),
    ]);
    if let Some(minimum_cycles) = entry.minimum_cycles {
        environment.insert(
            "EDGEOPS_FIELD_CAMPAIGN_MINIMUM_CYCLES".to_string(),
            minimum_cycles.to_string(),
        );
    }
    if !entry.changing_points.is_empty() {
        environment.insert(
            "EDGEOPS_FIELD_CAMPAIGN_CHANGING_POINTS".to_string(),
            entry.changing_points.join(","),
        );
    }
    environment
}

fn normalized_absolute_path(path: &Path) -> bool {
    path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::RootDir | Component::Normal(_)))
}

fn valid_campaign_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}
