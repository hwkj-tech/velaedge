use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use edge_core::EdgeConfigPackage;
use rocksdb::{Options, DB};

const DESIRED_CONFIG_PREFIX: &str = "desired-config";
const ACTIVE_CONFIG_PREFIX: &str = "active-config";

pub struct RocksEdgeRuntimeStore {
    db: DB,
}

impl RocksEdgeRuntimeStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut options = Options::default();
        options.create_if_missing(true);
        let db = DB::open(&options, path).context("failed to open RocksDB runtime store")?;
        Ok(Self { db })
    }

    pub fn put_desired_config(&self, package: &EdgeConfigPackage) -> Result<()> {
        if package.edge_id.trim().is_empty() {
            bail!("edge id is required");
        }
        if package.version.trim().is_empty() {
            bail!("config version is required");
        }

        let payload =
            serde_json::to_vec(package).context("failed to encode desired config package")?;
        self.db
            .put(
                desired_config_key(&package.edge_id, &package.version),
                payload,
            )
            .context("failed to persist desired config package")?;
        Ok(())
    }

    pub fn desired_config(
        &self,
        edge_id: &str,
        version: &str,
    ) -> Result<Option<EdgeConfigPackage>> {
        let Some(payload) = self
            .db
            .get(desired_config_key(edge_id, version))
            .context("failed to read desired config package")?
        else {
            return Ok(None);
        };

        serde_json::from_slice(&payload)
            .map(Some)
            .context("failed to decode desired config package")
    }

    pub fn promote_active_config(&self, edge_id: &str, version: &str) -> Result<()> {
        if self.desired_config(edge_id, version)?.is_none() {
            bail!("desired config not found for edge `{edge_id}` version `{version}`");
        }

        self.db
            .put(active_config_key(edge_id), version.as_bytes())
            .context("failed to promote active config")?;
        Ok(())
    }

    pub fn active_config(&self, edge_id: &str) -> Result<Option<EdgeConfigPackage>> {
        let Some(version) = self.active_version(edge_id)? else {
            return Ok(None);
        };
        self.desired_config(edge_id, &version)
    }

    pub fn active_version(&self, edge_id: &str) -> Result<Option<String>> {
        let Some(payload) = self
            .db
            .get(active_config_key(edge_id))
            .context("failed to read active config version")?
        else {
            return Ok(None);
        };

        String::from_utf8(payload)
            .map(Some)
            .map_err(|error| anyhow!(error).context("active config version is not UTF-8"))
    }
}

fn desired_config_key(edge_id: &str, version: &str) -> String {
    format!("{DESIRED_CONFIG_PREFIX}/{edge_id}/{version}")
}

fn active_config_key(edge_id: &str) -> String {
    format!("{ACTIVE_CONFIG_PREFIX}/{edge_id}")
}
