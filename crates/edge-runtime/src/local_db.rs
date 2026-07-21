use std::{path::Path, sync::Mutex};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use edge_core::EdgeConfigPackage;
use rocksdb::{Direction, IteratorMode, Options, WriteBatch, DB};
use serde::{Deserialize, Serialize};

use crate::{AppliedEdgeConfig, MqttPublishMessage};

const DESIRED_CONFIG_PREFIX: &str = "desired-config";
const ACTIVE_CONFIG_PREFIX: &str = "active-config";
const MQTT_OUTBOX_PREFIX: &str = "mqtt-outbox/";
const MQTT_OUTBOX_SEQUENCE_KEY: &str = "mqtt-outbox-sequence";

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct MqttOutboxEntry {
    pub sequence: u64,
    pub enqueued_at: DateTime<Utc>,
    pub attempts: u32,
    pub last_error: Option<String>,
    pub message: MqttPublishMessage,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MqttOutboxStats {
    pub pending_messages: u64,
    pub oldest_message_age_seconds: u64,
}

pub struct RocksEdgeRuntimeStore {
    db: DB,
    outbox_write_lock: Mutex<()>,
}

impl RocksEdgeRuntimeStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut options = Options::default();
        options.create_if_missing(true);
        let db = DB::open(&options, path).context("failed to open RocksDB runtime store")?;
        Ok(Self {
            db,
            outbox_write_lock: Mutex::new(()),
        })
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

    pub fn recover_active_config(&self, edge_id: &str) -> Result<Option<AppliedEdgeConfig>> {
        self.active_config(edge_id)?
            .map(AppliedEdgeConfig::apply)
            .transpose()
            .with_context(|| format!("failed to validate active config for edge `{edge_id}`"))
    }

    pub fn enqueue_mqtt_message(&self, message: MqttPublishMessage) -> Result<u64> {
        let _guard = self
            .outbox_write_lock
            .lock()
            .map_err(|_| anyhow!("mqtt outbox write lock is poisoned"))?;
        let sequence = self.next_mqtt_sequence()?;
        let entry = MqttOutboxEntry {
            sequence,
            enqueued_at: Utc::now(),
            attempts: 0,
            last_error: None,
            message,
        };
        let payload = serde_json::to_vec(&entry).context("failed to encode mqtt outbox entry")?;
        let mut batch = WriteBatch::default();
        batch.put(MQTT_OUTBOX_SEQUENCE_KEY, sequence.to_be_bytes());
        batch.put(mqtt_outbox_key(sequence), payload);
        self.db
            .write(batch)
            .context("failed to persist mqtt outbox entry")?;
        Ok(sequence)
    }

    pub fn pending_mqtt_messages(&self, limit: usize) -> Result<Vec<MqttOutboxEntry>> {
        let mut entries = Vec::new();
        for item in self.db.iterator(IteratorMode::From(
            MQTT_OUTBOX_PREFIX.as_bytes(),
            Direction::Forward,
        )) {
            let (key, payload) = item.context("failed to iterate mqtt outbox")?;
            if !key.starts_with(MQTT_OUTBOX_PREFIX.as_bytes()) {
                break;
            }
            entries.push(
                serde_json::from_slice(&payload).context("failed to decode mqtt outbox entry")?,
            );
            if entries.len() == limit {
                break;
            }
        }
        Ok(entries)
    }

    pub fn acknowledge_mqtt_message(&self, sequence: u64) -> Result<()> {
        self.db
            .delete(mqtt_outbox_key(sequence))
            .context("failed to acknowledge mqtt outbox entry")
    }

    pub fn mark_mqtt_message_failed(&self, sequence: u64, error: &str) -> Result<()> {
        let key = mqtt_outbox_key(sequence);
        let Some(payload) = self
            .db
            .get(&key)
            .context("failed to read failed mqtt outbox entry")?
        else {
            bail!("mqtt outbox entry {sequence} does not exist");
        };
        let mut entry: MqttOutboxEntry =
            serde_json::from_slice(&payload).context("failed to decode mqtt outbox entry")?;
        entry.attempts = entry.attempts.saturating_add(1);
        entry.last_error = Some(error.to_string());
        self.db
            .put(
                key,
                serde_json::to_vec(&entry).context("failed to encode mqtt outbox entry")?,
            )
            .context("failed to update failed mqtt outbox entry")
    }

    pub fn mqtt_outbox_len(&self) -> Result<usize> {
        Ok(self.pending_mqtt_messages(usize::MAX)?.len())
    }

    pub fn mqtt_outbox_stats(&self) -> Result<MqttOutboxStats> {
        let entries = self.pending_mqtt_messages(usize::MAX)?;
        let pending_messages = u64::try_from(entries.len()).unwrap_or(u64::MAX);
        let oldest_message_age_seconds = entries
            .first()
            .map(|entry| {
                Utc::now()
                    .signed_duration_since(entry.enqueued_at)
                    .num_seconds()
                    .max(0) as u64
            })
            .unwrap_or(0);
        Ok(MqttOutboxStats {
            pending_messages,
            oldest_message_age_seconds,
        })
    }

    fn next_mqtt_sequence(&self) -> Result<u64> {
        let current = self
            .db
            .get(MQTT_OUTBOX_SEQUENCE_KEY)
            .context("failed to read mqtt outbox sequence")?
            .map(|value| {
                value
                    .as_slice()
                    .try_into()
                    .map(u64::from_be_bytes)
                    .map_err(|_| anyhow!("mqtt outbox sequence is invalid"))
            })
            .transpose()?
            .unwrap_or(0);
        current
            .checked_add(1)
            .ok_or_else(|| anyhow!("mqtt outbox sequence is exhausted"))
    }
}

fn desired_config_key(edge_id: &str, version: &str) -> String {
    format!("{DESIRED_CONFIG_PREFIX}/{edge_id}/{version}")
}

fn active_config_key(edge_id: &str) -> String {
    format!("{ACTIVE_CONFIG_PREFIX}/{edge_id}")
}

fn mqtt_outbox_key(sequence: u64) -> String {
    format!("{MQTT_OUTBOX_PREFIX}{sequence:020}")
}
