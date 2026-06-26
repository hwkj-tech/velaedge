use std::path::{Path, PathBuf};

use anyhow::Result;
use async_trait::async_trait;
use edge_core::TelemetrySample;
use tokio::io::AsyncWriteExt;

#[async_trait]
pub trait LocalStore: Send + Sync {
    async fn append_sample(&self, sample: &TelemetrySample) -> Result<()>;
}

#[derive(Clone, Debug)]
pub struct JsonlLocalStore {
    path: PathBuf,
}

impl JsonlLocalStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[async_trait]
impl LocalStore for JsonlLocalStore {
    async fn append_sample(&self, sample: &TelemetrySample) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        let mut line = serde_json::to_string(sample)?;
        line.push('\n');
        file.write_all(line.as_bytes()).await?;
        Ok(())
    }
}
