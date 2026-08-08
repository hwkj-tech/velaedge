use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use edge_core::{EdgeConfigPackage, ProtocolType};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::protocol_catalog::RuntimeProtocolCatalog;

#[derive(Clone, Debug)]
pub struct FieldInteroperabilityEvidence {
    source: String,
    sha256: String,
    report: FieldEnduranceEvidenceReport,
    package: Option<BoundPackageEvidence>,
    broker_receipt_sha256: Option<String>,
    broker_receipt: Option<BrokerConsumerReceipt>,
    native_broker_audit_sha256: Option<String>,
    native_broker_audit: Option<NativeBrokerAudit>,
}

impl FieldInteroperabilityEvidence {
    pub fn from_slice(source: impl Into<String>, bytes: &[u8]) -> Result<Self> {
        let source = source.into();
        let report = serde_json::from_slice(bytes)
            .with_context(|| format!("decode field endurance evidence {source}"))?;
        Ok(Self {
            source,
            sha256: format!("{:x}", Sha256::digest(bytes)),
            report,
            package: None,
            broker_receipt_sha256: None,
            broker_receipt: None,
            native_broker_audit_sha256: None,
            native_broker_audit: None,
        })
    }

    pub fn from_artifacts(
        source: impl Into<String>,
        report_bytes: &[u8],
        package_bytes: &[u8],
        broker_receipt_bytes: &[u8],
        native_broker_audit_bytes: &[u8],
    ) -> Result<Self> {
        if native_broker_audit_bytes.is_empty() {
            bail!("native broker audit artifact must not be empty");
        }
        let mut evidence = Self::from_slice(source, report_bytes)?;
        let package = serde_json::from_slice::<EdgeConfigPackage>(package_bytes)
            .context("decode bound configuration package")?;
        let broker_receipt = serde_json::from_slice::<BrokerConsumerReceipt>(broker_receipt_bytes)
            .context("decode broker consumer receipt")?;
        let native_broker_audit = NativeBrokerAudit::from_json_slice(native_broker_audit_bytes)?;
        let protocol_connections = package
            .protocol_connections
            .iter()
            .map(|connection| {
                (
                    connection.connection_id.clone(),
                    field_protocol_name(connection.protocol).to_string(),
                )
            })
            .collect();
        let used_connection_ids = package
            .data_configs
            .iter()
            .filter(|config| config.enabled)
            .map(|config| config.protocol_connection_id.trim().to_string())
            .collect();
        let used_sink_ids = package
            .data_configs
            .iter()
            .filter(|config| config.enabled)
            .map(|config| config.publish.sink_id.trim().to_string())
            .collect();
        evidence.package = Some(BoundPackageEvidence {
            sha256: format!("{:x}", Sha256::digest(package_bytes)),
            edge_id: package.edge_id,
            config_version: package.version,
            protocol_connections,
            used_connection_ids,
            used_sink_ids,
        });
        evidence.broker_receipt_sha256 =
            Some(format!("{:x}", Sha256::digest(broker_receipt_bytes)));
        evidence.broker_receipt = Some(broker_receipt);
        evidence.native_broker_audit_sha256 =
            Some(format!("{:x}", Sha256::digest(native_broker_audit_bytes)));
        evidence.native_broker_audit = Some(native_broker_audit);
        Ok(evidence)
    }
}

#[derive(Clone, Debug)]
pub struct FieldInteroperabilityPolicy {
    pub required_protocols: BTreeSet<String>,
    pub minimum_manufacturers_per_protocol: usize,
    pub minimum_models_per_protocol: usize,
    pub minimum_manufacturers_by_protocol: BTreeMap<String, usize>,
    pub minimum_models_by_protocol: BTreeMap<String, usize>,
    pub minimum_duration_ms: u64,
    pub maximum_failure_ratio: f64,
    pub maximum_progress_gap_ms: u64,
}

