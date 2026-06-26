use std::collections::BTreeMap;

use edge_core::{DeviceSpec, EdgeConfigPackage, EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot};
use uuid::Uuid;

use crate::{AuditAction, AuditRecord, EdgeNode, ReleaseRecord};

#[derive(Clone, Debug, Default)]
pub struct CloudControlStore {
    edge_nodes: BTreeMap<String, EdgeNode>,
    device_models: BTreeMap<String, DeviceSpec>,
    config_packages: BTreeMap<(String, String), EdgeConfigPackage>,
    releases: BTreeMap<Uuid, ReleaseRecord>,
    audit_records: Vec<AuditRecord>,
    runtime_metrics: BTreeMap<String, EdgeRuntimeMetricsSnapshot>,
    runtime_events: Vec<EdgeRuntimeEvent>,
}

impl CloudControlStore {
    pub fn register_edge(&mut self, node: EdgeNode) {
        self.edge_nodes.insert(node.edge_id.clone(), node);
    }

    pub fn edge_nodes(&self) -> impl Iterator<Item = &EdgeNode> {
        self.edge_nodes.values()
    }

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

    pub fn config_packages(&self) -> impl Iterator<Item = &EdgeConfigPackage> {
        self.config_packages.values()
    }

    pub fn latest_config_package_for_edge(&self, edge_id: &str) -> Option<&EdgeConfigPackage> {
        self.config_packages()
            .filter(|package| package.edge_id == edge_id)
            .max_by(|left, right| left.version.cmp(&right.version))
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

    pub fn releases(&self) -> impl Iterator<Item = &ReleaseRecord> {
        self.releases.values()
    }

    pub fn push_audit(&mut self, action: AuditAction, target: impl Into<String>) {
        self.audit_records.push(AuditRecord::system(action, target));
    }

    pub fn audit_records(&self) -> &[AuditRecord] {
        &self.audit_records
    }

    pub fn upsert_runtime_metrics(&mut self, snapshot: EdgeRuntimeMetricsSnapshot) {
        self.runtime_metrics
            .insert(snapshot.edge_id.clone(), snapshot);
    }

    pub fn runtime_metrics(&self, edge_id: &str) -> Option<&EdgeRuntimeMetricsSnapshot> {
        self.runtime_metrics.get(edge_id)
    }

    pub fn runtime_metrics_snapshots(&self) -> impl Iterator<Item = &EdgeRuntimeMetricsSnapshot> {
        self.runtime_metrics.values()
    }

    pub fn push_runtime_event(&mut self, event: EdgeRuntimeEvent) {
        self.runtime_events.push(event);
    }

    pub fn runtime_events(&self) -> &[EdgeRuntimeEvent] {
        &self.runtime_events
    }
}
