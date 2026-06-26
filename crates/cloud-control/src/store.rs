use std::collections::BTreeMap;

use edge_core::{DeviceSpec, EdgeConfigPackage};
use uuid::Uuid;

use crate::{AuditAction, AuditRecord, ReleaseRecord};

#[derive(Clone, Debug, Default)]
pub struct CloudControlStore {
    device_models: BTreeMap<String, DeviceSpec>,
    config_packages: BTreeMap<(String, String), EdgeConfigPackage>,
    releases: BTreeMap<Uuid, ReleaseRecord>,
    audit_records: Vec<AuditRecord>,
}

impl CloudControlStore {
    pub fn upsert_device_model(&mut self, model: DeviceSpec) {
        self.device_models.insert(model.device_type.clone(), model);
    }

    pub fn device_model(&self, device_type: &str) -> Option<&DeviceSpec> {
        self.device_models.get(device_type)
    }

    pub fn upsert_config_package(&mut self, package: EdgeConfigPackage) {
        self.config_packages
            .insert((package.edge_id.clone(), package.version.clone()), package);
    }

    pub fn config_package(&self, edge_id: &str, version: &str) -> Option<&EdgeConfigPackage> {
        self.config_packages
            .get(&(edge_id.to_string(), version.to_string()))
    }

    pub fn insert_release(&mut self, release: ReleaseRecord) {
        self.releases.insert(release.release_id, release);
    }

    pub fn release(&self, release_id: Uuid) -> Option<&ReleaseRecord> {
        self.releases.get(&release_id)
    }

    pub fn release_mut(&mut self, release_id: Uuid) -> Option<&mut ReleaseRecord> {
        self.releases.get_mut(&release_id)
    }

    pub fn push_audit(&mut self, action: AuditAction, target: impl Into<String>) {
        self.audit_records.push(AuditRecord::system(action, target));
    }

    pub fn audit_records(&self) -> &[AuditRecord] {
        &self.audit_records
    }
}
