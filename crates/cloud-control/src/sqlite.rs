use std::str::FromStr;

use anyhow::{Context, Result};
use edge_core::{
    DeviceSpec, DiscoveryReport, EdgeConfigPackage, EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot,
    MqttUplinkConfig, PointMappingSuggestion,
};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{sqlite::SqliteConnectOptions, sqlite::SqlitePoolOptions, Row, SqlitePool};
use uuid::Uuid;

use crate::{AuditAction, AuditRecord, EdgeNode, ReleaseRecord, ReleaseStatus};

#[derive(Clone, Debug)]
pub struct SqliteCloudStore {
    pool: SqlitePool,
}

impl SqliteCloudStore {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(database_url)
            .with_context(|| format!("invalid sqlite database url: {database_url}"))?
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .context("connect sqlite cloud store")?;
        let store = Self { pool };
        store.migrate().await?;
        Ok(store)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    async fn migrate(&self) -> Result<()> {
        for statement in [
            r#"
            CREATE TABLE IF NOT EXISTS edge_nodes (
                edge_id TEXT PRIMARY KEY NOT NULL,
                node_json TEXT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS device_models (
                device_type TEXT PRIMARY KEY NOT NULL,
                model_json TEXT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS config_packages (
                edge_id TEXT NOT NULL,
                version TEXT NOT NULL,
                package_json TEXT NOT NULL,
                PRIMARY KEY (edge_id, version)
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS releases (
                release_id TEXT PRIMARY KEY NOT NULL,
                edge_id TEXT NOT NULL,
                desired_version TEXT NOT NULL,
                reported_version TEXT,
                status TEXT NOT NULL,
                release_json TEXT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS audit_records (
                audit_id TEXT PRIMARY KEY NOT NULL,
                action TEXT NOT NULL,
                target TEXT NOT NULL,
                record_json TEXT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS runtime_metrics (
                edge_id TEXT PRIMARY KEY NOT NULL,
                snapshot_json TEXT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS runtime_events (
                event_id TEXT PRIMARY KEY NOT NULL,
                edge_id TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                event_json TEXT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS mqtt_uplinks (
                edge_id TEXT PRIMARY KEY NOT NULL,
                uplink_json TEXT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS discovery_reports (
                report_id TEXT PRIMARY KEY NOT NULL,
                edge_id TEXT NOT NULL,
                report_json TEXT NOT NULL
            )
            "#,
        ] {
            sqlx::query(statement)
                .execute(&self.pool)
                .await
                .context("migrate sqlite cloud store")?;
        }

        Ok(())
    }

    pub async fn upsert_mqtt_uplink(&self, edge_id: &str, uplink: MqttUplinkConfig) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO mqtt_uplinks (edge_id, uplink_json)
            VALUES (?1, ?2)
            ON CONFLICT(edge_id) DO UPDATE SET uplink_json = excluded.uplink_json
            "#,
        )
        .bind(edge_id)
        .bind(encode(&uplink)?)
        .execute(&self.pool)
        .await
        .context("upsert mqtt uplink")?;
        Ok(())
    }

    pub async fn mqtt_uplink(&self, edge_id: &str) -> Result<Option<MqttUplinkConfig>> {
        let row = sqlx::query("SELECT uplink_json FROM mqtt_uplinks WHERE edge_id = ?1")
            .bind(edge_id)
            .fetch_optional(&self.pool)
            .await
            .context("get mqtt uplink")?;
        row.map(|row| decode_column(row, "uplink_json")).transpose()
    }

    pub async fn mqtt_uplinks(&self) -> Result<Vec<(String, MqttUplinkConfig)>> {
        let rows = sqlx::query("SELECT edge_id, uplink_json FROM mqtt_uplinks ORDER BY edge_id")
            .fetch_all(&self.pool)
            .await
            .context("list mqtt uplinks")?;
        rows.into_iter()
            .map(|row| {
                let edge_id: String = row.try_get("edge_id").context("decode edge_id")?;
                let uplink = decode_column(row, "uplink_json")?;
                Ok((edge_id, uplink))
            })
            .collect()
    }

    pub async fn insert_discovery_report(
        &self,
        edge_id: &str,
        report: DiscoveryReport,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO discovery_reports (report_id, edge_id, report_json)
            VALUES (?1, ?2, ?3)
            "#,
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(edge_id)
        .bind(encode(&report)?)
        .execute(&self.pool)
        .await
        .context("insert discovery report")?;
        Ok(())
    }

    pub async fn discovery_reports(&self, edge_id: &str) -> Result<Vec<DiscoveryReport>> {
        let rows = sqlx::query(
            r#"
            SELECT report_json
            FROM discovery_reports
            WHERE edge_id = ?1
            ORDER BY rowid
            "#,
        )
        .bind(edge_id)
        .fetch_all(&self.pool)
        .await
        .context("list discovery reports")?;
        decode_rows(rows, "report_json")
    }

    pub async fn discovery_report_entries(&self) -> Result<Vec<(String, DiscoveryReport)>> {
        let rows = sqlx::query(
            r#"
            SELECT edge_id, report_json
            FROM discovery_reports
            ORDER BY edge_id, rowid
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("list discovery report entries")?;
        rows.into_iter()
            .map(|row| {
                let edge_id: String = row.try_get("edge_id").context("decode edge_id")?;
                let report = decode_column(row, "report_json")?;
                Ok((edge_id, report))
            })
            .collect()
    }

    pub async fn discovery_suggestions(
        &self,
        edge_id: &str,
    ) -> Result<Vec<PointMappingSuggestion>> {
        Ok(self
            .discovery_reports(edge_id)
            .await?
            .into_iter()
            .flat_map(|report| report.suggestions)
            .collect())
    }

    pub async fn upsert_edge_node(&self, node: EdgeNode) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO edge_nodes (edge_id, node_json)
            VALUES (?1, ?2)
            ON CONFLICT(edge_id) DO UPDATE SET node_json = excluded.node_json
            "#,
        )
        .bind(&node.edge_id)
        .bind(encode(&node)?)
        .execute(&self.pool)
        .await
        .context("upsert edge node")?;
        Ok(())
    }

    pub async fn edge_nodes(&self) -> Result<Vec<EdgeNode>> {
        let rows = sqlx::query("SELECT node_json FROM edge_nodes ORDER BY edge_id")
            .fetch_all(&self.pool)
            .await
            .context("list edge nodes")?;
        decode_rows(rows, "node_json")
    }

    pub async fn delete_edge_node(&self, edge_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.context("begin delete edge node")?;
        for statement in [
            "DELETE FROM edge_nodes WHERE edge_id = ?1",
            "DELETE FROM config_packages WHERE edge_id = ?1",
            "DELETE FROM releases WHERE edge_id = ?1",
            "DELETE FROM runtime_metrics WHERE edge_id = ?1",
            "DELETE FROM runtime_events WHERE edge_id = ?1",
            "DELETE FROM mqtt_uplinks WHERE edge_id = ?1",
            "DELETE FROM discovery_reports WHERE edge_id = ?1",
        ] {
            sqlx::query(statement)
                .bind(edge_id)
                .execute(&mut *tx)
                .await
                .context("delete edge node related rows")?;
        }
        tx.commit().await.context("commit delete edge node")?;
        Ok(())
    }

    pub async fn upsert_device_model(&self, model: DeviceSpec) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO device_models (device_type, model_json)
            VALUES (?1, ?2)
            ON CONFLICT(device_type) DO UPDATE SET model_json = excluded.model_json
            "#,
        )
        .bind(&model.device_type)
        .bind(encode(&model)?)
        .execute(&self.pool)
        .await
        .context("upsert device model")?;
        Ok(())
    }

    pub async fn device_model(&self, device_type: &str) -> Result<Option<DeviceSpec>> {
        let row = sqlx::query("SELECT model_json FROM device_models WHERE device_type = ?1")
            .bind(device_type)
            .fetch_optional(&self.pool)
            .await
            .context("get device model")?;
        row.map(|row| decode_column(row, "model_json")).transpose()
    }

    pub async fn device_models(&self) -> Result<Vec<DeviceSpec>> {
        let rows = sqlx::query("SELECT model_json FROM device_models ORDER BY device_type")
            .fetch_all(&self.pool)
            .await
            .context("list device models")?;
        decode_rows(rows, "model_json")
    }

    pub async fn delete_device_model(&self, device_type: &str) -> Result<()> {
        sqlx::query("DELETE FROM device_models WHERE device_type = ?1")
            .bind(device_type)
            .execute(&self.pool)
            .await
            .context("delete device model")?;
        Ok(())
    }

    pub async fn upsert_config_package(&self, package: EdgeConfigPackage) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO config_packages (edge_id, version, package_json)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(edge_id, version) DO UPDATE SET package_json = excluded.package_json
            "#,
        )
        .bind(&package.edge_id)
        .bind(&package.version)
        .bind(encode(&package)?)
        .execute(&self.pool)
        .await
        .context("upsert config package")?;
        Ok(())
    }

    pub async fn config_package(
        &self,
        edge_id: &str,
        version: &str,
    ) -> Result<Option<EdgeConfigPackage>> {
        let row = sqlx::query(
            r#"
            SELECT package_json
            FROM config_packages
            WHERE edge_id = ?1 AND version = ?2
            "#,
        )
        .bind(edge_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await
        .context("get config package")?;
        row.map(|row| decode_config_package_column(row, "package_json"))
            .transpose()
    }

    pub async fn config_packages(&self) -> Result<Vec<EdgeConfigPackage>> {
        let rows = sqlx::query(
            r#"
            SELECT package_json
            FROM config_packages
            ORDER BY edge_id, version
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("list config packages")?;
        rows.into_iter()
            .map(|row| decode_config_package_column(row, "package_json"))
            .collect()
    }

    pub async fn latest_config_package_for_edge(
        &self,
        edge_id: &str,
    ) -> Result<Option<EdgeConfigPackage>> {
        let row = sqlx::query(
            r#"
            SELECT package_json
            FROM config_packages
            WHERE edge_id = ?1
            ORDER BY version DESC
            LIMIT 1
            "#,
        )
        .bind(edge_id)
        .fetch_optional(&self.pool)
        .await
        .context("get latest config package")?;
        row.map(|row| decode_config_package_column(row, "package_json"))
            .transpose()
    }

    pub async fn insert_release(&self, release: ReleaseRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO releases (
                release_id,
                edge_id,
                desired_version,
                reported_version,
                status,
                release_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(release_id) DO UPDATE SET
                edge_id = excluded.edge_id,
                desired_version = excluded.desired_version,
                reported_version = excluded.reported_version,
                status = excluded.status,
                release_json = excluded.release_json
            "#,
        )
        .bind(release.release_id.to_string())
        .bind(&release.edge_id)
        .bind(&release.desired_version)
        .bind(&release.reported_version)
        .bind(release_status_label(release.status))
        .bind(encode(&release)?)
        .execute(&self.pool)
        .await
        .context("insert release")?;
        Ok(())
    }

    pub async fn release(&self, release_id: Uuid) -> Result<Option<ReleaseRecord>> {
        let row = sqlx::query("SELECT release_json FROM releases WHERE release_id = ?1")
            .bind(release_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .context("get release")?;
        row.map(|row| decode_column(row, "release_json"))
            .transpose()
    }

    pub async fn releases(&self) -> Result<Vec<ReleaseRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT release_json
            FROM releases
            ORDER BY edge_id, desired_version, release_id
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("list releases")?;
        decode_rows(rows, "release_json")
    }

    pub async fn mark_release_reported(
        &self,
        release_id: Uuid,
        reported_version: impl Into<String>,
    ) -> Result<Option<ReleaseRecord>> {
        let Some(mut release) = self.release(release_id).await? else {
            return Ok(None);
        };

        let reported_version = reported_version.into();
        release.reported_version = Some(reported_version.clone());
        release.status = if release.desired_version == reported_version {
            ReleaseStatus::Applied
        } else {
            ReleaseStatus::Failed
        };

        sqlx::query(
            r#"
            UPDATE releases
            SET reported_version = ?1,
                status = ?2,
                release_json = ?3
            WHERE release_id = ?4
            "#,
        )
        .bind(&release.reported_version)
        .bind(release_status_label(release.status))
        .bind(encode(&release)?)
        .bind(release_id.to_string())
        .execute(&self.pool)
        .await
        .context("mark release reported")?;
        self.push_audit(AuditAction::ApplyRelease, release_id.to_string())
            .await?;

        Ok(Some(release))
    }

    pub async fn push_audit(&self, action: AuditAction, target: impl Into<String>) -> Result<()> {
        self.push_audit_record(AuditRecord::system(action, target))
            .await
    }

    pub async fn push_audit_record(&self, record: AuditRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO audit_records (audit_id, action, target, record_json)
            VALUES (?1, ?2, ?3, ?4)
            "#,
        )
        .bind(record.audit_id.to_string())
        .bind(format!("{:?}", record.action))
        .bind(&record.target)
        .bind(encode(&record)?)
        .execute(&self.pool)
        .await
        .context("push audit record")?;
        Ok(())
    }

    pub async fn audit_records(&self) -> Result<Vec<AuditRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT record_json
            FROM audit_records
            ORDER BY audit_id
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("list audit records")?;
        decode_rows(rows, "record_json")
    }

    pub async fn upsert_runtime_metrics(&self, snapshot: EdgeRuntimeMetricsSnapshot) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO runtime_metrics (edge_id, snapshot_json)
            VALUES (?1, ?2)
            ON CONFLICT(edge_id) DO UPDATE SET snapshot_json = excluded.snapshot_json
            "#,
        )
        .bind(&snapshot.edge_id)
        .bind(encode(&snapshot)?)
        .execute(&self.pool)
        .await
        .context("upsert runtime metrics")?;
        Ok(())
    }

    pub async fn runtime_metrics(
        &self,
        edge_id: &str,
    ) -> Result<Option<EdgeRuntimeMetricsSnapshot>> {
        let row = sqlx::query("SELECT snapshot_json FROM runtime_metrics WHERE edge_id = ?1")
            .bind(edge_id)
            .fetch_optional(&self.pool)
            .await
            .context("get runtime metrics")?;
        row.map(|row| decode_column(row, "snapshot_json"))
            .transpose()
    }

    pub async fn runtime_metrics_snapshots(&self) -> Result<Vec<EdgeRuntimeMetricsSnapshot>> {
        let rows = sqlx::query("SELECT snapshot_json FROM runtime_metrics ORDER BY edge_id")
            .fetch_all(&self.pool)
            .await
            .context("list runtime metrics")?;
        decode_rows(rows, "snapshot_json")
    }

    pub async fn push_runtime_event(&self, event: EdgeRuntimeEvent) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO runtime_events (event_id, edge_id, timestamp, event_json)
            VALUES (?1, ?2, ?3, ?4)
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&event.edge_id)
        .bind(event.timestamp.to_rfc3339())
        .bind(encode(&event)?)
        .execute(&self.pool)
        .await
        .context("push runtime event")?;
        Ok(())
    }

    pub async fn runtime_events(&self) -> Result<Vec<EdgeRuntimeEvent>> {
        let rows = sqlx::query(
            r#"
            SELECT event_json
            FROM runtime_events
            ORDER BY timestamp, event_id
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("list runtime events")?;
        decode_rows(rows, "event_json")
    }
}

