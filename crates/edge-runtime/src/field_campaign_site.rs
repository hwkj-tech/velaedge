use std::{fs, path::Path};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::{
    evaluate_field_campaign_plan_for_site_status, evaluate_field_interoperability,
    read_field_campaign_evidence, read_field_campaign_manifest, AcceptedInteroperabilityRun,
    FieldCampaignDeploymentPlan, FieldCampaignManifest, FieldCampaignPlanReport,
    FieldCampaignPlanStatus, FieldInteroperabilityEvidence, FieldInteroperabilityPolicy,
    FieldInteroperabilityReport,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldCampaignSiteStatus {
    Pending,
    Running,
    Passed,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldCampaignExecutionStatus {
    Pending,
    Running,
    Passed,
    Failed,
    Invalid,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldCampaignSiteReport {
    pub schema_version: u32,
    pub status: FieldCampaignSiteStatus,
    pub mode: &'static str,
    pub plan_sha256: String,
    pub policy_sha256: String,
    pub site_id: String,
    pub summary: FieldCampaignSiteSummary,
    pub campaigns: Vec<FieldCampaignExecutionReport>,
    pub plan_validation: FieldCampaignPlanReport,
    pub interoperability: FieldInteroperabilityReport,
    pub errors: Vec<String>,
}

impl FieldCampaignSiteReport {
    pub fn passed(&self) -> bool {
        self.status == FieldCampaignSiteStatus::Passed
    }

    pub fn failed(&self) -> bool {
        self.status == FieldCampaignSiteStatus::Failed
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldCampaignSiteSummary {
    pub campaign_count: usize,
    pub pending_count: usize,
    pub running_count: usize,
    pub passed_count: usize,
    pub failed_count: usize,
    pub invalid_count: usize,
    pub required_protocol_count: usize,
    pub satisfied_protocol_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldCampaignExecutionReport {
    pub campaign_id: String,
    pub protocol: Option<String>,
    pub output_dir: String,
    pub status: FieldCampaignExecutionStatus,
    pub phase: Option<String>,
    pub edge_id: Option<String>,
    pub config_version: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub reasons: Vec<String>,
}

pub fn evaluate_field_campaign_site_status(
    plan_bytes: &[u8],
    policy: &FieldInteroperabilityPolicy,
    policy_sha256: impl Into<String>,
) -> Result<FieldCampaignSiteReport> {
    let policy_sha256 = policy_sha256.into();
    let plan = FieldCampaignDeploymentPlan::from_json_slice(plan_bytes)?;
    let plan_validation =
        evaluate_field_campaign_plan_for_site_status(plan_bytes, policy, policy_sha256.clone())?;
    let mut evidence = Vec::<FieldInteroperabilityEvidence>::new();
    let mut campaigns = Vec::with_capacity(plan.campaigns.len());

    for (entry, validation) in plan.campaigns.iter().zip(plan_validation.campaigns.iter()) {
        let mut campaign = FieldCampaignExecutionReport {
            campaign_id: validation.campaign_id.clone(),
            protocol: validation.protocol.clone(),
            output_dir: validation.output_dir.clone(),
            status: FieldCampaignExecutionStatus::Pending,
            phase: None,
            edge_id: validation.edge_id.clone(),
            config_version: validation.config_version.clone(),
            started_at: None,
            finished_at: None,
            reasons: validation.reasons.clone(),
        };
        if !validation.ready {
            campaign.status = FieldCampaignExecutionStatus::Invalid;
            campaigns.push(campaign);
            continue;
        }

        inspect_campaign_output(&entry.output_dir, &mut campaign, &mut evidence);
        campaigns.push(campaign);
    }

    let interoperability = evaluate_field_interoperability(policy, &evidence)?;
    bind_completed_campaigns_to_plan(&plan, &plan_validation, &interoperability, &mut campaigns);

    let pending_count = count_status(&campaigns, FieldCampaignExecutionStatus::Pending);
    let running_count = count_status(&campaigns, FieldCampaignExecutionStatus::Running);
    let passed_count = count_status(&campaigns, FieldCampaignExecutionStatus::Passed);
    let failed_count = count_status(&campaigns, FieldCampaignExecutionStatus::Failed);
    let invalid_count = count_status(&campaigns, FieldCampaignExecutionStatus::Invalid);
    let status = if plan_validation.status == FieldCampaignPlanStatus::Failed
        || failed_count > 0
        || invalid_count > 0
    {
        FieldCampaignSiteStatus::Failed
    } else if passed_count == campaigns.len() && interoperability.passed() {
        FieldCampaignSiteStatus::Passed
    } else if running_count > 0 {
        FieldCampaignSiteStatus::Running
    } else {
        FieldCampaignSiteStatus::Pending
    };
    let mut errors = plan_validation.errors.clone();
    for campaign in campaigns.iter().filter(|campaign| {
        matches!(
            campaign.status,
            FieldCampaignExecutionStatus::Failed | FieldCampaignExecutionStatus::Invalid
        )
    }) {
        errors.extend(
            campaign
                .reasons
                .iter()
                .map(|reason| format!("campaign {}: {reason}", campaign.campaign_id)),
        );
    }
    errors.sort();
    errors.dedup();

    Ok(FieldCampaignSiteReport {
        schema_version: 1,
        status,
        mode: "physical_field_campaign_site_status",
        plan_sha256: plan_validation.plan_sha256.clone(),
        policy_sha256,
        site_id: plan.site_id,
        summary: FieldCampaignSiteSummary {
            campaign_count: campaigns.len(),
            pending_count,
            running_count,
            passed_count,
            failed_count,
            invalid_count,
            required_protocol_count: interoperability.summary.required_protocol_count,
            satisfied_protocol_count: interoperability.summary.satisfied_protocol_count,
        },
        campaigns,
        plan_validation,
        interoperability,
        errors,
    })
}

fn inspect_campaign_output(
    output_dir: &Path,
    campaign: &mut FieldCampaignExecutionReport,
    evidence: &mut Vec<FieldInteroperabilityEvidence>,
) {
    if !output_dir.exists() {
        return;
    }
    if !output_dir.is_dir() {
        campaign.status = FieldCampaignExecutionStatus::Invalid;
        campaign
            .reasons
            .push("outputDir exists and is not a directory".to_string());
        return;
    }
    let manifest_path = output_dir.join("manifest.json");
    if !manifest_path.exists() {
        match fs::read_dir(output_dir) {
            Ok(mut entries) => {
                if entries.next().is_some() {
                    campaign.status = FieldCampaignExecutionStatus::Invalid;
                    campaign
                        .reasons
                        .push("outputDir contains files but has no campaign manifest".to_string());
                }
            }
            Err(error) => {
                campaign.status = FieldCampaignExecutionStatus::Invalid;
                campaign
                    .reasons
                    .push(format!("cannot inspect outputDir: {error}"));
            }
        }
        return;
    }

    let manifest = match read_field_campaign_manifest(output_dir) {
        Ok(manifest) => manifest,
        Err(error) => {
            campaign.status = FieldCampaignExecutionStatus::Invalid;
            campaign
                .reasons
                .push(format!("campaign manifest is invalid: {error:#}"));
            return;
        }
    };
    apply_manifest(campaign, &manifest);
    match (manifest.status.as_str(), manifest.phase.as_str()) {
        ("running", _) => campaign.status = FieldCampaignExecutionStatus::Running,
        ("failed", _) => {
            campaign.status = FieldCampaignExecutionStatus::Failed;
            campaign.reasons.extend(manifest.errors);
        }
        ("passed", "complete") => match read_field_campaign_evidence(output_dir) {
            Ok(item) => {
                campaign.status = FieldCampaignExecutionStatus::Passed;
                evidence.push(item);
            }
            Err(error) => {
                campaign.status = FieldCampaignExecutionStatus::Failed;
                campaign
                    .reasons
                    .push(format!("completed campaign evidence is invalid: {error:#}"));
            }
        },
        (status, phase) => {
            campaign.status = FieldCampaignExecutionStatus::Invalid;
            campaign.reasons.push(format!(
                "campaign manifest has unsupported status/phase {status}/{phase}"
            ));
        }
    }
}

fn apply_manifest(campaign: &mut FieldCampaignExecutionReport, manifest: &FieldCampaignManifest) {
    campaign.phase = Some(manifest.phase.clone());
    campaign.edge_id = Some(manifest.edge_id.clone());
    campaign.config_version = Some(manifest.config_version.clone());
    campaign.started_at = Some(manifest.started_at);
    campaign.finished_at = Some(manifest.finished_at);
}

fn bind_completed_campaigns_to_plan(
    plan: &FieldCampaignDeploymentPlan,
    plan_validation: &FieldCampaignPlanReport,
    interoperability: &FieldInteroperabilityReport,
    campaigns: &mut [FieldCampaignExecutionReport],
) {
    let accepted = interoperability
        .protocols
        .iter()
        .flat_map(|protocol| protocol.accepted_runs.iter())
        .collect::<Vec<_>>();
    for ((entry, validation), campaign) in plan
        .campaigns
        .iter()
        .zip(plan_validation.campaigns.iter())
        .zip(campaigns.iter_mut())
    {
        if campaign.status != FieldCampaignExecutionStatus::Passed {
            continue;
        }
        let source = entry.output_dir.display().to_string();
        if let Some(rejected) = interoperability
            .rejected_evidence
            .iter()
            .find(|rejected| rejected.source == source)
        {
            campaign.status = FieldCampaignExecutionStatus::Failed;
            campaign.reasons.extend(rejected.reasons.clone());
            continue;
        }
        let Some(run) = accepted.iter().find(|run| run.source == source) else {
            campaign.status = FieldCampaignExecutionStatus::Failed;
            campaign.reasons.push(
                "completed campaign was not accepted by the interoperability gate".to_string(),
            );
            continue;
        };
        validate_plan_binding(&plan.site_id, entry, validation, run, &mut campaign.reasons);
        if !campaign.reasons.is_empty() {
            campaign.status = FieldCampaignExecutionStatus::Failed;
        }
    }
}

fn validate_plan_binding(
    site_id: &str,
    entry: &crate::FieldCampaignPlanEntry,
    validation: &crate::FieldCampaignPlanEntryReport,
    run: &AcceptedInteroperabilityRun,
    reasons: &mut Vec<String>,
) {
    compare_binding("siteId", &run.site_id, site_id, reasons);
    compare_binding("operator", &run.operator, &entry.operator, reasons);
    compare_binding(
        "connectionId",
        &run.connection_id,
        &entry.physical_device.connection_id,
        reasons,
    );
    compare_binding(
        "manufacturer",
        &run.manufacturer,
        &entry.physical_device.manufacturer,
        reasons,
    );
    compare_binding("model", &run.model, &entry.physical_device.model, reasons);
    compare_binding(
        "serialNumber",
        &run.serial_number,
        &entry.physical_device.serial_number,
        reasons,
    );
    if validation.package_sha256.as_deref() != Some(run.package_sha256.as_str()) {
        reasons.push("completed package SHA-256 does not match the planned package".to_string());
    }
    if validation.edge_id.as_deref() != Some(run.edge_id.as_str()) {
        reasons.push("completed edgeId does not match the planned package".to_string());
    }
    if validation.config_version.as_deref() != Some(run.config_version.as_str()) {
        reasons.push("completed configVersion does not match the planned package".to_string());
    }
}

fn compare_binding(label: &str, actual: &str, expected: &str, reasons: &mut Vec<String>) {
    if actual.trim() != expected.trim() {
        reasons.push(format!(
            "completed {label} {actual:?} does not match planned value {expected:?}"
        ));
    }
}

fn count_status(
    campaigns: &[FieldCampaignExecutionReport],
    status: FieldCampaignExecutionStatus,
) -> usize {
    campaigns
        .iter()
        .filter(|campaign| campaign.status == status)
        .count()
}
