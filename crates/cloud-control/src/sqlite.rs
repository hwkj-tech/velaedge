use std::str::FromStr;

use anyhow::{Context, Result};
use edge_core::{
    DeviceSpec, DiscoveryReport, EdgeConfigPackage, EdgeRuntimeEvent, EdgeRuntimeMetricsSnapshot,
    MqttUplinkConfig, PointMappingSuggestion,
};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{
    sqlite::SqliteConnectOptions, sqlite::SqlitePoolOptions, Row, Sqlite, SqlitePool, Transaction,
};
use uuid::Uuid;

use crate::{
    AgentConversation, AgentProposal, AuditAction, AuditRecord, EdgeAccessCredential, EdgeNode,
    KnowledgeDocument, PointSet, Product, ProductVersion, Project, ReleaseRecord, ReleaseStatus,
};

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

    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .context("check sqlite cloud store readiness")?;
        Ok(())
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
            CREATE TABLE IF NOT EXISTS edge_access_credentials (
                credential_id TEXT PRIMARY KEY NOT NULL,
                edge_id TEXT NOT NULL,
                active INTEGER NOT NULL,
                credential_json TEXT NOT NULL
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
            r#"
            CREATE TABLE IF NOT EXISTS projects (
                project_id TEXT PRIMARY KEY NOT NULL,
                project_json TEXT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS point_sets (
                point_set_id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT NOT NULL,
                point_set_json TEXT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS products (
                product_id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT NOT NULL,
                latest_version TEXT,
                product_json TEXT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS product_versions (
                product_id TEXT NOT NULL,
                version TEXT NOT NULL,
                status TEXT NOT NULL,
                version_json TEXT NOT NULL,
                PRIMARY KEY (product_id, version)
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS agent_proposals (
                proposal_id TEXT PRIMARY KEY NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                proposal_json TEXT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS knowledge_documents (
                document_id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT,
                enabled INTEGER NOT NULL,
                updated_at TEXT NOT NULL,
                document_json TEXT NOT NULL
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS idx_knowledge_documents_project
            ON knowledge_documents(project_id, enabled, updated_at)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS agent_conversations (
                conversation_id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT,
                operator_id TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                conversation_json TEXT NOT NULL
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS idx_agent_conversations_scope
            ON agent_conversations(operator_id, project_id, updated_at)
            "#,
        ] {
            sqlx::query(statement)
                .execute(&self.pool)
                .await
                .context("migrate sqlite cloud store")?;
        }

        Ok(())
    }

    pub async fn upsert_project(&self, project: Project) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO projects (project_id, project_json)
            VALUES (?1, ?2)
            ON CONFLICT(project_id) DO UPDATE SET project_json = excluded.project_json
            "#,
        )
        .bind(&project.project_id)
        .bind(encode(&project)?)
        .execute(&self.pool)
        .await
        .context("upsert project")?;
        Ok(())
    }

    pub async fn projects(&self) -> Result<Vec<Project>> {
        let rows = sqlx::query("SELECT project_json FROM projects ORDER BY project_id")
            .fetch_all(&self.pool)
            .await
            .context("list projects")?;
        decode_rows(rows, "project_json")
    }

    pub async fn delete_project(&self, project_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.context("begin delete project")?;
        sqlx::query(
            r#"
            DELETE FROM product_versions
            WHERE product_id IN (SELECT product_id FROM products WHERE project_id = ?1)
            "#,
        )
        .bind(project_id)
        .execute(&mut *tx)
        .await
        .context("delete project product versions")?;
        for statement in [
            "DELETE FROM products WHERE project_id = ?1",
            "DELETE FROM point_sets WHERE project_id = ?1",
            "DELETE FROM knowledge_documents WHERE project_id = ?1",
            "DELETE FROM agent_conversations WHERE project_id = ?1",
            "DELETE FROM projects WHERE project_id = ?1",
        ] {
            sqlx::query(statement)
                .bind(project_id)
                .execute(&mut *tx)
                .await
                .context("delete project catalog rows")?;
        }
        tx.commit().await.context("commit delete project")?;
        Ok(())
    }

    pub async fn upsert_point_set(&self, point_set: PointSet) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO point_sets (point_set_id, project_id, point_set_json)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(point_set_id) DO UPDATE SET
                project_id = excluded.project_id,
                point_set_json = excluded.point_set_json
            "#,
        )
        .bind(&point_set.point_set_id)
        .bind(&point_set.project_id)
        .bind(encode(&point_set)?)
        .execute(&self.pool)
        .await
        .context("upsert point set")?;
        Ok(())
    }

    pub async fn point_sets(&self) -> Result<Vec<PointSet>> {
        let rows = sqlx::query("SELECT point_set_json FROM point_sets ORDER BY point_set_id")
            .fetch_all(&self.pool)
            .await
            .context("list point sets")?;
        decode_rows(rows, "point_set_json")
    }

    pub async fn delete_point_set(&self, point_set_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM point_sets WHERE point_set_id = ?1")
            .bind(point_set_id)
            .execute(&self.pool)
            .await
            .context("delete point set")?;
        Ok(())
    }

    pub async fn upsert_product(&self, product: Product) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO products (product_id, project_id, latest_version, product_json)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(product_id) DO UPDATE SET
                project_id = excluded.project_id,
                latest_version = excluded.latest_version,
                product_json = excluded.product_json
            "#,
        )
        .bind(&product.product_id)
        .bind(&product.project_id)
        .bind(&product.latest_version)
        .bind(encode(&product)?)
        .execute(&self.pool)
        .await
        .context("upsert product")?;
        Ok(())
    }

    pub async fn products(&self) -> Result<Vec<Product>> {
        let rows = sqlx::query("SELECT product_json FROM products ORDER BY product_id")
            .fetch_all(&self.pool)
            .await
            .context("list products")?;
        decode_rows(rows, "product_json")
    }

    pub async fn delete_product(&self, product_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.context("begin delete product")?;
        sqlx::query("DELETE FROM product_versions WHERE product_id = ?1")
            .bind(product_id)
            .execute(&mut *tx)
            .await
            .context("delete product versions")?;
        sqlx::query("DELETE FROM products WHERE product_id = ?1")
            .bind(product_id)
            .execute(&mut *tx)
            .await
            .context("delete product")?;
        tx.commit().await.context("commit delete product")?;
        Ok(())
    }

    pub async fn upsert_product_version(&self, version: ProductVersion) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO product_versions (product_id, version, status, version_json)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(product_id, version) DO UPDATE SET
                status = excluded.status,
                version_json = excluded.version_json
            "#,
        )
        .bind(&version.product_id)
        .bind(&version.version)
        .bind(format!("{:?}", version.status).to_lowercase())
        .bind(encode(&version)?)
        .execute(&self.pool)
        .await
        .context("upsert product version")?;
        Ok(())
    }

    pub async fn transition_product_version(
        &self,
        product: Product,
        versions: Vec<ProductVersion>,
        edge_nodes: Vec<EdgeNode>,
        packages: Vec<EdgeConfigPackage>,
        releases: Vec<ReleaseRecord>,
    ) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("begin product version transition")?;
        sqlx::query(
            r#"
            INSERT INTO products (product_id, project_id, latest_version, product_json)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(product_id) DO UPDATE SET
                project_id = excluded.project_id,
                latest_version = excluded.latest_version,
                product_json = excluded.product_json
            "#,
        )
        .bind(&product.product_id)
        .bind(&product.project_id)
        .bind(&product.latest_version)
        .bind(encode(&product)?)
        .execute(&mut *tx)
        .await
        .context("update product latest version")?;

        for version in versions {
            sqlx::query(
                r#"
                INSERT INTO product_versions (product_id, version, status, version_json)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(product_id, version) DO UPDATE SET
                    status = excluded.status,
                    version_json = excluded.version_json
                "#,
            )
            .bind(&version.product_id)
            .bind(&version.version)
            .bind(format!("{:?}", version.status).to_lowercase())
            .bind(encode(&version)?)
            .execute(&mut *tx)
            .await
            .context("update product version status")?;
        }

        for node in edge_nodes {
            sqlx::query(
                r#"
                INSERT INTO edge_nodes (edge_id, node_json)
                VALUES (?1, ?2)
                ON CONFLICT(edge_id) DO UPDATE SET node_json = excluded.node_json
                "#,
            )
            .bind(&node.edge_id)
            .bind(encode(&node)?)
            .execute(&mut *tx)
            .await
            .context("update bound edge desired product version")?;
        }

        for package in packages {
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
            .execute(&mut *tx)
            .await
            .context("materialize product config package for bound edge")?;
        }

        for release in releases {
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
            .execute(&mut *tx)
            .await
            .context("update product rollout release")?;
        }

        tx.commit()
            .await
            .context("commit product version transition")?;
        Ok(())
    }

    pub async fn product_versions(&self) -> Result<Vec<ProductVersion>> {
        let rows =
            sqlx::query("SELECT version_json FROM product_versions ORDER BY product_id, version")
                .fetch_all(&self.pool)
                .await
                .context("list product versions")?;
        decode_rows(rows, "version_json")
    }

    pub async fn delete_product_version(&self, product_id: &str, version: &str) -> Result<()> {
        sqlx::query("DELETE FROM product_versions WHERE product_id = ?1 AND version = ?2")
            .bind(product_id)
            .bind(version)
            .execute(&self.pool)
            .await
            .context("delete product version")?;
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

    pub async fn replace_edge_credential(&self, credential: EdgeAccessCredential) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("begin replace edge credential")?;
        sqlx::query("UPDATE edge_access_credentials SET active = 0 WHERE edge_id = ?1")
            .bind(&credential.edge_id)
            .execute(&mut *tx)
            .await
            .context("revoke prior edge credentials")?;
        sqlx::query(
            r#"
            INSERT INTO edge_access_credentials (
                credential_id,
                edge_id,
                active,
                credential_json
            ) VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(credential_id) DO UPDATE SET
                edge_id = excluded.edge_id,
                active = excluded.active,
                credential_json = excluded.credential_json
            "#,
        )
        .bind(credential.credential_id.to_string())
        .bind(&credential.edge_id)
        .bind(credential.active)
        .bind(encode(&credential)?)
        .execute(&mut *tx)
        .await
        .context("insert edge credential")?;
        tx.commit()
            .await
            .context("commit replace edge credential")?;
        Ok(())
    }

    pub async fn upsert_edge_credential(&self, credential: EdgeAccessCredential) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO edge_access_credentials (
                credential_id,
                edge_id,
                active,
                credential_json
            ) VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(credential_id) DO UPDATE SET
                edge_id = excluded.edge_id,
                active = excluded.active,
                credential_json = excluded.credential_json
            "#,
        )
        .bind(credential.credential_id.to_string())
        .bind(&credential.edge_id)
        .bind(credential.active)
        .bind(encode(&credential)?)
        .execute(&self.pool)
        .await
        .context("upsert edge credential")?;
        Ok(())
    }

    pub async fn edge_credentials(&self) -> Result<Vec<EdgeAccessCredential>> {
        let rows = sqlx::query(
            r#"
            SELECT active, credential_json
            FROM edge_access_credentials
            ORDER BY edge_id, credential_id
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("list edge credentials")?;
        rows.into_iter()
            .map(|row| {
                let active = row
                    .try_get::<bool, _>("active")
                    .context("decode credential active state")?;
                let mut credential: EdgeAccessCredential = decode_column(row, "credential_json")?;
                credential.active = active;
                Ok(credential)
            })
            .collect()
    }

    pub async fn delete_edge_node(&self, edge_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.context("begin delete edge node")?;
        for statement in [
            "DELETE FROM edge_nodes WHERE edge_id = ?1",
            "DELETE FROM edge_access_credentials WHERE edge_id = ?1",
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

    pub async fn upsert_agent_proposal(&self, proposal: AgentProposal) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO agent_proposals (proposal_id, status, created_at, proposal_json)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(proposal_id) DO UPDATE SET
                status = excluded.status,
                created_at = excluded.created_at,
                proposal_json = excluded.proposal_json
            "#,
        )
        .bind(proposal.proposal_id.to_string())
        .bind(format!("{:?}", proposal.status).to_lowercase())
        .bind(proposal.created_at.to_rfc3339())
        .bind(encode(&proposal)?)
        .execute(&self.pool)
        .await
        .context("upsert agent proposal")?;
        Ok(())
    }

    pub async fn upsert_agent_proposal_with_audit(
        &self,
        proposal: AgentProposal,
        audit: AuditRecord,
    ) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("begin agent proposal transition")?;
        sqlx::query(
            r#"
            INSERT INTO agent_proposals (proposal_id, status, created_at, proposal_json)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(proposal_id) DO UPDATE SET
                status = excluded.status,
                created_at = excluded.created_at,
                proposal_json = excluded.proposal_json
            "#,
        )
        .bind(proposal.proposal_id.to_string())
        .bind(format!("{:?}", proposal.status).to_lowercase())
        .bind(proposal.created_at.to_rfc3339())
        .bind(encode(&proposal)?)
        .execute(&mut *tx)
        .await
        .context("persist agent proposal transition")?;
        sqlx::query(
            r#"
            INSERT INTO audit_records (audit_id, action, target, record_json)
            VALUES (?1, ?2, ?3, ?4)
            "#,
        )
        .bind(audit.audit_id.to_string())
        .bind(format!("{:?}", audit.action))
        .bind(&audit.target)
        .bind(encode(&audit)?)
        .execute(&mut *tx)
        .await
        .context("persist agent proposal audit")?;
        tx.commit()
            .await
            .context("commit agent proposal transition")?;
        Ok(())
    }

    pub async fn agent_proposals(&self) -> Result<Vec<AgentProposal>> {
        let rows = sqlx::query(
            r#"
            SELECT proposal_json
            FROM agent_proposals
            ORDER BY created_at DESC, proposal_id
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("list agent proposals")?;
        decode_rows(rows, "proposal_json")
    }

    pub async fn upsert_knowledge_document_with_audit(
        &self,
        document: KnowledgeDocument,
        audit: AuditRecord,
    ) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("begin knowledge transition")?;
        sqlx::query(
            r#"
            INSERT INTO knowledge_documents
                (document_id, project_id, enabled, updated_at, document_json)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(document_id) DO UPDATE SET
                project_id = excluded.project_id,
                enabled = excluded.enabled,
                updated_at = excluded.updated_at,
                document_json = excluded.document_json
            "#,
        )
        .bind(document.document_id.to_string())
        .bind(&document.project_id)
        .bind(document.enabled)
        .bind(document.updated_at.to_rfc3339())
        .bind(encode(&document)?)
        .execute(&mut *tx)
        .await
        .context("persist knowledge document")?;
        insert_audit_in_transaction(&mut tx, &audit).await?;
        tx.commit().await.context("commit knowledge transition")?;
        Ok(())
    }

    pub async fn delete_knowledge_document_with_audit(
        &self,
        document_id: Uuid,
        audit: AuditRecord,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.context("begin knowledge delete")?;
        sqlx::query("DELETE FROM knowledge_documents WHERE document_id = ?1")
            .bind(document_id.to_string())
            .execute(&mut *tx)
            .await
            .context("delete knowledge document")?;
        insert_audit_in_transaction(&mut tx, &audit).await?;
        tx.commit().await.context("commit knowledge delete")?;
        Ok(())
    }

    pub async fn knowledge_documents(&self) -> Result<Vec<KnowledgeDocument>> {
        let rows = sqlx::query(
            r#"
            SELECT document_json
            FROM knowledge_documents
            ORDER BY updated_at DESC, document_id
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("list knowledge documents")?;
        decode_rows(rows, "document_json")
    }

    pub async fn upsert_agent_conversation(&self, conversation: AgentConversation) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO agent_conversations
                (conversation_id, project_id, operator_id, updated_at, conversation_json)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(conversation_id) DO UPDATE SET
                project_id = excluded.project_id,
                operator_id = excluded.operator_id,
                updated_at = excluded.updated_at,
                conversation_json = excluded.conversation_json
            "#,
        )
        .bind(conversation.conversation_id.to_string())
        .bind(&conversation.project_id)
        .bind(&conversation.operator_id)
        .bind(conversation.updated_at.to_rfc3339())
        .bind(encode(&conversation)?)
        .execute(&self.pool)
        .await
        .context("upsert agent conversation")?;
        Ok(())
    }

    pub async fn upsert_agent_conversation_with_audit(
        &self,
        conversation: AgentConversation,
        audit: AuditRecord,
    ) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("begin agent conversation transition")?;
        sqlx::query(
            r#"
            INSERT INTO agent_conversations
                (conversation_id, project_id, operator_id, updated_at, conversation_json)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(conversation_id) DO UPDATE SET
                project_id = excluded.project_id,
                operator_id = excluded.operator_id,
                updated_at = excluded.updated_at,
                conversation_json = excluded.conversation_json
            "#,
        )
        .bind(conversation.conversation_id.to_string())
        .bind(&conversation.project_id)
        .bind(&conversation.operator_id)
        .bind(conversation.updated_at.to_rfc3339())
        .bind(encode(&conversation)?)
        .execute(&mut *tx)
        .await
        .context("persist agent conversation")?;
        insert_audit_in_transaction(&mut tx, &audit).await?;
        tx.commit()
            .await
            .context("commit agent conversation transition")?;
        Ok(())
    }

    pub async fn delete_agent_conversation_with_audit(
        &self,
        conversation_id: Uuid,
        audit: AuditRecord,
    ) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("begin agent conversation delete")?;
        sqlx::query("DELETE FROM agent_conversations WHERE conversation_id = ?1")
            .bind(conversation_id.to_string())
            .execute(&mut *tx)
            .await
            .context("delete agent conversation")?;
        insert_audit_in_transaction(&mut tx, &audit).await?;
        tx.commit()
            .await
            .context("commit agent conversation delete")?;
        Ok(())
    }

    pub async fn agent_conversations(&self) -> Result<Vec<AgentConversation>> {
        let rows = sqlx::query(
            r#"
            SELECT conversation_json
            FROM agent_conversations
            ORDER BY updated_at DESC, conversation_id
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("list agent conversations")?;
        decode_rows(rows, "conversation_json")
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

async fn insert_audit_in_transaction(
    tx: &mut Transaction<'_, Sqlite>,
    audit: &AuditRecord,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO audit_records (audit_id, action, target, record_json)
        VALUES (?1, ?2, ?3, ?4)
        "#,
    )
    .bind(audit.audit_id.to_string())
    .bind(format!("{:?}", audit.action))
    .bind(&audit.target)
    .bind(encode(audit)?)
    .execute(&mut **tx)
    .await
    .context("persist knowledge audit")?;
    Ok(())
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
        ReleaseStatus::Superseded => "superseded",
    }
}
