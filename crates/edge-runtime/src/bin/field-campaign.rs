use std::{collections::BTreeSet, path::PathBuf, time::Duration};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use clap::Parser;
use edge_core::EdgeConfigPackage;
use edge_runtime::{
    configured_data_mqtt_output_routes, field_protocol_name, run_field_endurance_acceptance,
    start_mqtt_field_receipt_session, validate_field_endurance_options,
    validate_mqtt_uplink_runtime_environment, ConfiguredEdgeRuntime, ConfiguredMqttOutputRoute,
    DataConfigSchedule, FieldDeviceIdentity, FieldEnduranceOptions, MqttFieldReceiptOptions,
    NativeBrokerAudit, TokioSerialBusFactory,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Parser)]
#[command(
    name = "field-campaign",
    about = "Run one released package through coordinated Runtime and broker-side field evidence capture"
)]
struct Cli {
    #[arg(long)]
    config: PathBuf,
    #[arg(long)]
    output_dir: PathBuf,
    #[arg(long, default_value_t = 86_400)]
    duration_seconds: u64,
    #[arg(long, default_value_t = 100)]
    scheduler_interval_ms: u64,
    #[arg(long)]
    minimum_cycles: Option<u64>,
    #[arg(long, default_value_t = 0.01)]
    maximum_failure_ratio: f64,
    /// Maximum interval without successful collection or MQTT publish progress.
    #[arg(long, default_value_t = 300)]
    maximum_progress_gap_seconds: u64,
    #[arg(long)]
    require_recovery: bool,
    #[arg(long = "require-changing-point", value_name = "DEVICE_ID/POINT_ID")]
    changing_points: Vec<String>,
    #[arg(long, default_value_t = 30)]
    receipt_startup_timeout_seconds: u64,
    #[arg(long, default_value_t = 60)]
    receipt_post_run_grace_seconds: u64,
    #[arg(long)]
    physical_device_exercised: bool,
    #[arg(long)]
    site_id: Option<String>,
    #[arg(long)]
    operator: Option<String>,
    #[arg(long)]
    device_connection_id: Option<String>,
    #[arg(long)]
    device_manufacturer: Option<String>,
    #[arg(long)]
    device_model: Option<String>,
    #[arg(long)]
    device_serial: Option<String>,
    #[arg(long)]
    rocksdb_path: Option<PathBuf>,
    /// Schema v1 native Broker audit exported by VelaMQ or the target Broker adapter.
    #[arg(long)]
    native_broker_audit: PathBuf,
    /// Time to wait after the Runtime and receipt window close for the Broker audit export.
    #[arg(long, default_value_t = 300)]
    native_broker_audit_wait_seconds: u64,
    /// Validate the released package and local Runtime/MQTT inputs without opening any session.
    #[arg(long)]
    preflight_only: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CampaignArtifact {
    file: String,
    sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FieldCampaignManifest {
    schema_version: u32,
    status: &'static str,
    phase: &'static str,
    edge_id: String,
    config_version: String,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    package: Option<CampaignArtifact>,
    runtime_report: Option<CampaignArtifact>,
    broker_receipt: Option<CampaignArtifact>,
    native_broker_audit: Option<CampaignArtifact>,
    native_broker_audit_required: bool,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FieldCampaignPreflightReport {
    schema_version: u32,
    status: &'static str,
    edge_id: String,
    config_version: String,
    package_sha256: String,
    physical_device: Option<FieldDeviceIdentity>,
    configured_duration_seconds: u64,
    scheduler_interval_ms: u64,
    minimum_cycles: u64,
    maximum_failure_ratio: f64,
    maximum_progress_gap_seconds: u64,
    protocol_connections: Vec<FieldCampaignPreflightProtocol>,
    mqtt_output_routes: Vec<ConfiguredMqttOutputRoute>,
    output_dir: String,
    rocksdb_path: String,
    native_broker_audit: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FieldCampaignPreflightProtocol {
    connection_id: String,
    protocol: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
enum CampaignRunOutcome<T> {
    Completed(T),
    Interrupted(&'static str),
}

#[cfg(unix)]
struct CampaignShutdownMonitor {
    interrupt: tokio::signal::unix::Signal,
    terminate: tokio::signal::unix::Signal,
}

#[cfg(unix)]
impl CampaignShutdownMonitor {
    fn new() -> Result<Self> {
        Ok(Self {
            interrupt: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                .context("install field campaign SIGINT handler")?,
            terminate: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .context("install field campaign SIGTERM handler")?,
        })
    }

    async fn recv(&mut self) -> Result<&'static str> {
        tokio::select! {
            signal = self.interrupt.recv() => {
                signal.context("field campaign SIGINT stream ended unexpectedly")?;
                Ok("SIGINT")
            }
            signal = self.terminate.recv() => {
                signal.context("field campaign SIGTERM stream ended unexpectedly")?;
                Ok("SIGTERM")
            }
        }
    }
}

#[cfg(not(unix))]
struct CampaignShutdownMonitor;

#[cfg(not(unix))]
impl CampaignShutdownMonitor {
    fn new() -> Result<Self> {
        Ok(Self)
    }

    async fn recv(&mut self) -> Result<&'static str> {
        tokio::signal::ctrl_c()
            .await
            .context("listen for field campaign interrupt")?;
        Ok("interrupt")
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    validate_campaign_paths(&cli).await?;
    if cli.preflight_only {
        let report = preflight_campaign(&cli).await?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    ensure_new_evidence_directory(&cli.output_dir).await?;

    let started_at = Utc::now();
    let (package, package_sha256, mut manifest) =
        initialize_campaign(&cli.config, &cli.output_dir, started_at).await?;
    write_manifest(&cli.output_dir, &manifest).await?;
    let mut shutdown = match CampaignShutdownMonitor::new() {
        Ok(shutdown) => shutdown,
        Err(error) => {
            retain_failure(&cli.output_dir, &mut manifest, format!("{error:#}")).await?;
            bail!(
                "field campaign signal setup failed; evidence retained at {}",
                cli.output_dir.display()
            );
        }
    };

    let receipt_options = MqttFieldReceiptOptions::new(package.clone(), package_sha256.clone())
        .with_startup_timeout(Duration::from_secs(cli.receipt_startup_timeout_seconds));
    let mut receipt_session = match start_mqtt_field_receipt_session(receipt_options) {
        Ok(session) => session,
        Err(error) => {
            retain_failure(&cli.output_dir, &mut manifest, error.to_string()).await?;
            bail!(
                "field campaign MQTT receipt setup failed; evidence retained at {}",
                cli.output_dir.display()
            );
        }
    };
    match await_or_shutdown(receipt_session.wait_ready(), shutdown.recv()).await {
        Ok(CampaignRunOutcome::Completed(Ok(()))) => {}
        Ok(CampaignRunOutcome::Completed(Err(error))) => {
            retain_failure(&cli.output_dir, &mut manifest, error.to_string()).await?;
            bail!(
                "field campaign MQTT subscriptions failed; evidence retained at {}",
                cli.output_dir.display()
            );
        }
        Ok(CampaignRunOutcome::Interrupted(signal)) => {
            manifest.phase = "interrupted";
            retain_failure(
                &cli.output_dir,
                &mut manifest,
                format!("field campaign interrupted by {signal} during MQTT subscription setup"),
            )
            .await?;
            bail!(
                "field campaign interrupted; evidence retained at {}",
                cli.output_dir.display()
            );
        }
        Err(error) => {
            retain_failure(
                &cli.output_dir,
                &mut manifest,
                format!("field campaign shutdown monitor failed: {error:#}"),
            )
            .await?;
            bail!(
                "field campaign shutdown monitor failed; evidence retained at {}",
                cli.output_dir.display()
            );
        }
    }
    tracing::info!(
        edge_id = package.edge_id,
        config_version = package.version,
        "all field campaign MQTT subscriptions are ready; starting Runtime"
    );

    manifest.phase = "runtime_endurance";
    checkpoint_manifest(&cli.output_dir, &mut manifest).await?;
    let endurance_options = build_endurance_options(&cli, package, package_sha256.clone());
    let (report_result, mut interrupted_by) = match await_or_shutdown(
        run_field_endurance_acceptance(endurance_options),
        shutdown.recv(),
    )
    .await
    {
        Ok(CampaignRunOutcome::Completed(report_result)) => (report_result, None),
        Ok(CampaignRunOutcome::Interrupted(signal)) => (
            Err(anyhow::anyhow!("field campaign interrupted by {signal}")),
            Some(signal),
        ),
        Err(error) => (
            Err(anyhow::anyhow!(
                "field campaign shutdown monitor failed: {error:#}"
            )),
            Some("shutdown-monitor-error"),
        ),
    };
    let mut lifecycle_error = None;
    if should_wait_for_receipt_grace(
        report_result
            .as_ref()
            .ok()
            .map(|report| report.mqtt.publish_success_count),
        cli.receipt_post_run_grace_seconds,
    ) {
        match await_or_shutdown(
            tokio::time::sleep(Duration::from_secs(cli.receipt_post_run_grace_seconds)),
            shutdown.recv(),
        )
        .await
        {
            Ok(CampaignRunOutcome::Completed(())) => {}
            Ok(CampaignRunOutcome::Interrupted(signal)) => {
                interrupted_by = Some(signal);
                lifecycle_error = Some(format!(
                    "field campaign interrupted by {signal} during MQTT receipt drain"
                ));
            }
            Err(error) => {
                interrupted_by = Some("shutdown-monitor-error");
                lifecycle_error = Some(format!(
                    "field campaign shutdown monitor failed during MQTT receipt drain: {error:#}"
                ));
            }
        }
    }
    let receipt_result = receipt_session.finish().await;

    let report = match report_result {
        Ok(report) => {
            manifest.runtime_report =
                Some(write_json_artifact(&cli.output_dir, "runtime-report.json", &report).await?);
            if !report.passed() {
                manifest
                    .errors
                    .push("Runtime field endurance criteria did not pass".to_string());
            }
            Some(report)
        }
        Err(error) => {
            manifest
                .errors
                .push(format!("Runtime field endurance failed: {error:#}"));
            None
        }
    };
    if let Some(error) = lifecycle_error {
        manifest.errors.push(error);
    }
    let receipt = match receipt_result {
        Ok(receipt) => {
            manifest.broker_receipt =
                Some(write_json_artifact(&cli.output_dir, "broker-receipt.json", &receipt).await?);
            Some(receipt)
        }
        Err(error) => {
            manifest
                .errors
                .push(format!("broker receipt capture failed: {error:#}"));
            None
        }
    };
    manifest.phase = "native_broker_audit";
    checkpoint_manifest(&cli.output_dir, &mut manifest).await?;
    let native_broker_audit = if let Some(signal) = interrupted_by {
        manifest.errors.push(format!(
            "native broker audit was not awaited because the campaign stopped on {signal}"
        ));
        false
    } else if let Some(receipt) = receipt.as_ref() {
        match await_or_shutdown(
            wait_for_native_broker_audit(
                &cli.native_broker_audit,
                Duration::from_secs(cli.native_broker_audit_wait_seconds),
            ),
            shutdown.recv(),
        )
        .await
        {
            Ok(CampaignRunOutcome::Completed(Ok(()))) => {
                match retain_native_broker_audit(&cli.native_broker_audit, &cli.output_dir).await {
                    Ok((artifact, audit)) => {
                        manifest.native_broker_audit = Some(artifact);
                        match audit.validate_against(receipt) {
                            Ok(()) => true,
                            Err(error) => {
                                manifest.errors.push(format!(
                                    "native broker audit does not match this campaign: {error:#}"
                                ));
                                false
                            }
                        }
                    }
                    Err(error) => {
                        manifest
                            .errors
                            .push(format!("native broker audit capture failed: {error:#}"));
                        false
                    }
                }
            }
            Ok(CampaignRunOutcome::Completed(Err(error))) => {
                manifest
                    .errors
                    .push(format!("native broker audit capture failed: {error:#}"));
                false
            }
            Ok(CampaignRunOutcome::Interrupted(signal)) => {
                interrupted_by = Some(signal);
                manifest.errors.push(format!(
                    "field campaign interrupted by {signal} while waiting for native broker audit"
                ));
                false
            }
            Err(error) => {
                interrupted_by = Some("shutdown-monitor-error");
                manifest.errors.push(format!(
                    "field campaign shutdown monitor failed while waiting for native broker audit: {error:#}"
                ));
                false
            }
        }
    } else {
        manifest.errors.push(
            "native broker audit was not awaited because no valid broker receipt was captured"
                .to_string(),
        );
        false
    };

    manifest.phase = "artifact_binding";
    if let (Some(report), Some(receipt)) = (&report, &receipt) {
        validate_artifact_binding(report, receipt, &package_sha256, &mut manifest.errors);
    }
    if interrupted_by.is_some() {
        manifest.status = "failed";
        manifest.phase = "interrupted";
    } else if manifest.errors.is_empty()
        && report.is_some()
        && receipt.is_some()
        && native_broker_audit
    {
        manifest.status = "passed";
        manifest.phase = "complete";
    } else {
        manifest.status = "failed";
    }
    manifest.finished_at = Utc::now();
    write_manifest(&cli.output_dir, &manifest).await?;
    println!("{}", serde_json::to_string_pretty(&manifest)?);

    if manifest.status != "passed" {
        bail!(
            "field campaign failed; evidence retained at {}",
            cli.output_dir.display()
        );
    }
    eprintln!(
        "field campaign evidence retained at {}",
        cli.output_dir.display()
    );
    Ok(())
}

async fn preflight_campaign(cli: &Cli) -> Result<FieldCampaignPreflightReport> {
    let package_bytes = tokio::fs::read(&cli.config)
        .await
        .with_context(|| format!("read configuration package {}", cli.config.display()))?;
    let package = serde_json::from_slice::<EdgeConfigPackage>(&package_bytes)
        .with_context(|| format!("decode configuration package {}", cli.config.display()))?;
    let package_sha256 = sha256(&package_bytes);
    let endurance = build_endurance_options(cli, package.clone(), package_sha256.clone());

    validate_field_endurance_options(&endurance)?;
    ConfiguredEdgeRuntime::new(package.clone(), TokioSerialBusFactory)
        .context("validate production Runtime configuration")?;
    DataConfigSchedule::from_package(&package).context("validate collection schedule")?;

    let mqtt_output_routes = configured_data_mqtt_output_routes(&package)
        .context("validate configured MQTT output routes")?;
    if mqtt_output_routes.is_empty() {
        bail!("field campaign requires at least one enabled MQTT output route");
    }
    let used_sink_ids = mqtt_output_routes
        .iter()
        .map(|route| route.sink_id.as_str())
        .collect::<BTreeSet<_>>();
    for uplink in package
        .mqtt_uplinks
        .iter()
        .filter(|uplink| used_sink_ids.contains(uplink.sink_id.as_str()))
    {
        validate_mqtt_uplink_runtime_environment(uplink)
            .with_context(|| format!("validate MQTT sink {} environment", uplink.sink_id))?;
    }

    let used_connection_ids = package
        .data_configs
        .iter()
        .filter(|config| config.enabled)
        .map(|config| config.protocol_connection_id.as_str())
        .collect::<BTreeSet<_>>();
    let protocol_connections = package
        .protocol_connections
        .iter()
        .filter(|connection| used_connection_ids.contains(connection.connection_id.as_str()))
        .map(|connection| FieldCampaignPreflightProtocol {
            connection_id: connection.connection_id.clone(),
            protocol: field_protocol_name(connection.protocol),
        })
        .collect();

    Ok(FieldCampaignPreflightReport {
        schema_version: 1,
        status: "passed",
        edge_id: package.edge_id,
        config_version: package.version,
        package_sha256,
        physical_device: endurance.physical_device,
        configured_duration_seconds: cli.duration_seconds,
        scheduler_interval_ms: cli.scheduler_interval_ms,
        minimum_cycles: endurance.minimum_cycles,
        maximum_failure_ratio: cli.maximum_failure_ratio,
        maximum_progress_gap_seconds: cli.maximum_progress_gap_seconds,
        protocol_connections,
        mqtt_output_routes,
        output_dir: cli.output_dir.display().to_string(),
        rocksdb_path: endurance.rocksdb_path.display().to_string(),
        native_broker_audit: cli.native_broker_audit.display().to_string(),
    })
}

async fn validate_campaign_paths(cli: &Cli) -> Result<()> {
    if cli.physical_device_exercised {
        for (label, path) in [
            ("configuration package", &cli.config),
            ("evidence directory", &cli.output_dir),
            ("native broker audit", &cli.native_broker_audit),
        ] {
            if !path.is_absolute() {
                bail!("physical field campaign {label} path must be absolute");
            }
        }
        if let Some(path) = cli.rocksdb_path.as_ref() {
            if !path.is_absolute() {
                bail!("physical field campaign RocksDB path must be absolute");
            }
        }
    }
    ensure_evidence_directory_available(&cli.output_dir).await?;
    if cli.native_broker_audit.starts_with(&cli.output_dir) {
        bail!("native broker audit source must be outside the campaign evidence directory");
    }
    if tokio::fs::try_exists(&cli.native_broker_audit).await? {
        bail!(
            "native broker audit source must not exist before the campaign starts: {}",
            cli.native_broker_audit.display()
        );
    }
    Ok(())
}

async fn initialize_campaign(
    config_path: &std::path::Path,
    output_dir: &std::path::Path,
    started_at: DateTime<Utc>,
) -> Result<(EdgeConfigPackage, String, FieldCampaignManifest)> {
    let package_bytes = match tokio::fs::read(config_path)
        .await
        .with_context(|| format!("read configuration package {}", config_path.display()))
    {
        Ok(bytes) => bytes,
        Err(error) => {
            let mut manifest = new_manifest(started_at, "configuration_read", None);
            retain_failure(output_dir, &mut manifest, format!("{error:#}")).await?;
            bail!(
                "field campaign configuration read failed; evidence retained at {}",
                output_dir.display()
            );
        }
    };
    let package_artifact =
        write_bytes_artifact(output_dir, "configuration-package.json", &package_bytes).await?;
    let package = match serde_json::from_slice::<EdgeConfigPackage>(&package_bytes)
        .with_context(|| format!("decode configuration package {}", config_path.display()))
    {
        Ok(package) => package,
        Err(error) => {
            let mut manifest =
                new_manifest(started_at, "configuration_decode", Some(package_artifact));
            retain_failure(output_dir, &mut manifest, format!("{error:#}")).await?;
            bail!(
                "field campaign configuration decode failed; evidence retained at {}",
                output_dir.display()
            );
        }
    };
    let package_sha256 = sha256(&package_bytes);
    let mut manifest = new_manifest(started_at, "mqtt_subscribe", Some(package_artifact));
    manifest.edge_id.clone_from(&package.edge_id);
    manifest.config_version.clone_from(&package.version);
    Ok((package, package_sha256, manifest))
}

fn new_manifest(
    started_at: DateTime<Utc>,
    phase: &'static str,
    package: Option<CampaignArtifact>,
) -> FieldCampaignManifest {
    FieldCampaignManifest {
        schema_version: 3,
        status: "running",
        phase,
        edge_id: String::new(),
        config_version: String::new(),
        started_at,
        finished_at: started_at,
        package,
        runtime_report: None,
        broker_receipt: None,
        native_broker_audit: None,
        native_broker_audit_required: true,
        errors: Vec::new(),
    }
}

fn build_endurance_options(
    cli: &Cli,
    package: EdgeConfigPackage,
    package_sha256: String,
) -> FieldEnduranceOptions {
    let duration = Duration::from_secs(cli.duration_seconds);
    let minimum_period_ms = package
        .data_configs
        .iter()
        .filter(|config| config.enabled)
        .map(|config| config.collection.period_ms)
        .min()
        .unwrap_or(1_000);
    let derived_minimum_cycles = duration
        .as_millis()
        .div_ceil(u128::from(minimum_period_ms.max(1)))
        .saturating_mul(9)
        / 10;
    let physical_device = cli.physical_device_exercised.then(|| FieldDeviceIdentity {
        site_id: cli.site_id.clone().unwrap_or_default(),
        operator: cli.operator.clone().unwrap_or_default(),
        connection_id: cli.device_connection_id.clone().unwrap_or_default(),
        manufacturer: cli.device_manufacturer.clone().unwrap_or_default(),
        model: cli.device_model.clone().unwrap_or_default(),
        serial_number: cli.device_serial.clone().unwrap_or_default(),
    });
    FieldEnduranceOptions {
        package,
        package_sha256: Some(package_sha256),
        duration,
        scheduler_interval: Duration::from_millis(cli.scheduler_interval_ms),
        minimum_cycles: cli
            .minimum_cycles
            .unwrap_or_else(|| u64::try_from(derived_minimum_cycles.max(1)).unwrap_or(u64::MAX)),
        maximum_failure_ratio: cli.maximum_failure_ratio,
        maximum_progress_gap: Duration::from_secs(cli.maximum_progress_gap_seconds),
        require_recovery: cli.require_recovery,
        changing_points: cli.changing_points.iter().cloned().collect::<BTreeSet<_>>(),
        exercise_mqtt: true,
        physical_device_exercised: cli.physical_device_exercised,
        physical_device,
        rocksdb_path: cli
            .rocksdb_path
            .clone()
            .unwrap_or_else(|| cli.output_dir.join("runtime.rocksdb")),
    }
}

fn should_wait_for_receipt_grace(publish_success_count: Option<u64>, grace_seconds: u64) -> bool {
    grace_seconds > 0 && publish_success_count.is_some_and(|count| count > 0)
}

async fn await_or_shutdown<T>(
    operation: impl std::future::Future<Output = T>,
    shutdown: impl std::future::Future<Output = Result<&'static str>>,
) -> Result<CampaignRunOutcome<T>> {
    tokio::select! {
        result = operation => Ok(CampaignRunOutcome::Completed(result)),
        signal = shutdown => Ok(CampaignRunOutcome::Interrupted(signal?)),
    }
}

fn validate_artifact_binding(
    report: &edge_runtime::FieldEnduranceReport,
    receipt: &edge_runtime::BrokerConsumerReceipt,
    package_sha256: &str,
    errors: &mut Vec<String>,
) {
    if report.edge_id != receipt.edge_id || report.config_version != receipt.config_version {
        errors.push("Runtime report and broker receipt edge/version do not match".to_string());
    }
    if report.package_sha256.as_deref() != Some(package_sha256)
        || receipt.package_sha256 != package_sha256
    {
        errors.push("Runtime report or broker receipt package digest does not match".to_string());
    }
    if report.mqtt.publish_success_count != receipt.message_count {
        errors.push(format!(
            "broker received {} messages but Runtime recorded {} successful publishes",
            receipt.message_count, report.mqtt.publish_success_count
        ));
    }
}

async fn ensure_new_evidence_directory(path: &std::path::Path) -> Result<()> {
    ensure_evidence_directory_available(path).await?;
    if !tokio::fs::try_exists(path).await? {
        tokio::fs::create_dir_all(path)
            .await
            .with_context(|| format!("create evidence directory {}", path.display()))?;
    }
    Ok(())
}

async fn ensure_evidence_directory_available(path: &std::path::Path) -> Result<()> {
    if tokio::fs::try_exists(path).await? {
        let mut entries = tokio::fs::read_dir(path)
            .await
            .with_context(|| format!("read evidence directory {}", path.display()))?;
        if entries.next_entry().await?.is_some() {
            bail!(
                "field campaign evidence directory must be empty: {}",
                path.display()
            );
        }
    }
    Ok(())
}

async fn retain_failure(
    output_dir: &std::path::Path,
    manifest: &mut FieldCampaignManifest,
    error: String,
) -> Result<()> {
    manifest.status = "failed";
    manifest.errors.push(error);
    manifest.finished_at = Utc::now();
    write_manifest(output_dir, manifest).await
}

async fn checkpoint_manifest(
    output_dir: &std::path::Path,
    manifest: &mut FieldCampaignManifest,
) -> Result<()> {
    manifest.finished_at = Utc::now();
    write_manifest(output_dir, manifest).await
}

async fn wait_for_native_broker_audit(path: &std::path::Path, wait: Duration) -> Result<()> {
    let wait_for_file = async {
        loop {
            match tokio::fs::metadata(path).await {
                Ok(metadata) if metadata.is_file() && metadata.len() > 0 => return Ok(()),
                Ok(metadata) if !metadata.is_file() => {
                    bail!("native broker audit path is not a file: {}", path.display())
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("inspect native broker audit {}", path.display()))
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };

    tokio::time::timeout(wait, wait_for_file)
        .await
        .with_context(|| {
            format!(
                "native broker audit did not arrive within {} seconds: {}",
                wait.as_secs(),
                path.display()
            )
        })?
}

async fn retain_native_broker_audit(
    source: &std::path::Path,
    output_dir: &std::path::Path,
) -> Result<(CampaignArtifact, NativeBrokerAudit)> {
    let bytes = tokio::fs::read(source)
        .await
        .with_context(|| format!("read native broker audit {}", source.display()))?;
    let audit = NativeBrokerAudit::from_json_slice(&bytes)
        .with_context(|| format!("validate native broker audit {}", source.display()))?;
    let artifact = write_bytes_artifact(output_dir, "native-broker-audit.json", &bytes).await?;
    Ok((artifact, audit))
}

async fn write_manifest(
    output_dir: &std::path::Path,
    manifest: &FieldCampaignManifest,
) -> Result<()> {
    write_json_artifact(output_dir, "manifest.json", manifest)
        .await
        .map(|_| ())
}

async fn write_json_artifact(
    output_dir: &std::path::Path,
    file: &str,
    value: &impl Serialize,
) -> Result<CampaignArtifact> {
    let bytes = serde_json::to_vec_pretty(value)?;
    write_bytes_artifact(output_dir, file, &bytes).await
}

async fn write_bytes_artifact(
    output_dir: &std::path::Path,
    file: &str,
    bytes: &[u8],
) -> Result<CampaignArtifact> {
    let path = output_dir.join(file);
    let temporary = output_dir.join(format!(".{file}.{}.tmp", uuid::Uuid::new_v4().simple()));
    tokio::fs::write(&temporary, bytes)
        .await
        .with_context(|| format!("write temporary field artifact {}", temporary.display()))?;
    if let Err(error) = tokio::fs::rename(&temporary, &path).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error).with_context(|| {
            format!(
                "publish field artifact {} from {}",
                path.display(),
                temporary.display()
            )
        });
    }
    Ok(CampaignArtifact {
        file: file.to_string(),
        sha256: sha256(bytes),
    })
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn evidence_directory_must_be_new_or_empty() {
        let temporary = tempfile::tempdir().unwrap();
        let evidence = temporary.path().join("campaign");

        ensure_new_evidence_directory(&evidence).await.unwrap();
        tokio::fs::write(evidence.join("existing.json"), b"{}")
            .await
            .unwrap();

        let error = ensure_new_evidence_directory(&evidence).await.unwrap_err();
        assert!(error.to_string().contains("must be empty"));
    }

    #[tokio::test]
    async fn artifact_digest_covers_the_exact_retained_bytes() {
        let temporary = tempfile::tempdir().unwrap();
        let bytes = br#"{"edgeId":"edge-a","version":"v1"}"#;

        let artifact = write_bytes_artifact(temporary.path(), "package.json", bytes)
            .await
            .unwrap();

        assert_eq!(artifact.sha256, sha256(bytes));
        assert_eq!(
            tokio::fs::read(temporary.path().join("package.json"))
                .await
                .unwrap(),
            bytes
        );
    }

    #[tokio::test]
    async fn native_broker_audit_is_required_and_hash_bound() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("velamq-audit.json");
        let output = temporary.path().join("campaign");
        tokio::fs::create_dir(&output).await.unwrap();
        let bytes = br#"{
          "schemaVersion": 1,
          "broker": "VelaMQ",
          "brokerInstanceId": "velamq-node-a",
          "auditId": "audit-20260718-a",
          "exportedAt": "2026-07-19T00:00:01Z",
          "edgeId": "edge-a",
          "configVersion": "v1",
          "packageSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "firstObservedAt": "2026-07-18T00:00:00Z",
          "lastObservedAt": "2026-07-19T00:00:00Z",
          "messageCount": 1,
          "routes": [{
            "broker": "mqtts://velamq.example:8883",
            "consumerId": "field-audit-consumer",
            "messageCount": 1,
            "topics": ["field/edge-a/telemetry"]
          }]
        }"#;
        tokio::fs::write(&source, bytes).await.unwrap();

        let (artifact, audit) = retain_native_broker_audit(&source, &output).await.unwrap();

        assert_eq!(artifact.file, "native-broker-audit.json");
        assert_eq!(artifact.sha256, sha256(bytes));
        assert_eq!(audit.audit_id, "audit-20260718-a");
        assert_eq!(
            tokio::fs::read(output.join("native-broker-audit.json"))
                .await
                .unwrap(),
            bytes
        );

        tokio::fs::write(&source, b"").await.unwrap();
        let error = retain_native_broker_audit(&source, &output)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("validate native broker audit"));
    }

    #[tokio::test]
    async fn native_broker_audit_waits_for_a_delayed_export() {
        let temporary = tempfile::tempdir().unwrap();
        let audit = temporary.path().join("native-audit.json");
        let delayed_audit = audit.clone();
        let writer = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(125)).await;
            tokio::fs::write(delayed_audit, b"{\"schemaVersion\":1}")
                .await
                .unwrap();
        });

        wait_for_native_broker_audit(&audit, Duration::from_secs(1))
            .await
            .unwrap();
        writer.await.unwrap();
    }

