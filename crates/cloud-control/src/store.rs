use std::collections::BTreeMap;

use edge_core::{
    DeviceSpec, DiscoveryReport, EdgeConfigPackage, EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot,
    MqttUplinkConfig, PointMappingSuggestion,
};
use uuid::Uuid;

use crate::{
    AgentConversation, AgentProposal, AuditAction, AuditRecord, EdgeAccessCredential, EdgeNode,
    KnowledgeDocument, PointSet, Product, ProductVersion, Project, ReleaseRecord,
};

#[derive(Clone, Debug, Default)]
pub struct CloudControlStore {
    edge_nodes: BTreeMap<String, EdgeNode>,
    edge_credentials: BTreeMap<Uuid, EdgeAccessCredential>,
    device_models: BTreeMap<String, DeviceSpec>,
    config_packages: BTreeMap<(String, String), EdgeConfigPackage>,
    releases: BTreeMap<Uuid, ReleaseRecord>,
    audit_records: Vec<AuditRecord>,
    runtime_metrics: BTreeMap<String, EdgeRuntimeMetricsSnapshot>,
    runtime_events: Vec<EdgeRuntimeEvent>,
    mqtt_uplinks: BTreeMap<String, MqttUplinkConfig>,
    discovery_reports: BTreeMap<String, Vec<DiscoveryReport>>,
    projects: BTreeMap<String, Project>,
    point_sets: BTreeMap<String, PointSet>,
    products: BTreeMap<String, Product>,
    product_versions: BTreeMap<(String, String), ProductVersion>,
    agent_proposals: BTreeMap<Uuid, AgentProposal>,
    knowledge_documents: BTreeMap<Uuid, KnowledgeDocument>,
    agent_conversations: BTreeMap<Uuid, AgentConversation>,
}

impl CloudControlStore {
    pub fn register_edge(&mut self, node: EdgeNode) {
        self.edge_nodes.insert(node.edge_id.clone(), node);
    }

    pub fn edge_nodes(&self) -> impl Iterator<Item = &EdgeNode> {
        self.edge_nodes.values()
    }

    pub fn remove_edge_node(&mut self, edge_id: &str) -> Option<EdgeNode> {
        let removed = self.edge_nodes.remove(edge_id)?;
        self.config_packages
            .retain(|(package_edge_id, _), _| package_edge_id != edge_id);
        self.releases
            .retain(|_, release| release.edge_id != edge_id);
        self.runtime_metrics.remove(edge_id);
        self.runtime_events.retain(|event| event.edge_id != edge_id);
        self.mqtt_uplinks.remove(edge_id);
        self.discovery_reports.remove(edge_id);
        self.edge_credentials
            .retain(|_, credential| credential.edge_id != edge_id);
        Some(removed)
    }

    pub fn replace_edge_credential(&mut self, credential: EdgeAccessCredential) {
        for existing in self
            .edge_credentials
            .values_mut()
            .filter(|existing| existing.edge_id == credential.edge_id)
        {
            existing.active = false;
        }
        self.upsert_edge_credential(credential);
    }

    pub fn upsert_edge_credential(&mut self, credential: EdgeAccessCredential) {
        self.edge_credentials
            .insert(credential.credential_id, credential);
    }

    pub fn edge_credentials(&self) -> impl Iterator<Item = &EdgeAccessCredential> {
        self.edge_credentials.values()
    }

    pub fn active_edge_credential(&self, edge_id: &str) -> Option<&EdgeAccessCredential> {
        self.edge_credentials
            .values()
            .filter(|credential| credential.edge_id == edge_id && credential.active)
            .max_by_key(|credential| credential.created_at)
    }

    pub fn upsert_device_model(&mut self, model: DeviceSpec) {
        self.device_models.insert(model.device_type.clone(), model);
    }

    pub fn device_model(&self, device_type: &str) -> Option<&DeviceSpec> {
        self.device_models.get(device_type)
    }

    pub fn device_models(&self) -> impl Iterator<Item = &DeviceSpec> {
        self.device_models.values()
    }