impl Default for FieldInteroperabilityPolicy {
    fn default() -> Self {
        Self {
            required_protocols: [
                "DL/T 645-2007".to_string(),
                "IEC-101".to_string(),
                "IEC-104".to_string(),
                "OPC UA".to_string(),
            ]
            .into_iter()
            .collect(),
            minimum_manufacturers_per_protocol: 2,
            minimum_models_per_protocol: 1,
            minimum_manufacturers_by_protocol: BTreeMap::new(),
            minimum_models_by_protocol: BTreeMap::new(),
            minimum_duration_ms: 86_400_000,
            maximum_failure_ratio: 0.01,
            maximum_progress_gap_ms: 300_000,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FieldInteroperabilityPolicyFile {
    schema_version: u32,
    minimum_duration_seconds: u64,
    maximum_failure_ratio: f64,
    #[serde(default = "default_maximum_progress_gap_seconds")]
    maximum_progress_gap_seconds: u64,
    protocols: Vec<FieldProtocolRequirementFile>,
}

const fn default_maximum_progress_gap_seconds() -> u64 {
    300
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FieldProtocolRequirementFile {
    protocol: String,
    minimum_manufacturers: usize,
    minimum_models: usize,
}

impl FieldInteroperabilityPolicy {
    pub fn from_json_slice(bytes: &[u8]) -> Result<Self> {
        let file = serde_json::from_slice::<FieldInteroperabilityPolicyFile>(bytes)
            .context("decode field interoperability policy")?;
        if file.schema_version != 1 {
            bail!(
                "field interoperability policy uses unsupported schema {}; version 1 is required",
                file.schema_version
            );
        }
        if file.protocols.is_empty() {
            bail!("field interoperability policy requires at least one protocol");
        }

        let mut required_protocols = BTreeSet::new();
        let mut minimum_manufacturers_by_protocol = BTreeMap::new();
        let mut minimum_models_by_protocol = BTreeMap::new();
        let executable_protocols = RuntimeProtocolCatalog::executable()
            .into_iter()
            .filter(|descriptor| descriptor.protocol_type != ProtocolType::Simulated)
            .map(|descriptor| field_protocol_name(descriptor.protocol_type))
            .collect::<BTreeSet<_>>();
        for requirement in file.protocols {
            let protocol = canonical_protocol_name(&requirement.protocol);
            if protocol.is_empty() {
                bail!("field interoperability policy contains an empty protocol");
            }
            if !executable_protocols.contains(protocol.as_str()) {
                bail!(
                    "field interoperability policy references unsupported or non-physical protocol {protocol}"
                );
            }
            if !required_protocols.insert(protocol.clone()) {
                bail!("field interoperability policy contains duplicate protocol alias {protocol}");
            }
            minimum_manufacturers_by_protocol
                .insert(protocol.clone(), requirement.minimum_manufacturers);
            minimum_models_by_protocol.insert(protocol, requirement.minimum_models);
        }

        let policy = Self {
            required_protocols,
            minimum_manufacturers_per_protocol: 1,
            minimum_models_per_protocol: 1,
            minimum_manufacturers_by_protocol,
            minimum_models_by_protocol,
            minimum_duration_ms: file
                .minimum_duration_seconds
                .checked_mul(1_000)
                .context("field interoperability policy duration overflows milliseconds")?,
            maximum_failure_ratio: file.maximum_failure_ratio,
            maximum_progress_gap_ms: file
                .maximum_progress_gap_seconds
                .checked_mul(1_000)
                .context("field interoperability maximum progress gap overflows milliseconds")?,
        };
        validate_policy(&policy)?;
        Ok(policy)
    }
}

pub fn validate_field_interoperability_policy(policy: &FieldInteroperabilityPolicy) -> Result<()> {
    validate_policy(policy).map(|_| ())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldInteroperabilityStatus {
    Passed,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldInteroperabilityReport {
    pub schema_version: u32,
    pub status: FieldInteroperabilityStatus,
    pub mode: &'static str,
    pub policy: FieldInteroperabilityPolicyReport,
    pub summary: FieldInteroperabilitySummary,
    pub protocols: Vec<ProtocolInteroperabilityEvidence>,
    pub rejected_evidence: Vec<RejectedInteroperabilityEvidence>,
}

impl FieldInteroperabilityReport {
    pub fn passed(&self) -> bool {
        self.status == FieldInteroperabilityStatus::Passed
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldInteroperabilityPolicyReport {
    pub required_protocols: Vec<String>,
    pub minimum_manufacturers_per_protocol: usize,
    pub minimum_models_per_protocol: usize,
    pub protocol_requirements: Vec<FieldProtocolCoverageRequirementReport>,
    pub minimum_duration_ms: u64,
    pub maximum_failure_ratio: f64,
    pub maximum_progress_gap_ms: u64,
    pub require_physical_device: bool,
    pub require_mqtt_puback: bool,
    pub require_configuration_package: bool,
    pub require_broker_consumer_receipt: bool,
    pub require_native_broker_audit: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldProtocolCoverageRequirementReport {
    pub protocol: String,
    pub minimum_manufacturers: usize,
    pub minimum_models: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldInteroperabilitySummary {
    pub supplied_evidence_count: usize,
    pub accepted_evidence_count: usize,
    pub rejected_evidence_count: usize,
    pub required_protocol_count: usize,
    pub satisfied_protocol_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolInteroperabilityEvidence {
    pub protocol: String,
    pub required_manufacturer_count: usize,
    pub observed_manufacturer_count: usize,
    pub manufacturers: Vec<String>,
    pub required_model_count: usize,
    pub observed_model_count: usize,
    pub models: Vec<String>,
    pub accepted_runs: Vec<AcceptedInteroperabilityRun>,
    pub satisfied: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptedInteroperabilityRun {
    pub source: String,
    pub report_sha256: String,
    pub package_sha256: String,
    pub edge_id: String,
    pub config_version: String,
    pub site_id: String,
    pub operator: String,
    pub connection_id: String,
    pub manufacturer: String,
    pub model: String,
    pub serial_number: String,
    pub observed_duration_ms: u64,
    pub attempted_cycles: u64,
    pub failure_ratio: f64,
    pub collection_attempt_count: u64,
    pub collection_success_count: u64,
    pub maximum_collection_success_gap_ms: u64,
    pub mqtt_publish_success_count: u64,
    pub maximum_mqtt_publish_gap_ms: u64,
    pub broker_receipt_sha256: String,
    pub native_broker_audit_sha256: String,
    pub native_broker: String,
    pub native_broker_instance_id: String,
    pub native_broker_audit_id: String,
    pub native_broker_exported_at: DateTime<Utc>,
    pub broker_message_count: u64,
    pub broker_routes: Vec<BrokerConsumerRouteReceipt>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectedInteroperabilityEvidence {
    pub source: String,
    pub report_sha256: String,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FieldEnduranceEvidenceReport {
    schema_version: u32,
    status: EvidenceRunStatus,
    mode: String,
    physical_device_exercised: bool,
    physical_device: Option<EvidenceDeviceIdentity>,
    edge_id: String,
    config_version: String,
    package_sha256: Option<String>,
    configured_duration_ms: u64,
    observed_duration_ms: u64,
    cycles: EvidenceCycles,
    protocols: Vec<EvidenceProtocol>,
    protocol_acceptance: Vec<EvidenceProtocolAcceptance>,
    mqtt: EvidenceMqtt,
    criteria: EvidenceCriteria,
}

#[derive(Clone, Debug)]
struct BoundPackageEvidence {
    sha256: String,
    edge_id: String,
    config_version: String,
    protocol_connections: BTreeMap<String, String>,
    used_connection_ids: BTreeSet<String>,
    used_sink_ids: BTreeSet<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerConsumerReceipt {
    pub schema_version: u32,
    pub edge_id: String,
    pub config_version: String,
    pub package_sha256: String,
    pub first_received_at: DateTime<Utc>,
    pub last_received_at: DateTime<Utc>,
    pub message_count: u64,
    pub routes: Vec<BrokerConsumerRouteReceipt>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerConsumerRouteReceipt {
    pub broker: String,
    pub consumer_id: String,
    pub message_count: u64,
    pub topics: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeBrokerAudit {
    pub schema_version: u32,
    pub broker: String,
    pub broker_instance_id: String,
    pub audit_id: String,
    pub exported_at: DateTime<Utc>,
    pub edge_id: String,
    pub config_version: String,
    pub package_sha256: String,
    pub first_observed_at: DateTime<Utc>,
    pub last_observed_at: DateTime<Utc>,
    pub message_count: u64,
    pub routes: Vec<BrokerConsumerRouteReceipt>,
}

impl NativeBrokerAudit {
    pub fn from_json_slice(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() {
            bail!("native broker audit artifact must not be empty");
        }
        serde_json::from_slice(bytes).context("decode structured native broker audit")
    }

    pub fn validate_against(&self, receipt: &BrokerConsumerReceipt) -> Result<()> {
        let errors = self.validation_errors_against(receipt);
        if errors.is_empty() {
            Ok(())
        } else {
            bail!("{}", errors.join("; "))
        }
    }

    fn validation_errors_against(&self, receipt: &BrokerConsumerReceipt) -> Vec<String> {
        let mut errors = Vec::new();
        if self.schema_version != 1 {
            errors.push(format!(
                "unsupported native broker audit schema version {}",
                self.schema_version
            ));
        }
        for (name, value) in [
            ("broker", self.broker.as_str()),
            ("broker instance id", self.broker_instance_id.as_str()),
            ("audit id", self.audit_id.as_str()),
        ] {
            if value.trim().is_empty() {
                errors.push(format!("native broker audit {name} is required"));
            }
        }
        if !is_sha256(&self.package_sha256) {
            errors.push("native broker audit package digest is invalid".to_string());
        }
        if self.last_observed_at < self.first_observed_at {
            errors.push("native broker audit timestamps are reversed".to_string());
        }
        if self.exported_at < self.last_observed_at {
            errors.push(
                "native broker audit was exported before its observation window ended".to_string(),
            );
        }
        if self.edge_id != receipt.edge_id || self.config_version != receipt.config_version {
            errors.push(
                "native broker audit edge id or version does not match the broker receipt"
                    .to_string(),
            );
        }
        if self.package_sha256 != receipt.package_sha256 {
            errors.push(
                "native broker audit package digest does not match the broker receipt".to_string(),
            );
        }
        if self.message_count != receipt.message_count {
            errors.push(format!(
                "native broker audit message count {} does not match broker receipt count {}",
                self.message_count, receipt.message_count
            ));
        }
        let audit_route_message_count = self
            .routes
            .iter()
            .try_fold(0_u64, |total, route| total.checked_add(route.message_count));
        if audit_route_message_count != Some(self.message_count) {
            errors.push(format!(
                "native broker audit route message count {} does not match audit total {}",
                audit_route_message_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "overflow".to_string()),
                self.message_count
            ));
        }
        if self.last_observed_at < receipt.first_received_at
            || self.first_observed_at > receipt.last_received_at
        {
            errors.push(
                "native broker audit observation window does not overlap the broker receipt"
                    .to_string(),
            );
        }

        let audit_routes =
            normalized_broker_routes("native broker audit", &self.routes, &mut errors);
        let receipt_routes =
            normalized_broker_routes("broker receipt", &receipt.routes, &mut errors);
        if audit_routes != receipt_routes {
            errors.push(
                "native broker audit routes, topics or counts do not match the broker receipt"
                    .to_string(),
            );
        }
        errors
    }
}

fn normalized_broker_routes(
    source: &str,
    routes: &[BrokerConsumerRouteReceipt],
    errors: &mut Vec<String>,
) -> BTreeMap<(String, String), (u64, BTreeSet<String>)> {
    if routes.is_empty() {
        errors.push(format!("{source} must contain at least one route"));
    }
    let mut normalized = BTreeMap::new();
    let mut total = 0_u64;
    for route in routes {
        let broker = route.broker.trim().to_string();
        let consumer_id = route.consumer_id.trim().to_string();
        if broker.is_empty() || consumer_id.is_empty() {
            errors.push(format!("{source} route requires broker and consumer id"));
        }
        if route.message_count == 0 {
            errors.push(format!(
                "{source} route {broker} / {consumer_id} contains no messages"
            ));
        }
        total = match total.checked_add(route.message_count) {
            Some(total) => total,
            None => {
                errors.push(format!("{source} route message count overflow"));
                total
            }
        };
        let topics = route
            .topics
            .iter()
            .map(|topic| topic.trim().to_string())
            .collect::<BTreeSet<_>>();
        if topics.is_empty() || topics.contains("") || topics.len() != route.topics.len() {
            errors.push(format!(
                "{source} route {broker} / {consumer_id} requires unique non-empty topics"
            ));
        }
        if normalized
            .insert(
                (broker.clone(), consumer_id.clone()),
                (route.message_count, topics),
            )
            .is_some()
        {
            errors.push(format!(
                "{source} contains duplicate route {broker} / {consumer_id}"
            ));
        }
    }
    let declared_total = routes
        .iter()
        .fold(0_u64, |sum, route| sum.saturating_add(route.message_count));
    if total != declared_total {
        errors.push(format!("{source} route message total overflowed"));
    }
    normalized
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum EvidenceRunStatus {
    Passed,
    Failed,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceDeviceIdentity {
    site_id: String,
    operator: String,
    connection_id: String,
    manufacturer: String,
    model: String,
    serial_number: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceCycles {
    attempted: u64,
    failure_ratio: f64,
}

#[derive(Clone, Debug, Deserialize)]
struct EvidenceProtocol {
    connection_id: String,
    protocol: String,
    connected: bool,
    collection_attempt_count: u64,
    collection_success_count: u64,
    circuit_state: EvidenceCircuitState,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
enum EvidenceCircuitState {
    #[default]
    Closed,
    Open,
    HalfOpen,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceProtocolAcceptance {
    connection_id: String,
    protocol: String,
    connected_at_finish: bool,
    circuit_state_at_finish: EvidenceCircuitState,
    collection_attempt_count: u64,
    collection_success_count: u64,
    collection_failure_count: u64,
    failure_ratio: f64,
    activity_observed: bool,
    failure_ratio_within_limit: bool,
    maximum_observed_success_gap_ms: u64,
    maximum_allowed_success_gap_ms: u64,
    counter_reset_observed: bool,
    continuous_activity: bool,
    passed: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceMqtt {
    exercised: bool,
    connected_sink_count: usize,
    publish_success_count: u64,
    publish_failure_count: u64,
    pending_outbox_messages: u64,
    #[serde(default)]
    sinks: Vec<EvidenceMqttSink>,
    sink_acceptance: Vec<EvidenceMqttSinkAcceptance>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceMqttSinkAcceptance {
    sink_id: String,
    publish_success_count: u64,
    maximum_observed_success_gap_ms: u64,
    maximum_allowed_success_gap_ms: u64,
    counter_reset_observed: bool,
    continuous_activity: bool,
    passed: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct EvidenceMqttSink {
    sink_id: String,
    broker: String,
    #[serde(default)]
    connected: bool,
    #[serde(default)]
    publish_success_count: u64,
    last_topic: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceCriteria {
    configured_duration_met: bool,
    minimum_cycles_met: bool,
    failure_ratio_within_limit: bool,
    all_configured_points_observed: bool,
    changing_points_observed: bool,
    protocols_connected_at_finish: bool,
    #[serde(default)]
    protocol_activity_observed: bool,
    #[serde(default)]
    protocols_individually_healthy: bool,
    mqtt_sinks_continuously_publishing: Option<bool>,
    recovery_observed: bool,
    mqtt_puback_complete: Option<bool>,
    mqtt_sinks_connected: Option<bool>,
    outbox_drained: Option<bool>,
    physical_identity_complete: Option<bool>,
    production_protocols_only: Option<bool>,
}

#[derive(Clone, Copy, Debug)]
struct ProtocolCoverageRequirement {
    minimum_manufacturers: usize,
    minimum_models: usize,
}

#[derive(Clone, Debug)]
struct ValidatedFieldInteroperabilityPolicy {
    required_protocols: BTreeSet<String>,
    requirements: BTreeMap<String, ProtocolCoverageRequirement>,
}

pub fn evaluate_field_interoperability(
    policy: &FieldInteroperabilityPolicy,
    evidence: &[FieldInteroperabilityEvidence],
) -> Result<FieldInteroperabilityReport> {
    let validated_policy = validate_policy(policy)?;
    let required_protocols = &validated_policy.required_protocols;
    let mut accepted_by_protocol = required_protocols
        .iter()
        .map(|protocol| (protocol.clone(), Vec::<AcceptedInteroperabilityRun>::new()))
        .collect::<BTreeMap<_, _>>();
    let mut rejected_evidence = Vec::new();
    let mut seen_report_digests = BTreeSet::new();
    let mut seen_devices_by_protocol = BTreeMap::<String, BTreeSet<String>>::new();
    let mut accepted_evidence_digests = BTreeSet::new();

    for item in evidence {
        let mut reasons = validate_report(policy, item);
        if !seen_report_digests.insert(item.sha256.clone()) {
            reasons.push("duplicate report content cannot be counted twice".to_string());
        }

        let identity = item.report.physical_device.as_ref();
        let identity_connection_id = identity
            .map(|identity| identity.connection_id.trim())
            .filter(|connection_id| !connection_id.is_empty());
        let covered_protocols = item
            .report
            .protocols
            .iter()
            .filter(|protocol| Some(protocol.connection_id.trim()) == identity_connection_id)
            .filter(|protocol| protocol.connected && protocol_has_collection_activity(protocol))
            .filter_map(|protocol| {
                let canonical = canonical_protocol_name(&protocol.protocol);
                required_protocols.contains(&canonical).then_some(canonical)
            })
            .collect::<BTreeSet<_>>();
        if covered_protocols.is_empty() {
            let observed = item
                .report
                .protocols
                .iter()
                .map(|protocol| format!("{} ({})", protocol.protocol, protocol.connection_id))
                .collect::<Vec<_>>()
                .join(", ");
            reasons.push(match identity_connection_id {
                Some(connection_id) => format!(
                    "physical device connection {connection_id} does not contain a connected required protocol with successful collection activity; observed: {observed}"
                ),
                None => format!(
                    "report does not contain a connected required protocol; observed: {observed}"
                ),
            });
        } else if covered_protocols.len() > 1 {
            reasons.push(format!(
                "physical device connection cannot prove multiple required protocols: {}",
                covered_protocols
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        let device_key = identity.map(|identity| {
            format!(
                "{}|{}|{}",
                identity.manufacturer.trim().to_lowercase(),
                identity.model.trim().to_lowercase(),
                identity.serial_number.trim().to_lowercase()
            )
        });
        if reasons.is_empty() {
            let identity = identity.expect("validated physical identity is present");
            let device_key = device_key
                .as_deref()
                .expect("validated device key is present");
            for protocol in &covered_protocols {
                if seen_devices_by_protocol
                    .get(protocol)
                    .is_some_and(|devices| devices.contains(device_key))
                {
                    reasons.push(format!(
                        "physical device {} {} / {} is already counted for {protocol}",
                        identity.manufacturer, identity.model, identity.serial_number
                    ));
                }
            }
        }
        if reasons.is_empty() {
            let identity = identity.expect("validated physical identity is present");
            let device_key = device_key.expect("validated device key is present");
            let broker_receipt = item
                .broker_receipt
                .as_ref()
                .expect("validated broker receipt is present");
            let broker_receipt_sha256 = item
                .broker_receipt_sha256
                .as_ref()
                .expect("validated broker receipt digest is present");
            let native_broker_audit_sha256 = item
                .native_broker_audit_sha256
                .as_ref()
                .expect("validated native broker audit digest is present");
            let native_broker_audit = item
                .native_broker_audit
                .as_ref()
                .expect("validated native broker audit is present");
            for protocol in covered_protocols {
                let protocol_acceptance = item
                    .report
                    .protocol_acceptance
                    .iter()
                    .find(|evidence| {
                        evidence.connection_id.trim() == identity.connection_id.trim()
                            && canonical_protocol_name(&evidence.protocol) == protocol
                    })
                    .expect("validated physical connection acceptance is present");
                seen_devices_by_protocol
                    .entry(protocol.clone())
                    .or_default()
                    .insert(device_key.clone());
                accepted_by_protocol
                    .get_mut(&protocol)
                    .expect("required protocol bucket exists")
                    .push(AcceptedInteroperabilityRun {
                        source: item.source.clone(),
                        report_sha256: item.sha256.clone(),
                        package_sha256: item.report.package_sha256.clone().unwrap_or_default(),
                        edge_id: item.report.edge_id.clone(),
                        config_version: item.report.config_version.clone(),
                        site_id: identity.site_id.trim().to_string(),
                        operator: identity.operator.trim().to_string(),
                        connection_id: identity.connection_id.trim().to_string(),
                        manufacturer: identity.manufacturer.trim().to_string(),
                        model: identity.model.trim().to_string(),
                        serial_number: identity.serial_number.trim().to_string(),
                        observed_duration_ms: item.report.observed_duration_ms,
                        attempted_cycles: item.report.cycles.attempted,
                        failure_ratio: protocol_acceptance.failure_ratio,
                        collection_attempt_count: protocol_acceptance.collection_attempt_count,
                        collection_success_count: protocol_acceptance.collection_success_count,
                        maximum_collection_success_gap_ms: protocol_acceptance
                            .maximum_observed_success_gap_ms,
                        mqtt_publish_success_count: item.report.mqtt.publish_success_count,
                        maximum_mqtt_publish_gap_ms: item
                            .report
                            .mqtt
                            .sink_acceptance
                            .iter()
                            .map(|evidence| evidence.maximum_observed_success_gap_ms)
                            .max()
                            .unwrap_or_default(),
                        broker_receipt_sha256: broker_receipt_sha256.clone(),
                        native_broker_audit_sha256: native_broker_audit_sha256.clone(),
                        native_broker: native_broker_audit.broker.clone(),
                        native_broker_instance_id: native_broker_audit.broker_instance_id.clone(),
                        native_broker_audit_id: native_broker_audit.audit_id.clone(),
                        native_broker_exported_at: native_broker_audit.exported_at,
                        broker_message_count: broker_receipt.message_count,
                        broker_routes: broker_receipt.routes.clone(),
                    });
                accepted_evidence_digests.insert(item.sha256.clone());
            }
        }

        if !reasons.is_empty() {
            rejected_evidence.push(RejectedInteroperabilityEvidence {
                source: item.source.clone(),
                report_sha256: item.sha256.clone(),
                reasons,
            });
        }
    }

    let protocols = required_protocols
        .iter()
        .map(|protocol| {
            let requirement = validated_policy
                .requirements
                .get(protocol)
                .expect("validated required protocol has a coverage requirement");
            let mut runs = accepted_by_protocol.remove(protocol).unwrap_or_default();
            runs.sort_by(|left, right| {
                left.manufacturer
                    .to_lowercase()
                    .cmp(&right.manufacturer.to_lowercase())
                    .then_with(|| left.model.cmp(&right.model))
                    .then_with(|| left.serial_number.cmp(&right.serial_number))
            });
            let manufacturers = runs
                .iter()
                .map(|run| run.manufacturer.trim().to_lowercase())
                .collect::<BTreeSet<_>>();
            let models = runs
                .iter()
                .map(|run| {
                    format!(
                        "{} / {}",
                        run.manufacturer.trim().to_lowercase(),
                        run.model.trim().to_lowercase()
                    )
                })
                .collect::<BTreeSet<_>>();
            let satisfied = manufacturers.len() >= requirement.minimum_manufacturers
                && models.len() >= requirement.minimum_models;
            ProtocolInteroperabilityEvidence {
                protocol: protocol.clone(),
                required_manufacturer_count: requirement.minimum_manufacturers,
                observed_manufacturer_count: manufacturers.len(),
                manufacturers: manufacturers.into_iter().collect(),
                required_model_count: requirement.minimum_models,
                observed_model_count: models.len(),
                models: models.into_iter().collect(),
                satisfied,
                accepted_runs: runs,
            }
        })
        .collect::<Vec<_>>();
    let satisfied_protocol_count = protocols
        .iter()
        .filter(|protocol| protocol.satisfied)
        .count();
    let status =
        if rejected_evidence.is_empty() && satisfied_protocol_count == required_protocols.len() {
            FieldInteroperabilityStatus::Passed
        } else {
            FieldInteroperabilityStatus::Failed
        };

    Ok(FieldInteroperabilityReport {
        schema_version: 4,
        status,
        mode: "physical_field_interoperability",
        policy: FieldInteroperabilityPolicyReport {
            required_protocols: required_protocols.iter().cloned().collect(),
            minimum_manufacturers_per_protocol: policy.minimum_manufacturers_per_protocol,
            minimum_models_per_protocol: policy.minimum_models_per_protocol,
            protocol_requirements: validated_policy
                .requirements
                .iter()
                .map(
                    |(protocol, requirement)| FieldProtocolCoverageRequirementReport {
                        protocol: protocol.clone(),
                        minimum_manufacturers: requirement.minimum_manufacturers,
                        minimum_models: requirement.minimum_models,
                    },
                )
                .collect(),
            minimum_duration_ms: policy.minimum_duration_ms,
            maximum_failure_ratio: policy.maximum_failure_ratio,
            maximum_progress_gap_ms: policy.maximum_progress_gap_ms,
            require_physical_device: true,
            require_mqtt_puback: true,
            require_configuration_package: true,
            require_broker_consumer_receipt: true,
            require_native_broker_audit: true,
        },
        summary: FieldInteroperabilitySummary {
            supplied_evidence_count: evidence.len(),
            accepted_evidence_count: accepted_evidence_digests.len(),
            rejected_evidence_count: rejected_evidence.len(),
            required_protocol_count: required_protocols.len(),
            satisfied_protocol_count,
        },
        protocols,
        rejected_evidence,
    })
}

fn validate_policy(
    policy: &FieldInteroperabilityPolicy,
) -> Result<ValidatedFieldInteroperabilityPolicy> {
    if policy.required_protocols.is_empty() {
        bail!("field interoperability requires at least one protocol");
    }
    if policy.minimum_manufacturers_per_protocol == 0 {
        bail!("field interoperability requires at least one manufacturer per protocol");
    }
    if policy.minimum_models_per_protocol == 0 {
        bail!("field interoperability requires at least one model per protocol");
    }
    if policy.minimum_duration_ms == 0 {
        bail!("field interoperability minimum duration must be greater than zero");
    }
    if !(0.0..=1.0).contains(&policy.maximum_failure_ratio) {
        bail!("field interoperability maximum failure ratio must be between 0 and 1");
    }
    if policy.maximum_progress_gap_ms == 0 {
        bail!("field interoperability maximum progress gap must be greater than zero");
    }
    let required_protocols = policy
        .required_protocols
        .iter()
        .map(|protocol| canonical_protocol_name(protocol))
        .collect::<BTreeSet<_>>();
    if required_protocols.len() != policy.required_protocols.len() {
        bail!("field interoperability required protocols contain duplicate aliases");
    }
    let manufacturer_overrides = canonicalize_policy_overrides(
        "manufacturer",
        &policy.minimum_manufacturers_by_protocol,
        &required_protocols,
    )?;
    let model_overrides = canonicalize_policy_overrides(
        "model",
        &policy.minimum_models_by_protocol,
        &required_protocols,
    )?;
    let requirements = required_protocols
        .iter()
        .map(|protocol| {
            (
                protocol.clone(),
                ProtocolCoverageRequirement {
                    minimum_manufacturers: manufacturer_overrides
                        .get(protocol)
                        .copied()
                        .unwrap_or(policy.minimum_manufacturers_per_protocol),
                    minimum_models: model_overrides
                        .get(protocol)
                        .copied()
                        .unwrap_or(policy.minimum_models_per_protocol),
                },
            )
        })
        .collect();
    Ok(ValidatedFieldInteroperabilityPolicy {
        required_protocols,
        requirements,
    })
}

fn canonicalize_policy_overrides(
    dimension: &str,
    overrides: &BTreeMap<String, usize>,
    required_protocols: &BTreeSet<String>,
) -> Result<BTreeMap<String, usize>> {
    let mut canonical = BTreeMap::new();
    for (protocol, count) in overrides {
        if *count == 0 {
            bail!(
                "field interoperability {dimension} requirement for {protocol} must be greater than zero"
            );
        }
        let protocol = canonical_protocol_name(protocol);
        if !required_protocols.contains(&protocol) {
            bail!(
                "field interoperability {dimension} requirement references non-required protocol {protocol}"
            );
        }
        if canonical.insert(protocol.clone(), *count).is_some() {
            bail!(
                "field interoperability {dimension} requirements contain duplicate protocol alias {protocol}"
            );
        }
    }
    Ok(canonical)
}

fn validate_report(
    policy: &FieldInteroperabilityPolicy,
    item: &FieldInteroperabilityEvidence,
) -> Vec<String> {
    let report = &item.report;
    let mut reasons = Vec::new();
    if report.schema_version != 4 {
        reasons.push(format!(
            "unsupported field endurance schema version {}; version 4 is required",
            report.schema_version
        ));
    }
    if report.status != EvidenceRunStatus::Passed {
        reasons.push("field endurance report status is not passed".to_string());
    }
    if report.mode != "physical_field_endurance" || !report.physical_device_exercised {
        reasons.push("report is not physical field endurance evidence".to_string());
    }
    if !identity_complete(report.physical_device.as_ref()) {
        reasons.push(
            "physical site, operator, connection, manufacturer, model and serial are required"
                .to_string(),
        );
    }
    if report.edge_id.trim().is_empty() || report.config_version.trim().is_empty() {
        reasons.push("edge id and configuration version are required".to_string());
    }
    if !report.package_sha256.as_deref().is_some_and(is_sha256) {
        reasons.push("configuration package SHA-256 is missing or invalid".to_string());
    }
    validate_bound_artifacts(item, &mut reasons);
    if report.configured_duration_ms < policy.minimum_duration_ms
        || report.observed_duration_ms < policy.minimum_duration_ms
    {
        reasons.push(format!(
            "configured and observed duration must both be at least {} ms",
            policy.minimum_duration_ms
        ));
    }
    if report.cycles.attempted == 0 {
        reasons.push("no collection cycles were attempted".to_string());
    }
    if !report.cycles.failure_ratio.is_finite()
        || report.cycles.failure_ratio > policy.maximum_failure_ratio
    {
        reasons.push(format!(
            "failure ratio {} exceeds {}",
            report.cycles.failure_ratio, policy.maximum_failure_ratio
        ));
    }
    validate_protocol_acceptance_evidence(policy, report, &mut reasons);
    if !report.mqtt.exercised
        || report.mqtt.connected_sink_count == 0
        || report.mqtt.publish_success_count == 0
        || report.mqtt.publish_failure_count > 0
        || report.mqtt.pending_outbox_messages > 0
    {
        reasons.push(
            "MQTT PUBACK, connected sink and drained outbox evidence are required".to_string(),
        );
    }
    validate_mqtt_sink_evidence(&report.mqtt, &mut reasons);
    validate_mqtt_continuity_evidence(policy, report, &mut reasons);
    if let Some(identity) = report.physical_device.as_ref() {
        for protocol in report.protocols.iter().filter(|protocol| {
            protocol.connection_id.trim() == identity.connection_id.trim()
                && policy
                    .required_protocols
                    .iter()
                    .map(|required| canonical_protocol_name(required))
                    .any(|required| required == canonical_protocol_name(&protocol.protocol))
        }) {
            if !protocol_has_collection_activity(protocol) {
                reasons.push(format!(
                    "protocol {} ({}) has no successful collection activity",
                    protocol.protocol, protocol.connection_id
                ));
            }
        }
    }
    let criteria = &report.criteria;
    if !criteria.configured_duration_met
        || !criteria.minimum_cycles_met
        || !criteria.failure_ratio_within_limit
        || !criteria.all_configured_points_observed
        || !criteria.changing_points_observed
        || !criteria.protocols_connected_at_finish
        || !criteria.protocol_activity_observed
        || !criteria.protocols_individually_healthy
        || criteria.mqtt_sinks_continuously_publishing != Some(true)
        || !criteria.recovery_observed
        || criteria.mqtt_puback_complete != Some(true)
        || criteria.mqtt_sinks_connected != Some(true)
        || criteria.outbox_drained != Some(true)
        || criteria.physical_identity_complete != Some(true)
        || criteria.production_protocols_only != Some(true)
    {
        reasons.push("one or more mandatory field endurance criteria did not pass".to_string());
    }
    reasons
}

fn validate_protocol_acceptance_evidence(
    policy: &FieldInteroperabilityPolicy,
    report: &FieldEnduranceEvidenceReport,
    reasons: &mut Vec<String>,
) {
    if report.protocol_acceptance.is_empty() {
        reasons.push("per-connection protocol acceptance evidence is required".to_string());
        return;
    }

    let mut seen_connections = BTreeSet::new();
    for evidence in &report.protocol_acceptance {
        let connection_id = evidence.connection_id.trim();
        if connection_id.is_empty() || !seen_connections.insert(connection_id.to_string()) {
            reasons.push(format!(
                "per-connection protocol acceptance contains an empty or duplicate connection id {connection_id}"
            ));
            continue;
        }
        let matching_metrics = report
            .protocols
            .iter()
            .filter(|metrics| metrics.connection_id.trim() == connection_id)
            .collect::<Vec<_>>();
        if matching_metrics.len() != 1 {
            reasons.push(format!(
                "protocol acceptance for {connection_id} requires exactly one matching Runtime metric; found {}",
                matching_metrics.len()
            ));
            continue;
        }
        let metrics = matching_metrics[0];
        if canonical_protocol_name(&evidence.protocol) != canonical_protocol_name(&metrics.protocol)
        {
            reasons.push(format!(
                "protocol acceptance for {connection_id} names {} but Runtime metrics name {}",
                evidence.protocol, metrics.protocol
            ));
        }
        if evidence.connected_at_finish != metrics.connected
            || evidence.circuit_state_at_finish != metrics.circuit_state
        {
            reasons.push(format!(
                "protocol acceptance for {connection_id} does not match the Runtime connection or circuit state"
            ));
        }
        if evidence.collection_attempt_count != metrics.collection_attempt_count
            || evidence.collection_success_count != metrics.collection_success_count
        {
            reasons.push(format!(
                "protocol acceptance counters for {connection_id} do not match Runtime metrics"
            ));
        }

        let expected_failures = evidence
            .collection_attempt_count
            .saturating_sub(evidence.collection_success_count);
        let expected_failure_ratio = if evidence.collection_attempt_count == 0 {
            1.0
        } else {
            expected_failures as f64 / evidence.collection_attempt_count as f64
        };
        let expected_activity = evidence.collection_attempt_count > 0
            && evidence.collection_success_count > 0
            && evidence.collection_success_count <= evidence.collection_attempt_count;
        if evidence.collection_failure_count != expected_failures
            || !evidence.failure_ratio.is_finite()
            || (evidence.failure_ratio - expected_failure_ratio).abs() > 1e-12
            || evidence.activity_observed != expected_activity
        {
            reasons.push(format!(
                "protocol acceptance arithmetic for {connection_id} is inconsistent"
            ));
        }
        if evidence.failure_ratio > policy.maximum_failure_ratio {
            reasons.push(format!(
                "protocol connection {connection_id} failure ratio {} exceeds {}",
                evidence.failure_ratio, policy.maximum_failure_ratio
            ));
        }
        let expected_continuity = expected_activity
            && !evidence.counter_reset_observed
            && evidence.maximum_allowed_success_gap_ms > 0
            && evidence.maximum_allowed_success_gap_ms <= policy.maximum_progress_gap_ms
            && evidence.maximum_observed_success_gap_ms <= evidence.maximum_allowed_success_gap_ms;
        let expected_pass = evidence.connected_at_finish
            && evidence.circuit_state_at_finish == EvidenceCircuitState::Closed
            && expected_activity
            && evidence.failure_ratio_within_limit
            && expected_continuity;
        if evidence.continuous_activity != expected_continuity {
            reasons.push(format!(
                "protocol connection {connection_id} continuity evidence is inconsistent"
            ));
        }
        if evidence.maximum_allowed_success_gap_ms == 0
            || evidence.maximum_allowed_success_gap_ms > policy.maximum_progress_gap_ms
        {
            reasons.push(format!(
                "protocol connection {connection_id} allows a success gap of {} ms; policy maximum is {} ms",
                evidence.maximum_allowed_success_gap_ms, policy.maximum_progress_gap_ms
            ));
        }
        if evidence.maximum_observed_success_gap_ms > evidence.maximum_allowed_success_gap_ms
            || evidence.counter_reset_observed
        {
            reasons.push(format!(
                "protocol connection {connection_id} did not maintain continuous collection progress"
            ));
        }
        if !evidence.failure_ratio_within_limit
            || !evidence.passed
            || evidence.passed != expected_pass
        {
            reasons.push(format!(
                "protocol connection {connection_id} did not pass its independent acceptance criteria"
            ));
        }
    }

    if let Some(identity) = report.physical_device.as_ref() {
        let bound = report
            .protocol_acceptance
            .iter()
            .filter(|evidence| evidence.connection_id.trim() == identity.connection_id.trim())
            .count();
        if bound != 1 {
            reasons.push(format!(
                "physical device connection {} requires exactly one protocol acceptance record; found {bound}",
                identity.connection_id.trim()
            ));
        }
    }
}

fn validate_mqtt_continuity_evidence(
    policy: &FieldInteroperabilityPolicy,
    report: &FieldEnduranceEvidenceReport,
    reasons: &mut Vec<String>,
) {
    if report.mqtt.sink_acceptance.is_empty() {
        reasons.push("per-sink MQTT continuity evidence is required".to_string());
        return;
    }
    let mut seen_sinks = BTreeSet::new();
    for evidence in &report.mqtt.sink_acceptance {
        let sink_id = evidence.sink_id.trim();
        if sink_id.is_empty() || !seen_sinks.insert(sink_id.to_string()) {
            reasons.push(format!(
                "MQTT continuity evidence contains an empty or duplicate sink id {sink_id}"
            ));
            continue;
        }
        let matching = report
            .mqtt
            .sinks
            .iter()
            .filter(|sink| sink.sink_id.trim() == sink_id)
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            reasons.push(format!(
                "MQTT continuity for {sink_id} requires exactly one matching Runtime sink; found {}",
                matching.len()
            ));
            continue;
        }
        if evidence.publish_success_count != matching[0].publish_success_count {
            reasons.push(format!(
                "MQTT continuity publish count for {sink_id} does not match Runtime sink metrics"
            ));
        }
        let expected_continuity = evidence.publish_success_count > 0
            && !evidence.counter_reset_observed
            && evidence.maximum_allowed_success_gap_ms > 0
            && evidence.maximum_allowed_success_gap_ms <= policy.maximum_progress_gap_ms
            && evidence.maximum_observed_success_gap_ms <= evidence.maximum_allowed_success_gap_ms;
        if evidence.continuous_activity != expected_continuity
            || evidence.passed != expected_continuity
        {
            reasons.push(format!(
                "MQTT sink {sink_id} continuity evidence is inconsistent"
            ));
        }
        if evidence.maximum_allowed_success_gap_ms == 0
            || evidence.maximum_allowed_success_gap_ms > policy.maximum_progress_gap_ms
        {
            reasons.push(format!(
                "MQTT sink {sink_id} allows a success gap of {} ms; policy maximum is {} ms",
                evidence.maximum_allowed_success_gap_ms, policy.maximum_progress_gap_ms
            ));
        }
        if evidence.maximum_observed_success_gap_ms > evidence.maximum_allowed_success_gap_ms
            || evidence.counter_reset_observed
        {
            reasons.push(format!(
                "MQTT sink {sink_id} did not maintain continuous publish progress"
            ));
        }
    }
    let active_runtime_sinks = report
        .mqtt
        .sinks
        .iter()
        .filter(|sink| sink.publish_success_count > 0)
        .map(|sink| sink.sink_id.trim().to_string())
        .collect::<BTreeSet<_>>();
    if seen_sinks != active_runtime_sinks {
        reasons.push(format!(
            "MQTT continuity sinks {:?} do not match publishing Runtime sinks {:?}",
            seen_sinks, active_runtime_sinks
        ));
    }
}

fn protocol_has_collection_activity(protocol: &EvidenceProtocol) -> bool {
    protocol.collection_attempt_count > 0
        && protocol.collection_success_count > 0
        && protocol.collection_success_count <= protocol.collection_attempt_count
}

fn validate_mqtt_sink_evidence(mqtt: &EvidenceMqtt, reasons: &mut Vec<String>) {
    if mqtt.sinks.is_empty() {
        reasons.push("Runtime MQTT sink-level evidence is required".to_string());
        return;
    }
    let connected_sink_count = mqtt.sinks.iter().filter(|sink| sink.connected).count();
    if connected_sink_count != mqtt.connected_sink_count {
        reasons.push(format!(
            "Runtime MQTT connected sink detail count {connected_sink_count} does not match aggregate {}",
            mqtt.connected_sink_count
        ));
    }
    let mut sink_publish_success_count = 0_u64;
    for sink in &mqtt.sinks {
        if sink.publish_success_count > 0 && sink.broker.trim().is_empty() {
            reasons.push("Runtime MQTT sink with successful publishes has no broker".to_string());
        }
        match sink_publish_success_count.checked_add(sink.publish_success_count) {
            Some(total) => sink_publish_success_count = total,
            None => reasons.push("Runtime MQTT sink publish count overflow".to_string()),
        }
    }
    if sink_publish_success_count != mqtt.publish_success_count {
        reasons.push(format!(
            "Runtime MQTT sink publish count {sink_publish_success_count} does not match aggregate {}",
            mqtt.publish_success_count
        ));
    }
}

fn validate_bound_artifacts(item: &FieldInteroperabilityEvidence, reasons: &mut Vec<String>) {
    let report = &item.report;
    match item.package.as_ref() {
        Some(package) => {
            if report.package_sha256.as_deref() != Some(package.sha256.as_str()) {
                reasons.push(
                    "bound configuration package digest does not match the field report"
                        .to_string(),
                );
            }
            if package.edge_id != report.edge_id || package.config_version != report.config_version
            {
                reasons.push(
                    "bound configuration package edge id or version does not match the field report"
                        .to_string(),
                );
            }
            let report_connections = report
                .protocol_acceptance
                .iter()
                .map(|evidence| evidence.connection_id.trim().to_string())
                .collect::<BTreeSet<_>>();
            if report_connections != package.used_connection_ids {
                reasons.push(format!(
                    "protocol acceptance connections {:?} do not match enabled package connections {:?}",
                    report_connections, package.used_connection_ids
                ));
            }
            let report_sinks = report
                .mqtt
                .sink_acceptance
                .iter()
                .map(|evidence| evidence.sink_id.trim().to_string())
                .collect::<BTreeSet<_>>();
            if report_sinks != package.used_sink_ids {
                reasons.push(format!(
                    "MQTT continuity sinks {:?} do not match enabled package sinks {:?}",
                    report_sinks, package.used_sink_ids
                ));
            }
            if let Some(identity) = report.physical_device.as_ref() {
                let connection_id = identity.connection_id.trim();
                if !connection_id.is_empty() {
                    match package.protocol_connections.get(connection_id) {
                        Some(package_protocol) => {
                            let matching_metrics = report
                                .protocols
                                .iter()
                                .filter(|metrics| metrics.connection_id.trim() == connection_id)
                                .collect::<Vec<_>>();
                            if matching_metrics.is_empty() {
                                reasons.push(format!(
                                    "physical device connection {connection_id} has no Runtime protocol metrics"
                                ));
                            }
                            for metrics in matching_metrics {
                                let report_protocol = canonical_protocol_name(&metrics.protocol);
                                if &report_protocol != package_protocol {
                                    reasons.push(format!(
                                        "Runtime protocol {} for physical device connection {connection_id} does not match bound package protocol {package_protocol}",
                                        metrics.protocol
                                    ));
                                }
                            }
                        }
                        None => reasons.push(format!(
                            "physical device connection {connection_id} does not exist in the bound configuration package"
                        )),
                    }
                }
            }
        }
        None => reasons.push("bound configuration package is required".to_string()),
    }

    let Some(receipt) = item.broker_receipt.as_ref() else {
        reasons.push("broker consumer receipt is required".to_string());
        return;
    };
    if item
        .broker_receipt_sha256
        .as_deref()
        .is_none_or(|digest| !is_sha256(digest))
    {
        reasons.push("broker consumer receipt digest is missing or invalid".to_string());
    }
    if item
        .native_broker_audit_sha256
        .as_deref()
        .is_none_or(|digest| !is_sha256(digest))
    {
        reasons.push("native broker audit artifact is required".to_string());
    }
    match item.native_broker_audit.as_ref() {
        Some(audit) => reasons.extend(audit.validation_errors_against(receipt)),
        None => reasons.push("structured native broker audit is required".to_string()),
    }
    if receipt.schema_version != 1 {
        reasons.push(format!(
            "unsupported broker consumer receipt schema version {}",
            receipt.schema_version
        ));
    }
    if receipt.edge_id != report.edge_id || receipt.config_version != report.config_version {
        reasons.push(
            "broker consumer receipt edge id or version does not match the field report"
                .to_string(),
        );
    }
    if report.package_sha256.as_deref() != Some(receipt.package_sha256.as_str()) {
        reasons.push(
            "broker consumer receipt package digest does not match the field report".to_string(),
        );
    }
    if receipt.message_count != report.mqtt.publish_success_count {
        reasons.push(format!(
            "broker consumer receipt message count {} does not match Runtime publish success count {}",
            receipt.message_count, report.mqtt.publish_success_count
        ));
    }
    if receipt.last_received_at < receipt.first_received_at {
        reasons.push("broker consumer receipt timestamps are reversed".to_string());
    }
    if receipt.routes.is_empty() {
        reasons.push("broker consumer receipt must contain at least one route".to_string());
    }
    let mut route_message_count = 0_u64;
    let mut seen_routes = BTreeSet::new();
    for route in &receipt.routes {
        if route.broker.trim().is_empty() || route.consumer_id.trim().is_empty() {
            reasons.push("broker and consumer id are required for every receipt route".to_string());
        }
        if route.message_count == 0 {
            reasons.push(format!(
                "broker receipt route {} / {} contains no messages",
                route.broker, route.consumer_id
            ));
        }
        if route.topics.is_empty() || route.topics.iter().any(|topic| topic.trim().is_empty()) {
            reasons.push(format!(
                "broker receipt route {} / {} must contain non-empty topics",
                route.broker, route.consumer_id
            ));
        }
        let route_key = format!(
            "{}\u{0}{}",
            route.broker.trim().to_lowercase(),
            route.consumer_id.trim().to_lowercase()
        );
        if !seen_routes.insert(route_key) {
            reasons.push(format!(
                "duplicate broker receipt route {} / {}",
                route.broker, route.consumer_id
            ));
        }
        match route_message_count.checked_add(route.message_count) {
            Some(total) => route_message_count = total,
            None => reasons.push("broker receipt route message count overflow".to_string()),
        }
    }
    if route_message_count != receipt.message_count {
        reasons.push(format!(
            "broker receipt route message count {route_message_count} does not match receipt total {}",
            receipt.message_count
        ));
    }
    for sink in report
        .mqtt
        .sinks
        .iter()
        .filter(|sink| sink.publish_success_count > 0)
    {
        let Some(topic) = sink.last_topic.as_deref() else {
            reasons.push(format!(
                "Runtime MQTT sink {} has successful publishes but no last topic",
                sink.broker
            ));
            continue;
        };
        let route_matches = receipt.routes.iter().any(|route| {
            route.broker == sink.broker && route.topics.iter().any(|candidate| candidate == topic)
        });
        if !route_matches {
            reasons.push(format!(
                "broker consumer receipt does not contain Runtime route {} / {}",
                sink.broker, topic
            ));
        }
    }
}

fn identity_complete(identity: Option<&EvidenceDeviceIdentity>) -> bool {
    identity.is_some_and(|identity| {
        [
            identity.site_id.as_str(),
            identity.operator.as_str(),
            identity.connection_id.as_str(),
            identity.manufacturer.as_str(),
            identity.model.as_str(),
            identity.serial_number.as_str(),
        ]
        .iter()
        .all(|value| !value.trim().is_empty())
    })
}

pub fn field_protocol_name(protocol: ProtocolType) -> &'static str {
    match protocol {
        ProtocolType::Simulated => "Simulated",
        ProtocolType::ModbusTcp => "Modbus TCP",
        ProtocolType::ModbusRtu => "Modbus RTU",
        ProtocolType::Dlt645 => "DL/T 645-2007",
        ProtocolType::Iec101 => "IEC-101",
        ProtocolType::Iec104 => "IEC-104",
        ProtocolType::CustomSerial => "Custom Serial",
        ProtocolType::OpcUa => "OPC UA",
        ProtocolType::BacnetIp => "BACnet/IP",
        ProtocolType::SiemensS7 => "Siemens S7",
        ProtocolType::OmronFins => "Omron FINS",
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn canonical_protocol_name(protocol: &str) -> String {
    match protocol.trim().to_ascii_lowercase().as_str() {
        "modbus tcp" | "modbus-tcp" | "modbus_tcp" => "Modbus TCP".to_string(),
        "modbus rtu" | "modbus-rtu" | "modbus_rtu" => "Modbus RTU".to_string(),
        "dlt645" | "dlt-645" | "dlt 645" | "dlt645-2007" | "dlt-645-2007" | "dl/t645"
        | "dl/t 645" | "dl/t645-2007" | "dl/t 645-2007" => "DL/T 645-2007".to_string(),
        "iec-101"
        | "iec101"
        | "iec 60870-5-101"
        | "iec-60870-5-101"
        | "iec60870-5-101-unbalanced" => "IEC-101".to_string(),
        "iec-104" | "iec104" | "iec-60870-5-104" | "iec60870-5-104-client" => "IEC-104".to_string(),
        "custom serial" | "custom-serial" | "custom_serial" => "Custom Serial".to_string(),
        "opc ua" | "opcua" | "opc-ua" | "opc-ua-client" => "OPC UA".to_string(),
        "bacnet/ip" | "bacnet ip" | "bacnet-ip" | "bacnet_ip" => "BACnet/IP".to_string(),
        "siemens s7" | "siemens-s7" | "siemens_s7" | "s7" => "Siemens S7".to_string(),
        "omron fins" | "omron-fins" | "omron_fins" | "fins" => "Omron FINS".to_string(),
        _ => protocol.trim().to_string(),
    }
}