    #[tokio::test]
    async fn native_broker_audit_wait_has_a_bounded_timeout() {
        let temporary = tempfile::tempdir().unwrap();
        let audit = temporary.path().join("missing-audit.json");

        let error = wait_for_native_broker_audit(&audit, Duration::from_millis(75))
            .await
            .unwrap_err();

        let message = format!("{error:#}");
        assert!(message.contains("did not arrive"));
        assert!(message.contains("missing-audit.json"));
    }

    #[test]
    fn receipt_grace_is_only_used_after_runtime_mqtt_progress() {
        assert!(!should_wait_for_receipt_grace(Some(0), 60));
        assert!(should_wait_for_receipt_grace(Some(1), 60));
        assert!(!should_wait_for_receipt_grace(Some(1), 0));
        assert!(!should_wait_for_receipt_grace(None, 60));
    }

    #[tokio::test]
    async fn operation_completion_wins_without_a_shutdown_signal() {
        let outcome = await_or_shutdown(async { 42_u8 }, std::future::pending())
            .await
            .unwrap();

        assert_eq!(outcome, CampaignRunOutcome::Completed(42));
    }

    #[tokio::test]
    async fn shutdown_interrupts_a_running_campaign() {
        let outcome = await_or_shutdown(std::future::pending::<u8>(), async { Ok("SIGTERM") })
            .await
            .unwrap();

        assert_eq!(outcome, CampaignRunOutcome::Interrupted("SIGTERM"));
    }

