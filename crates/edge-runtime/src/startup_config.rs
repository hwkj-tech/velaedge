use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

const SUPPORTED_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeStartupConfig {
    pub edge_id: String,
    pub device_id: String,
    pub runtime_id: String,
    pub storage: PathBuf,
    pub runtime_db: PathBuf,
    pub cloud_api_url: Option<String>,
    pub cloud_gateway_addr: Option<String>,
    pub edgelink_tls_ca: Option<PathBuf>,
    pub edgelink_tls_cert: Option<PathBuf>,
    pub edgelink_tls_key: Option<PathBuf>,
    pub edgelink_tls_server_name: String,
    pub access_token: Option<String>,
    pub access_token_env: Option<String>,
    pub mqtt_uplink: bool,
    pub edgelink_daemon: bool,
    pub edgelink_command_wait_ms: u64,
    pub edgelink_reconnect_ms: u64,
    pub health_listen: SocketAddr,
    pub scheduled_ticks: u32,
    pub scheduler_tick_ms: u64,
    pub allow_simulated: bool,
}

impl Default for RuntimeStartupConfig {
    fn default() -> Self {
        Self {
            edge_id: "edge-dev".to_owned(),
            device_id: "pump-1".to_owned(),
            runtime_id: "runtime-dev".to_owned(),
            storage: PathBuf::from("data/telemetry.jsonl"),
            runtime_db: PathBuf::from("data/edge-runtime.rocksdb"),
            cloud_api_url: None,
            cloud_gateway_addr: None,
            edgelink_tls_ca: None,
            edgelink_tls_cert: None,
            edgelink_tls_key: None,
            edgelink_tls_server_name: "localhost".to_owned(),
            access_token: None,
            access_token_env: None,
            mqtt_uplink: false,
            edgelink_daemon: false,
            edgelink_command_wait_ms: 30_000,
            edgelink_reconnect_ms: 1_000,
            health_listen: "127.0.0.1:19090"
                .parse()
                .expect("default Runtime health address must be valid"),
            scheduled_ticks: 0,
            scheduler_tick_ms: 1_000,
            allow_simulated: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeStartupOverrides {
    pub edge_id: Option<String>,
    pub device_id: Option<String>,
    pub runtime_id: Option<String>,
    pub storage: Option<PathBuf>,
    pub runtime_db: Option<PathBuf>,
    pub cloud_api_url: Option<String>,
    pub cloud_gateway_addr: Option<String>,
    pub edgelink_tls_ca: Option<PathBuf>,
    pub edgelink_tls_cert: Option<PathBuf>,
    pub edgelink_tls_key: Option<PathBuf>,
    pub edgelink_tls_server_name: Option<String>,
    pub access_token: Option<String>,
    pub access_token_env: Option<String>,
    pub mqtt_uplink: Option<bool>,
    pub edgelink_daemon: Option<bool>,
    pub edgelink_command_wait_ms: Option<u64>,
    pub edgelink_reconnect_ms: Option<u64>,
    pub health_listen: Option<SocketAddr>,
    pub scheduled_ticks: Option<u32>,
    pub scheduler_tick_ms: Option<u64>,
    pub allow_simulated: Option<bool>,
}

impl RuntimeStartupConfig {
    pub fn load(config_path: Option<&Path>, overrides: RuntimeStartupOverrides) -> Result<Self> {
        let mut config = Self::default();
        if let Some(path) = config_path {
            let raw = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read Runtime config {}", path.display()))?;
            let file: RuntimeConfigFile = toml::from_str(&raw)
                .with_context(|| format!("failed to parse Runtime config {}", path.display()))?;
            anyhow::ensure!(
                file.schema_version == SUPPORTED_SCHEMA_VERSION,
                "unsupported Runtime config schema_version {}; expected {}",
                file.schema_version,
                SUPPORTED_SCHEMA_VERSION
            );
            config.apply_file(file, path.parent().unwrap_or_else(|| Path::new(".")));
        }

        config.apply_overrides(overrides);
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        ensure_not_blank("runtime.edge_id", &self.edge_id)?;
        ensure_not_blank("runtime.device_id", &self.device_id)?;
        ensure_not_blank("runtime.runtime_id", &self.runtime_id)?;
        anyhow::ensure!(
            !self.storage.as_os_str().is_empty(),
            "runtime.storage_path cannot be empty"
        );
        anyhow::ensure!(
            !self.runtime_db.as_os_str().is_empty(),
            "runtime.database_path cannot be empty"
        );
        anyhow::ensure!(
            self.edgelink_command_wait_ms > 0,
            "cloud.command_wait_ms must be greater than zero"
        );
        anyhow::ensure!(
            self.edgelink_reconnect_ms > 0,
            "cloud.reconnect_ms must be greater than zero"
        );
        anyhow::ensure!(
            self.scheduler_tick_ms > 0,
            "scheduler.tick_ms must be greater than zero"
        );

        if let Some(address) = self.cloud_gateway_addr.as_deref() {
            ensure_not_blank("cloud.gateway_address", address)?;
        }
        if let Some(url) = self.cloud_api_url.as_deref() {
            ensure_not_blank("cloud.legacy_http_api", url)?;
        }
        anyhow::ensure!(
            !(self.cloud_gateway_addr.is_some() && self.cloud_api_url.is_some()),
            "cloud.gateway_address and cloud.legacy_http_api cannot be configured together"
        );
        anyhow::ensure!(
            !self.edgelink_daemon || self.cloud_gateway_addr.is_some(),
            "cloud.daemon requires cloud.gateway_address"
        );
        anyhow::ensure!(
            self.scheduled_ticks == 0 || self.cloud_api_url.is_some(),
            "scheduler.scheduled_ticks is only supported with cloud.legacy_http_api"
        );
        anyhow::ensure!(
            self.cloud_gateway_addr.is_some()
                || self.cloud_api_url.is_some()
                || self.allow_simulated,
            "production Runtime requires cloud.gateway_address or cloud.legacy_http_api; set features.allow_simulated only for explicit local tests"
        );

        let tls_path_count = [
            self.edgelink_tls_ca.as_ref(),
            self.edgelink_tls_cert.as_ref(),
            self.edgelink_tls_key.as_ref(),
        ]
        .into_iter()
        .flatten()
        .count();
        anyhow::ensure!(
            tls_path_count == 0 || tls_path_count == 3,
            "cloud.tls.ca_cert, cloud.tls.client_cert and cloud.tls.client_key must be configured together"
        );
        if tls_path_count == 3 {
            anyhow::ensure!(
                self.cloud_gateway_addr.is_some(),
                "cloud.tls is only supported with cloud.gateway_address"
            );
            ensure_not_blank("cloud.tls.server_name", &self.edgelink_tls_server_name)?;
        }

        if let Some(variable_name) = self.access_token_env.as_deref() {
            ensure_not_blank("cloud.access_token_env", variable_name)?;
        }
        if let Some(token) = self.access_token.as_deref() {
            anyhow::ensure!(!token.trim().is_empty(), "--access-token cannot be empty");
        }
        anyhow::ensure!(
            !(self.access_token.is_some() && self.access_token_env.is_some()),
            "an EdgeLink access token and cloud.access_token_env cannot both be configured"
        );

        Ok(())
    }

    fn apply_file(&mut self, file: RuntimeConfigFile, base_dir: &Path) {
        apply_option(&mut self.edge_id, file.runtime.edge_id);
        apply_option(&mut self.device_id, file.runtime.device_id);
        apply_option(&mut self.runtime_id, file.runtime.runtime_id);
        if let Some(value) = file.runtime.storage_path {
            self.storage = resolve_file_path(base_dir, value);
        }
        if let Some(value) = file.runtime.database_path {
            self.runtime_db = resolve_file_path(base_dir, value);
        }
        apply_option(&mut self.health_listen, file.runtime.health_listen);

        if let Some(value) = file.cloud.gateway_address {
            self.cloud_gateway_addr = Some(value);
        }
        if let Some(value) = file.cloud.legacy_http_api {
            self.cloud_api_url = Some(value);
        }
        if let Some(value) = file.cloud.access_token_env {
            self.access_token_env = Some(value);
        }
        apply_option(&mut self.edgelink_daemon, file.cloud.daemon);
        apply_option(
            &mut self.edgelink_command_wait_ms,
            file.cloud.command_wait_ms,
        );
        apply_option(&mut self.edgelink_reconnect_ms, file.cloud.reconnect_ms);
        if let Some(value) = file.cloud.tls.ca_cert {
            self.edgelink_tls_ca = Some(resolve_file_path(base_dir, value));
        }
        if let Some(value) = file.cloud.tls.client_cert {
            self.edgelink_tls_cert = Some(resolve_file_path(base_dir, value));
        }
        if let Some(value) = file.cloud.tls.client_key {
            self.edgelink_tls_key = Some(resolve_file_path(base_dir, value));
        }
        apply_option(
            &mut self.edgelink_tls_server_name,
            file.cloud.tls.server_name,
        );

        apply_option(&mut self.mqtt_uplink, file.features.mqtt_uplink);
        apply_option(&mut self.allow_simulated, file.features.allow_simulated);
        apply_option(&mut self.scheduled_ticks, file.scheduler.scheduled_ticks);
        apply_option(&mut self.scheduler_tick_ms, file.scheduler.tick_ms);
    }

    fn apply_overrides(&mut self, overrides: RuntimeStartupOverrides) {
        apply_option(&mut self.edge_id, overrides.edge_id);
        apply_option(&mut self.device_id, overrides.device_id);
        apply_option(&mut self.runtime_id, overrides.runtime_id);
        apply_option(&mut self.storage, overrides.storage);
        apply_option(&mut self.runtime_db, overrides.runtime_db);
        if let Some(value) = overrides.cloud_api_url {
            self.cloud_api_url = Some(value);
        }
        if let Some(value) = overrides.cloud_gateway_addr {
            self.cloud_gateway_addr = Some(value);
        }
        if let Some(value) = overrides.edgelink_tls_ca {
            self.edgelink_tls_ca = Some(value);
        }
        if let Some(value) = overrides.edgelink_tls_cert {
            self.edgelink_tls_cert = Some(value);
        }
        if let Some(value) = overrides.edgelink_tls_key {
            self.edgelink_tls_key = Some(value);
        }
        apply_option(
            &mut self.edgelink_tls_server_name,
            overrides.edgelink_tls_server_name,
        );
        if let Some(value) = overrides.access_token {
            self.access_token = Some(value);
            self.access_token_env = None;
        }
        if let Some(value) = overrides.access_token_env {
            self.access_token_env = Some(value);
            self.access_token = None;
        }
        apply_option(&mut self.mqtt_uplink, overrides.mqtt_uplink);
        apply_option(&mut self.edgelink_daemon, overrides.edgelink_daemon);
        apply_option(
            &mut self.edgelink_command_wait_ms,
            overrides.edgelink_command_wait_ms,
        );
        apply_option(
            &mut self.edgelink_reconnect_ms,
            overrides.edgelink_reconnect_ms,
        );
        apply_option(&mut self.health_listen, overrides.health_listen);
        apply_option(&mut self.scheduled_ticks, overrides.scheduled_ticks);
        apply_option(&mut self.scheduler_tick_ms, overrides.scheduler_tick_ms);
        apply_option(&mut self.allow_simulated, overrides.allow_simulated);
    }
}

fn apply_option<T>(target: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *target = value;
    }
}

fn ensure_not_blank(name: &str, value: &str) -> Result<()> {
    anyhow::ensure!(!value.trim().is_empty(), "{name} cannot be empty");
    Ok(())
}

fn resolve_file_path(base_dir: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() || path.as_os_str().is_empty() {
        path
    } else {
        base_dir.join(path)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeConfigFile {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default)]
    runtime: RuntimeFileSection,
    #[serde(default)]
    cloud: CloudFileSection,
    #[serde(default)]
    features: FeaturesFileSection,
    #[serde(default)]
    scheduler: SchedulerFileSection,
}

fn default_schema_version() -> u32 {
    SUPPORTED_SCHEMA_VERSION
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RuntimeFileSection {
    edge_id: Option<String>,
    device_id: Option<String>,
    runtime_id: Option<String>,
    storage_path: Option<PathBuf>,
    database_path: Option<PathBuf>,
    health_listen: Option<SocketAddr>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CloudFileSection {
    gateway_address: Option<String>,
    legacy_http_api: Option<String>,
    access_token_env: Option<String>,
    daemon: Option<bool>,
    command_wait_ms: Option<u64>,
    reconnect_ms: Option<u64>,
    tls: TlsFileSection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct TlsFileSection {
    ca_cert: Option<PathBuf>,
    client_cert: Option<PathBuf>,
    client_key: Option<PathBuf>,
    server_name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct FeaturesFileSection {
    mqtt_uplink: Option<bool>,
    allow_simulated: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SchedulerFileSection {
    scheduled_ticks: Option<u32>,
    tick_ms: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_toml_and_resolves_relative_paths_from_config_directory() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("runtime.toml");
        std::fs::write(
            &config_path,
            r#"
schema_version = 1

[runtime]
edge_id = "edge-factory-01"
device_id = "pump-01"
runtime_id = "runtime-factory-01"
storage_path = "state/telemetry.jsonl"
database_path = "state/runtime.rocksdb"
health_listen = "127.0.0.1:29090"

[cloud]
gateway_address = "cloud.example.com:18080"
access_token_env = "VELAEDGE_EDGE_TOKEN"
daemon = true
command_wait_ms = 45000
reconnect_ms = 2000

[features]
mqtt_uplink = true

[scheduler]
tick_ms = 500
"#,
        )
        .unwrap();

        let config =
            RuntimeStartupConfig::load(Some(&config_path), RuntimeStartupOverrides::default())
                .unwrap();

        assert_eq!(config.edge_id, "edge-factory-01");
        assert_eq!(config.storage, temp.path().join("state/telemetry.jsonl"));
        assert_eq!(config.runtime_db, temp.path().join("state/runtime.rocksdb"));
        assert_eq!(config.health_listen, "127.0.0.1:29090".parse().unwrap());
        assert!(config.edgelink_daemon);
        assert!(config.mqtt_uplink);
        assert_eq!(config.scheduler_tick_ms, 500);
    }

    #[test]
    fn explicit_overrides_take_precedence_over_file_values() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("runtime.toml");
        std::fs::write(
            &config_path,
            r#"
[runtime]
edge_id = "edge-from-file"

[cloud]
gateway_address = "127.0.0.1:18080"
daemon = true

[features]
mqtt_uplink = true
"#,
        )
        .unwrap();

        let config = RuntimeStartupConfig::load(
            Some(&config_path),
            RuntimeStartupOverrides {
                edge_id: Some("edge-from-cli".to_owned()),
                mqtt_uplink: Some(false),
                ..RuntimeStartupOverrides::default()
            },
        )
        .unwrap();

        assert_eq!(config.edge_id, "edge-from-cli");
        assert!(!config.mqtt_uplink);
    }

    #[test]
    fn rejects_partial_mtls_configuration() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("runtime.toml");
        std::fs::write(
            &config_path,
            r#"
[cloud]
gateway_address = "127.0.0.1:18080"

[cloud.tls]
ca_cert = "ca.pem"
"#,
        )
        .unwrap();

        let error =
            RuntimeStartupConfig::load(Some(&config_path), RuntimeStartupOverrides::default())
                .unwrap_err();
        assert!(error.to_string().contains("must be configured together"));
    }

    #[test]
    fn supports_explicit_standalone_simulation_without_a_cloud_target() {
        let config = RuntimeStartupConfig::load(
            None,
            RuntimeStartupOverrides {
                allow_simulated: Some(true),
                ..RuntimeStartupOverrides::default()
            },
        )
        .unwrap();

        assert!(config.allow_simulated);
        assert!(config.cloud_gateway_addr.is_none());
    }
}
