use edge_core::EdgeConfigPackage;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AuditAction, CloudControlStore, ConfigValidator, ValidationError};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseRecord {
    pub release_id: Uuid,
    pub edge_id: String,
    pub desired_version: String,
    pub reported_version: Option<String>,
    pub status: ReleaseStatus,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReleaseStatus {
    Pending,
    Applied,
    Failed,
}

pub struct ReleaseService;

impl ReleaseService {
    pub fn create_release(
        store: &mut CloudControlStore,
        package: EdgeConfigPackage,
    ) -> Result<ReleaseRecord, Vec<ValidationError>> {
        let errors = ConfigValidator::validate_package(&package);
        if !errors.is_empty() {
            return Err(errors);
        }

        let release = ReleaseRecord {
            release_id: Uuid::new_v4(),
            edge_id: package.edge_id.clone(),
            desired_version: package.version.clone(),
            reported_version: None,
            status: ReleaseStatus::Pending,
        };

        store.upsert_config_package(package);
        store.insert_release(release.clone());
        store.push_audit(AuditAction::CreateRelease, release.release_id.to_string());
        Ok(release)
    }

    pub fn mark_reported(
        store: &mut CloudControlStore,
        release_id: Uuid,
        reported_version: impl Into<String>,
    ) -> Option<ReleaseRecord> {
        let reported_version = reported_version.into();
        let release = store.release_mut(release_id)?;
        release.reported_version = Some(reported_version.clone());
        release.status = if release.desired_version == reported_version {
            ReleaseStatus::Applied
        } else {
            ReleaseStatus::Failed
        };
        let cloned = release.clone();
        store.push_audit(AuditAction::ApplyRelease, release_id.to_string());
        Some(cloned)
    }
}