    #[tokio::test]
    async fn shutdown_listener_failure_is_not_reported_as_a_signal() {
        let error = await_or_shutdown(std::future::pending::<u8>(), async {
            anyhow::bail!("signal registration failed")
        })
        .await
        .unwrap_err();

        assert!(error.to_string().contains("signal registration failed"));
    }

    #[tokio::test]
    async fn invalid_package_retains_original_bytes_and_failure_manifest() {
        let temporary = tempfile::tempdir().unwrap();
        let config = temporary.path().join("invalid-package.json");
        let evidence = temporary.path().join("campaign");
        let bytes = br#"{"edgeId":"missing-required-fields"}"#;
        tokio::fs::write(&config, bytes).await.unwrap();
        ensure_new_evidence_directory(&evidence).await.unwrap();

        let error = initialize_campaign(&config, &evidence, Utc::now())
            .await
            .unwrap_err();

        assert!(error.to_string().contains("decode failed"));
        assert_eq!(
            tokio::fs::read(evidence.join("configuration-package.json"))
                .await
                .unwrap(),
            bytes
        );
        let manifest: serde_json::Value = serde_json::from_slice(
            &tokio::fs::read(evidence.join("manifest.json"))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["status"], "failed");
        assert_eq!(manifest["schemaVersion"], 3);
        assert_eq!(manifest["phase"], "configuration_decode");
        assert_eq!(manifest["package"]["sha256"], sha256(bytes));
        assert!(manifest["errors"][0]
            .as_str()
            .unwrap()
            .contains("decode configuration package"));
    }
}
