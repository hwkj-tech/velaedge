use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgeNode {
    pub edge_id: String,
    pub display_name: String,
    pub site: Option<String>,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub product_id: Option<String>,
    #[serde(default)]
    pub desired_product_version: Option<String>,
    #[serde(default)]
    pub reported_product_version: Option<String>,
}

impl EdgeNode {
    pub fn new(edge_id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            edge_id: edge_id.into(),
            display_name: display_name.into(),
            site: None,
            capabilities: Vec::new(),
            project_id: None,
            product_id: None,
            desired_product_version: None,
            reported_product_version: None,
        }
    }

    pub fn at_site(mut self, site: impl Into<String>) -> Self {
        self.site = Some(site.into());
        self
    }

    pub fn with_capability(mut self, capability: impl Into<String>) -> Self {
        self.capabilities.push(capability.into());
        self
    }

    pub fn bind_product(
        mut self,
        project_id: impl Into<String>,
        product_id: impl Into<String>,
        desired_version: impl Into<String>,
    ) -> Self {
        self.project_id = Some(project_id.into());
        self.product_id = Some(product_id.into());
        self.desired_product_version = Some(desired_version.into());
        self
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FleetRegistry {
    nodes: BTreeMap<String, EdgeNode>,
}

impl FleetRegistry {
    pub fn register(&mut self, node: EdgeNode) {
        self.nodes.insert(node.edge_id.clone(), node);
    }

    pub fn get(&self, edge_id: &str) -> Option<&EdgeNode> {
        self.nodes.get(edge_id)
    }

    pub fn nodes(&self) -> impl Iterator<Item = &EdgeNode> {
        self.nodes.values()
    }
}