    pub fn remove_device_model(&mut self, device_type: &str) -> Option<DeviceSpec> {
        self.device_models.remove(device_type)
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

    pub fn push_audit_record(&mut self, record: AuditRecord) {
        self.audit_records.push(record);
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

    pub fn upsert_mqtt_uplink(&mut self, edge_id: impl Into<String>, uplink: MqttUplinkConfig) {
        self.mqtt_uplinks.insert(edge_id.into(), uplink);
    }

    pub fn mqtt_uplink(&self, edge_id: &str) -> Option<&MqttUplinkConfig> {
        self.mqtt_uplinks.get(edge_id)
    }

    pub fn mqtt_uplinks(&self) -> impl Iterator<Item = (&String, &MqttUplinkConfig)> {
        self.mqtt_uplinks.iter()
    }

    pub fn insert_discovery_report(&mut self, edge_id: impl Into<String>, report: DiscoveryReport) {
        self.discovery_reports
            .entry(edge_id.into())
            .or_default()
            .push(report);
    }

    pub fn discovery_reports(&self, edge_id: &str) -> &[DiscoveryReport] {
        self.discovery_reports
            .get(edge_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn discovery_report_entries(
        &self,
    ) -> impl Iterator<Item = (&String, &Vec<DiscoveryReport>)> {
        self.discovery_reports.iter()
    }

    pub fn discovery_suggestions(&self, edge_id: &str) -> Vec<PointMappingSuggestion> {
        self.discovery_reports(edge_id)
            .iter()
            .flat_map(|report| report.suggestions.clone())
            .collect()
    }

    pub fn upsert_project(&mut self, project: Project) {
        self.projects.insert(project.project_id.clone(), project);
    }

    pub fn project(&self, project_id: &str) -> Option<&Project> {
        self.projects.get(project_id)
    }

    pub fn projects(&self) -> impl Iterator<Item = &Project> {
        self.projects.values()
    }

    pub fn remove_project(&mut self, project_id: &str) -> Option<Project> {
        let removed = self.projects.remove(project_id)?;
        let product_ids = self
            .products
            .values()
            .filter(|product| product.project_id == project_id)
            .map(|product| product.product_id.clone())
            .collect::<Vec<_>>();
        self.point_sets
            .retain(|_, point_set| point_set.project_id != project_id);
        self.knowledge_documents
            .retain(|_, document| document.project_id.as_deref() != Some(project_id));
        self.agent_conversations
            .retain(|_, conversation| conversation.project_id.as_deref() != Some(project_id));
        self.products
            .retain(|_, product| product.project_id != project_id);
        self.product_versions
            .retain(|(product_id, _), _| !product_ids.contains(product_id));
        Some(removed)
    }

    pub fn upsert_point_set(&mut self, point_set: PointSet) {
        self.point_sets
            .insert(point_set.point_set_id.clone(), point_set);
    }

    pub fn point_set(&self, point_set_id: &str) -> Option<&PointSet> {
        self.point_sets.get(point_set_id)
    }

    pub fn point_sets(&self) -> impl Iterator<Item = &PointSet> {
        self.point_sets.values()
    }

    pub fn remove_point_set(&mut self, point_set_id: &str) -> Option<PointSet> {
        self.point_sets.remove(point_set_id)
    }

    pub fn upsert_product(&mut self, product: Product) {
        self.products.insert(product.product_id.clone(), product);
    }

    pub fn product(&self, product_id: &str) -> Option<&Product> {
        self.products.get(product_id)
    }

    pub fn products(&self) -> impl Iterator<Item = &Product> {
        self.products.values()
    }

    pub fn remove_product(&mut self, product_id: &str) -> Option<Product> {
        let removed = self.products.remove(product_id)?;
        self.product_versions
            .retain(|(candidate_id, _), _| candidate_id != product_id);
        Some(removed)
    }

    pub fn upsert_product_version(&mut self, version: ProductVersion) {
        self.product_versions.insert(
            (version.product_id.clone(), version.version.clone()),
            version,
        );
    }

    pub fn product_version(&self, product_id: &str, version: &str) -> Option<&ProductVersion> {
        self.product_versions
            .get(&(product_id.to_string(), version.to_string()))
    }

    pub fn product_versions(&self) -> impl Iterator<Item = &ProductVersion> {
        self.product_versions.values()
    }

    pub fn remove_product_version(
        &mut self,
        product_id: &str,
        version: &str,
    ) -> Option<ProductVersion> {
        self.product_versions
            .remove(&(product_id.to_string(), version.to_string()))
    }

    pub fn upsert_agent_proposal(&mut self, proposal: AgentProposal) {
        self.agent_proposals.insert(proposal.proposal_id, proposal);
    }

    pub fn agent_proposal(&self, proposal_id: Uuid) -> Option<&AgentProposal> {
        self.agent_proposals.get(&proposal_id)
    }

    pub fn agent_proposals(&self) -> impl Iterator<Item = &AgentProposal> {
        self.agent_proposals.values()
    }

    pub fn upsert_knowledge_document(&mut self, document: KnowledgeDocument) {
        self.knowledge_documents
            .insert(document.document_id, document);
    }

    pub fn knowledge_document(&self, document_id: Uuid) -> Option<&KnowledgeDocument> {
        self.knowledge_documents.get(&document_id)
    }

    pub fn knowledge_documents(&self) -> impl Iterator<Item = &KnowledgeDocument> {
        self.knowledge_documents.values()
    }

    pub fn remove_knowledge_document(&mut self, document_id: Uuid) -> Option<KnowledgeDocument> {
        self.knowledge_documents.remove(&document_id)
    }

    pub fn upsert_agent_conversation(&mut self, conversation: AgentConversation) {
        self.agent_conversations
            .insert(conversation.conversation_id, conversation);
    }

    pub fn agent_conversation(&self, conversation_id: Uuid) -> Option<&AgentConversation> {
        self.agent_conversations.get(&conversation_id)
    }

    pub fn agent_conversations(&self) -> impl Iterator<Item = &AgentConversation> {
        self.agent_conversations.values()
    }

    pub fn remove_agent_conversation(
        &mut self,
        conversation_id: Uuid,
    ) -> Option<AgentConversation> {
        self.agent_conversations.remove(&conversation_id)
    }
}