fn encode<T>(value: &T) -> Result<String>
where
    T: Serialize,
{
    serde_json::to_string(value).context("encode sqlite json payload")
}

fn decode_column<T>(row: sqlx::sqlite::SqliteRow, column: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let payload: String = row
        .try_get(column)
        .with_context(|| format!("read sqlite json column: {column}"))?;
    serde_json::from_str(&payload).with_context(|| format!("decode sqlite json column: {column}"))
}

fn decode_config_package_column(
    row: sqlx::sqlite::SqliteRow,
    column: &str,
) -> Result<EdgeConfigPackage> {
    let payload: String = row
        .try_get(column)
        .with_context(|| format!("read sqlite json column: {column}"))?;
    decode_config_package_payload(&payload)
        .with_context(|| format!("decode sqlite json column: {column}"))
}

fn decode_config_package_payload(payload: &str) -> Result<EdgeConfigPackage> {
    let mut value: serde_json::Value =
        serde_json::from_str(payload).context("parse config package json")?;

    if let Some(connections) = value
        .get_mut("protocol_connections")
        .and_then(serde_json::Value::as_array_mut)
    {
        for connection in connections {
            if connection
                .get("protocol")
                .and_then(serde_json::Value::as_str)
                == Some("Mqtt")
            {
                connection["protocol"] = serde_json::Value::String("CustomSerial".to_string());
            }
        }
    }

    serde_json::from_value(value).context("deserialize config package json")
}

fn decode_rows<T>(rows: Vec<sqlx::sqlite::SqliteRow>, column: &str) -> Result<Vec<T>>
where
    T: DeserializeOwned,
{
    rows.into_iter()
        .map(|row| decode_column(row, column))
        .collect()
}

fn release_status_label(status: ReleaseStatus) -> &'static str {
    match status {
        ReleaseStatus::Pending => "pending",
        ReleaseStatus::Applied => "applied",
        ReleaseStatus::Failed => "failed",
    }
}
