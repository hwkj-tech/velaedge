use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::FieldInteroperabilityEvidence;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldCampaignArtifact {
    pub file: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldCampaignManifest {
    pub schema_version: u32,
    pub status: String,
    pub phase: String,
    pub edge_id: String,
    pub config_version: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub package: Option<FieldCampaignArtifact>,
    pub runtime_report: Option<FieldCampaignArtifact>,
    pub broker_receipt: Option<FieldCampaignArtifact>,
    pub native_broker_audit: Option<FieldCampaignArtifact>,
    pub native_broker_audit_required: bool,
    #[serde(default)]
    pub errors: Vec<String>,
}

pub fn read_field_campaign_manifest(directory: &Path) -> Result<FieldCampaignManifest> {
    let manifest_path = directory.join("manifest.json");
    let manifest_bytes = fs::read(&manifest_path)
        .with_context(|| format!("read field campaign manifest {}", manifest_path.display()))?;
    let manifest = serde_json::from_slice::<FieldCampaignManifest>(&manifest_bytes)
        .with_context(|| format!("decode field campaign manifest {}", manifest_path.display()))?;
    if manifest.schema_version != 3 {
        bail!(
            "field campaign {} uses unsupported manifest schema {}",
            directory.display(),
            manifest.schema_version
        );
    }
    Ok(manifest)
}

pub fn read_field_campaign_evidence(directory: &Path) -> Result<FieldInteroperabilityEvidence> {
    let manifest = read_field_campaign_manifest(directory)?;
    if manifest.status != "passed" || manifest.phase != "complete" {
        bail!(
            "field campaign {} is not complete and passed (status={}, phase={})",
            directory.display(),
            manifest.status,
            manifest.phase
        );
    }
    if !manifest.native_broker_audit_required {
        bail!(
            "field campaign {} does not preserve the native broker audit requirement",
            directory.display()
        );
    }
    let package = manifest.package.as_ref().with_context(|| {
        format!(
            "field campaign {} manifest is missing package artifact",
            directory.display()
        )
    })?;
    let report = manifest.runtime_report.as_ref().with_context(|| {
        format!(
            "field campaign {} manifest is missing Runtime report artifact",
            directory.display()
        )
    })?;
    let receipt = manifest.broker_receipt.as_ref().with_context(|| {
        format!(
            "field campaign {} manifest is missing broker receipt artifact",
            directory.display()
        )
    })?;
    let native_broker_audit = manifest.native_broker_audit.as_ref().with_context(|| {
        format!(
            "field campaign {} manifest is missing native broker audit artifact",
            directory.display()
        )
    })?;
    let package_path = verified_field_campaign_artifact(directory, package)?;
    let report_path = verified_field_campaign_artifact(directory, report)?;
    let receipt_path = verified_field_campaign_artifact(directory, receipt)?;
    let native_broker_audit_path =
        verified_field_campaign_artifact(directory, native_broker_audit)?;
    read_field_interoperability_artifacts(
        &report_path,
        &package_path,
        &receipt_path,
        &native_broker_audit_path,
        directory.display().to_string(),
    )
}

pub fn read_field_interoperability_artifacts(
    report_path: &Path,
    package_path: &Path,
    broker_receipt_path: &Path,
    native_broker_audit_path: &Path,
    source: String,
) -> Result<FieldInteroperabilityEvidence> {
    let report_bytes = fs::read(report_path)
        .with_context(|| format!("read field endurance report {}", report_path.display()))?;
    let package_bytes = fs::read(package_path)
        .with_context(|| format!("read configuration package {}", package_path.display()))?;
    let broker_receipt_bytes = fs::read(broker_receipt_path).with_context(|| {
        format!(
            "read broker consumer receipt {}",
            broker_receipt_path.display()
        )
    })?;
    let native_broker_audit_bytes = fs::read(native_broker_audit_path).with_context(|| {
        format!(
            "read native broker audit {}",
            native_broker_audit_path.display()
        )
    })?;
    FieldInteroperabilityEvidence::from_artifacts(
        source,
        &report_bytes,
        &package_bytes,
        &broker_receipt_bytes,
        &native_broker_audit_bytes,
    )
}

fn verified_field_campaign_artifact(
    directory: &Path,
    artifact: &FieldCampaignArtifact,
) -> Result<PathBuf> {
    let relative = Path::new(&artifact.file);
    let mut components = relative.components();
    let is_single_file =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();
    if !is_single_file {
        bail!(
            "field campaign {} contains unsafe artifact path {}",
            directory.display(),
            artifact.file
        );
    }
    let path = directory.join(relative);
    let bytes = fs::read(&path)
        .with_context(|| format!("read field campaign artifact {}", path.display()))?;
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if actual != artifact.sha256 {
        bail!(
            "field campaign artifact {} digest does not match manifest",
            path.display()
        );
    }
    Ok(path)
}
