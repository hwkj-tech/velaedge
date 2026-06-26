# Cloud Console Management UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first cloud management console that lets users configure edge nodes, device models, protocol connections, telemetry point mappings, collection tasks, config releases, and simulated edge apply status.

**Architecture:** Keep shared edge-facing contracts in `edge-core`, cloud authoring and validation services in `cloud-control`, HTTP/static serving in a new `cloud-api` crate, and the browser UI in `web/console`. Edge execution remains deterministic: cloud produces versioned config packages, while `edge-runtime` validates and applies them before collecting telemetry.

**Tech Stack:** Rust 2021, Cargo workspace, Axum, Tower HTTP, Tokio, Serde, in-memory first-version store, React, TypeScript, Vite, Vitest, CSS modules/plain CSS, and existing `edge-core`, `cloud-control`, and `edge-runtime` crates.

---

## Scope Check

This plan covers one coherent product slice: the cloud console configuration loop from authoring to simulated edge apply. It intentionally uses an in-memory store and simulated edge apply so the UI/API/runtime contract can stabilize before adding a database, real MQTT, real Modbus, OPC UA, S7, FINS, or production authentication.

## File Structure

- Modify `Cargo.toml` to add `crates/cloud-api` and shared web/API dependencies.
- Modify `crates/edge-core/src/lib.rs` and create `crates/edge-core/src/config.rs` for edge-facing config contracts.
- Modify `crates/cloud-control/src/lib.rs` and create `store.rs`, `validation.rs`, `release.rs`, and `audit.rs` for cloud authoring.
- Modify `crates/edge-runtime/src/lib.rs` and create `config.rs` for simulated config application.
- Create `crates/cloud-api` for REST APIs and static console hosting.
- Create `web/console` for the React management console.
- Update `README.md` and `docs/architecture.md` with the new run commands and module boundaries.

## Task 0: Baseline Existing Rust MVP

**Files:**
- Stage existing: `.gitignore`
- Stage existing: `Cargo.lock`
- Stage existing: `Cargo.toml`
- Stage existing: `README.md`
- Stage existing: `configs/cloud.sample.toml`
- Stage existing: `configs/edge.sample.toml`
- Stage existing: `crates/cloud-control/**`
- Stage existing: `crates/edge-core/**`
- Stage existing: `crates/edge-runtime/**`
- Stage existing: `docs/architecture.md`
- Stage existing: `docs/superpowers/plans/2026-06-26-edge-cloud-rust-platform.md`
- Stage existing: `docs/superpowers/specs/2026-06-26-edge-cloud-rust-platform-design.md`

- [ ] **Step 1: Verify the existing MVP before committing it**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

Expected: all commands exit 0.

- [ ] **Step 2: Commit the existing MVP as a clean baseline**

Run:

```bash
git add .gitignore Cargo.lock Cargo.toml README.md configs crates docs/architecture.md docs/superpowers/plans/2026-06-26-edge-cloud-rust-platform.md docs/superpowers/specs/2026-06-26-edge-cloud-rust-platform-design.md
git commit -m "feat: scaffold rust edge platform"
```

Expected: commit succeeds and the next feature work starts from a stable baseline.

## Task 1: Edge-Facing Configuration Contracts

**Files:**
- Modify: `crates/edge-core/src/lib.rs`
- Create: `crates/edge-core/src/config.rs`
- Test: `crates/edge-core/tests/config_contract.rs`

- [ ] **Step 1: Write failing tests for protocol connections, point mappings, tasks, and config packages**

Create `crates/edge-core/tests/config_contract.rs`:

```rust
use edge_core::{
    CollectionTask, DeviceInstance, EdgeConfigPackage, NumberRange, PointAddress,
    ProtocolConnection, ProtocolType, TelemetryPointMapping, TelemetryType,
};

#[test]
fn config_package_contains_edge_targets_and_point_mappings() {
    let package = EdgeConfigPackage::new("edge-dev", "2026.06.26-001")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_point_mapping(
            TelemetryPointMapping::new(
                "pressure",
                "pump-1",
                "pressure",
                "sim-main",
                PointAddress::simulated("pressure"),
                TelemetryType::Float,
            )
            .with_unit("MPa")
            .with_range(NumberRange::new(0.0, 20.0))
            .with_interval_ms(1000),
        )
        .with_collection_task(CollectionTask::interval(
            "pump-main-collection",
            "pump-1",
            vec!["pressure".to_string()],
            1000,
        ));

    assert_eq!(package.edge_id, "edge-dev");
    assert_eq!(package.version, "2026.06.26-001");
    assert_eq!(package.point_mappings[0].point_id, "pressure");
    assert_eq!(package.protocol_connections[0].protocol, ProtocolType::Simulated);
    assert_eq!(package.collection_tasks[0].point_ids, vec!["pressure"]);
}

#[test]
fn modbus_point_address_preserves_register_metadata() {
    let address = PointAddress::modbus_holding_register(40001);

    assert_eq!(address.kind, "holding_register");
    assert_eq!(address.value, "40001");
}
```

- [ ] **Step 2: Run tests and verify red**

Run:

```bash
cargo test -p edge-core --test config_contract
```

Expected: compilation fails because `EdgeConfigPackage`, `TelemetryPointMapping`, `ProtocolConnection`, `CollectionTask`, `DeviceInstance`, `ProtocolType`, and `PointAddress` are not defined.

- [ ] **Step 3: Implement the shared config model**

Create `crates/edge-core/src/config.rs` with serializable, edge-facing contracts:

```rust
use serde::{Deserialize, Serialize};

use crate::{AlgorithmSpec, DeviceSpec, NumberRange, TelemetryType};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EdgeConfigPackage {
    pub edge_id: String,
    pub version: String,
    pub device_models: Vec<DeviceSpec>,
    pub devices: Vec<DeviceInstance>,
    pub protocol_connections: Vec<ProtocolConnection>,
    pub point_mappings: Vec<TelemetryPointMapping>,
    pub collection_tasks: Vec<CollectionTask>,
    pub algorithms: Vec<AlgorithmSpec>,
}

impl EdgeConfigPackage {
    pub fn new(edge_id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            edge_id: edge_id.into(),
            version: version.into(),
            device_models: Vec::new(),
            devices: Vec::new(),
            protocol_connections: Vec::new(),
            point_mappings: Vec::new(),
            collection_tasks: Vec::new(),
            algorithms: Vec::new(),
        }
    }

    pub fn with_device(mut self, device: DeviceInstance) -> Self {
        self.devices.push(device);
        self
    }

    pub fn with_protocol_connection(mut self, connection: ProtocolConnection) -> Self {
        self.protocol_connections.push(connection);
        self
    }

    pub fn with_point_mapping(mut self, mapping: TelemetryPointMapping) -> Self {
        self.point_mappings.push(mapping);
        self
    }

    pub fn with_collection_task(mut self, task: CollectionTask) -> Self {
        self.collection_tasks.push(task);
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceInstance {
    pub device_id: String,
    pub device_type: String,
}

impl DeviceInstance {
    pub fn new(device_id: impl Into<String>, device_type: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            device_type: device_type.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolConnection {
    pub connection_id: String,
    pub protocol: ProtocolType,
    pub endpoint: Option<String>,
}

impl ProtocolConnection {
    pub fn simulated(connection_id: impl Into<String>) -> Self {
        Self {
            connection_id: connection_id.into(),
            protocol: ProtocolType::Simulated,
            endpoint: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProtocolType {
    Simulated,
    ModbusTcp,
    OpcUa,
    Mqtt,
    SiemensS7,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TelemetryPointMapping {
    pub point_id: String,
    pub device_id: String,
    pub semantic_id: String,
    pub protocol_connection_id: String,
    pub address: PointAddress,
    pub value_type: TelemetryType,
    pub unit: Option<String>,
    pub range: Option<NumberRange>,
    pub interval_ms: u64,
}

impl TelemetryPointMapping {
    pub fn new(
        point_id: impl Into<String>,
        device_id: impl Into<String>,
        semantic_id: impl Into<String>,
        protocol_connection_id: impl Into<String>,
        address: PointAddress,
        value_type: TelemetryType,
    ) -> Self {
        Self {
            point_id: point_id.into(),
            device_id: device_id.into(),
            semantic_id: semantic_id.into(),
            protocol_connection_id: protocol_connection_id.into(),
            address,
            value_type,
            unit: None,
            range: None,
            interval_ms: 1000,
        }
    }

    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    pub fn with_range(mut self, range: NumberRange) -> Self {
        self.range = Some(range);
        self
    }

    pub fn with_interval_ms(mut self, interval_ms: u64) -> Self {
        self.interval_ms = interval_ms;
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PointAddress {
    pub kind: String,
    pub value: String,
}

impl PointAddress {
    pub fn simulated(value: impl Into<String>) -> Self {
        Self {
            kind: "simulated".to_string(),
            value: value.into(),
        }
    }

    pub fn modbus_holding_register(address: u32) -> Self {
        Self {
            kind: "holding_register".to_string(),
            value: address.to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CollectionTask {
    pub task_id: String,
    pub device_id: String,
    pub point_ids: Vec<String>,
    pub interval_ms: u64,
    pub enabled: bool,
}

impl CollectionTask {
    pub fn interval(
        task_id: impl Into<String>,
        device_id: impl Into<String>,
        point_ids: Vec<String>,
        interval_ms: u64,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            device_id: device_id.into(),
            point_ids,
            interval_ms,
            enabled: true,
        }
    }
}
```

Update `crates/edge-core/src/lib.rs`:

```rust
pub mod config;

pub use config::{
    CollectionTask, DeviceInstance, EdgeConfigPackage, PointAddress, ProtocolConnection,
    ProtocolType, TelemetryPointMapping,
};
```

- [ ] **Step 4: Run tests and verify green**

Run:

```bash
cargo test -p edge-core --test config_contract
cargo test -p edge-core
```

Expected: all `edge-core` tests pass.

- [ ] **Step 5: Commit**

Run:

```bash
git add crates/edge-core/src/lib.rs crates/edge-core/src/config.rs crates/edge-core/tests/config_contract.rs
git commit -m "feat: add edge config contracts"
```

## Task 2: Cloud Authoring Store, Validation, Releases, And Audit

**Files:**
- Modify: `crates/cloud-control/src/lib.rs`
- Create: `crates/cloud-control/src/store.rs`
- Create: `crates/cloud-control/src/validation.rs`
- Create: `crates/cloud-control/src/release.rs`
- Create: `crates/cloud-control/src/audit.rs`
- Test: `crates/cloud-control/tests/config_loop.rs`

- [ ] **Step 1: Write failing cloud configuration-loop tests**

Create `crates/cloud-control/tests/config_loop.rs`:

```rust
use cloud_control::{
    AuditAction, CloudControlStore, ConfigValidator, ReleaseService, ReleaseStatus,
};
use edge_core::{
    CollectionTask, DeviceInstance, DeviceSpec, EdgeConfigPackage, PointAddress,
    ProtocolConnection, TelemetryPoint, TelemetryPointMapping, TelemetryType,
};

fn valid_package() -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", "2026.06.26-001")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_point_mapping(TelemetryPointMapping::new(
            "pressure",
            "pump-1",
            "pressure",
            "sim-main",
            PointAddress::simulated("pressure"),
            TelemetryType::Float,
        ))
        .with_collection_task(CollectionTask::interval(
            "pump-main",
            "pump-1",
            vec!["pressure".to_string()],
            1000,
        ))
}

#[test]
fn store_keeps_edges_models_and_config_packages() {
    let mut store = CloudControlStore::default();
    let model = DeviceSpec::new("pump", "1.0.0").with_telemetry(vec![
        TelemetryPoint::new("pressure", TelemetryType::Float),
    ]);

    store.upsert_device_model(model.clone());
    store.upsert_config_package(valid_package());

    assert_eq!(store.device_model("pump").unwrap(), &model);
    assert_eq!(store.config_package("edge-dev", "2026.06.26-001").unwrap().edge_id, "edge-dev");
}

#[test]
fn validator_rejects_point_mapping_with_missing_connection() {
    let mut package = valid_package();
    package.point_mappings[0].protocol_connection_id = "missing".to_string();

    let errors = ConfigValidator::validate_package(&package);

    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("missing protocol connection"));
}

#[test]
fn release_service_tracks_desired_and_reported_versions() {
    let mut store = CloudControlStore::default();
    let release = ReleaseService::create_release(&mut store, valid_package()).unwrap();

    assert_eq!(release.edge_id, "edge-dev");
    assert_eq!(release.desired_version, "2026.06.26-001");
    assert_eq!(release.status, ReleaseStatus::Pending);

    let applied = ReleaseService::mark_reported(&mut store, release.release_id, "2026.06.26-001")
        .unwrap();

    assert_eq!(applied.reported_version.as_deref(), Some("2026.06.26-001"));
    assert_eq!(applied.status, ReleaseStatus::Applied);
    assert_eq!(store.audit_records()[0].action, AuditAction::CreateRelease);
}
```

- [ ] **Step 2: Run tests and verify red**

Run:

```bash
cargo test -p cloud-control --test config_loop
```

Expected: compilation fails because the store, validator, release service, and audit types are missing.

- [ ] **Step 3: Implement the in-memory store and audit record**

Create `store.rs` and `audit.rs` with:

```rust
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use edge_core::{DeviceSpec, EdgeConfigPackage};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AuditAction, AuditRecord, ReleaseRecord};

#[derive(Clone, Debug, Default)]
pub struct CloudControlStore {
    device_models: BTreeMap<String, DeviceSpec>,
    config_packages: BTreeMap<(String, String), EdgeConfigPackage>,
    releases: BTreeMap<Uuid, ReleaseRecord>,
    audit_records: Vec<AuditRecord>,
}

impl CloudControlStore {
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
        self.config_packages.get(&(edge_id.to_string(), version.to_string()))
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

    pub fn push_audit(&mut self, action: AuditAction, target: impl Into<String>) {
        self.audit_records.push(AuditRecord {
            audit_id: Uuid::new_v4(),
            action,
            target: target.into(),
            actor: "system".to_string(),
            created_at: Utc::now(),
        });
    }

    pub fn audit_records(&self) -> &[AuditRecord] {
        &self.audit_records
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditRecord {
    pub audit_id: Uuid,
    pub action: AuditAction,
    pub target: String,
    pub actor: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditAction {
    CreateRelease,
    ApplyRelease,
}
```

- [ ] **Step 4: Implement validation and release services**

Create `validation.rs` and `release.rs`:

```rust
use std::collections::BTreeSet;

use edge_core::EdgeConfigPackage;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AuditAction, CloudControlStore};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationError {
    pub message: String,
}

pub struct ConfigValidator;

impl ConfigValidator {
    pub fn validate_package(package: &EdgeConfigPackage) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let connections = package
            .protocol_connections
            .iter()
            .map(|connection| connection.connection_id.as_str())
            .collect::<BTreeSet<_>>();
        let devices = package
            .devices
            .iter()
            .map(|device| device.device_id.as_str())
            .collect::<BTreeSet<_>>();

        for mapping in &package.point_mappings {
            if !connections.contains(mapping.protocol_connection_id.as_str()) {
                errors.push(ValidationError {
                    message: format!(
                        "point `{}` references missing protocol connection `{}`",
                        mapping.point_id, mapping.protocol_connection_id
                    ),
                });
            }
            if !devices.contains(mapping.device_id.as_str()) {
                errors.push(ValidationError {
                    message: format!(
                        "point `{}` references missing device `{}`",
                        mapping.point_id, mapping.device_id
                    ),
                });
            }
        }

        errors
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseRecord {
    pub release_id: Uuid,
    pub edge_id: String,
    pub desired_version: String,
    pub reported_version: Option<String>,
    pub status: ReleaseStatus,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReleaseStatus {
    Pending,
    Applied,
    Failed,
}

pub struct ReleaseService;

impl ReleaseService {
    pub fn create_release(
        store: &mut CloudControlStore,
        package: EdgeConfigPackage,
    ) -> Result<ReleaseRecord, Vec<ValidationError>> {
        let errors = ConfigValidator::validate_package(&package);
        if !errors.is_empty() {
            return Err(errors);
        }

        let release = ReleaseRecord {
            release_id: Uuid::new_v4(),
            edge_id: package.edge_id.clone(),
            desired_version: package.version.clone(),
            reported_version: None,
            status: ReleaseStatus::Pending,
        };

        store.upsert_config_package(package);
        store.insert_release(release.clone());
        store.push_audit(AuditAction::CreateRelease, release.release_id.to_string());
        Ok(release)
    }

    pub fn mark_reported(
        store: &mut CloudControlStore,
        release_id: Uuid,
        reported_version: impl Into<String>,
    ) -> Option<ReleaseRecord> {
        let reported_version = reported_version.into();
        let release = store.release_mut(release_id)?;
        release.reported_version = Some(reported_version.clone());
        release.status = if release.desired_version == reported_version {
            ReleaseStatus::Applied
        } else {
            ReleaseStatus::Failed
        };
        let cloned = release.clone();
        store.push_audit(AuditAction::ApplyRelease, release_id.to_string());
        Some(cloned)
    }
}
```

Update `lib.rs` to export all new types.

- [ ] **Step 5: Run tests and verify green**

Run:

```bash
cargo test -p cloud-control --test config_loop
cargo test -p cloud-control
```

Expected: all `cloud-control` tests pass.

- [ ] **Step 6: Commit**

Run:

```bash
git add crates/cloud-control/src/lib.rs crates/cloud-control/src/store.rs crates/cloud-control/src/validation.rs crates/cloud-control/src/release.rs crates/cloud-control/src/audit.rs crates/cloud-control/tests/config_loop.rs
git commit -m "feat: add cloud config authoring services"
```

## Task 3: Edge Runtime Config Apply And Simulated Collection

**Files:**
- Modify: `crates/edge-runtime/src/lib.rs`
- Create: `crates/edge-runtime/src/config.rs`
- Modify: `crates/edge-runtime/src/runtime.rs`
- Test: `crates/edge-runtime/tests/config_apply.rs`

- [ ] **Step 1: Write failing tests for applying cloud config to edge runtime**

Create `crates/edge-runtime/tests/config_apply.rs`:

```rust
use edge_core::{
    CollectionTask, DeviceInstance, EdgeConfigPackage, PointAddress, ProtocolConnection,
    TelemetryPointMapping, TelemetryType, TelemetryValue,
};
use edge_runtime::{AppliedEdgeConfig, ConfiguredSimulatedRuntime};

fn package() -> EdgeConfigPackage {
    EdgeConfigPackage::new("edge-dev", "2026.06.26-001")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_point_mapping(TelemetryPointMapping::new(
            "pressure",
            "pump-1",
            "pressure",
            "sim-main",
            PointAddress::simulated("pressure"),
            TelemetryType::Float,
        ))
        .with_collection_task(CollectionTask::interval(
            "pump-main",
            "pump-1",
            vec!["pressure".to_string()],
            1000,
        ))
}

#[tokio::test]
async fn applying_config_reports_version_and_collects_named_points() {
    let applied = AppliedEdgeConfig::apply(package()).unwrap();
    let mut runtime = ConfiguredSimulatedRuntime::new(applied);

    let report = runtime.collect_once().await.unwrap();

    assert_eq!(runtime.reported_version(), "2026.06.26-001");
    assert_eq!(report.samples_collected, 1);
    assert_eq!(
        runtime.shadow("pump-1").unwrap().latest_value("pressure"),
        Some(&TelemetryValue::Float(1.0))
    );
}
```

- [ ] **Step 2: Run tests and verify red**

Run:

```bash
cargo test -p edge-runtime --test config_apply
```

Expected: compilation fails because `AppliedEdgeConfig` and `ConfiguredSimulatedRuntime` do not exist.

- [ ] **Step 3: Implement config apply and configured simulated runtime**

Create `crates/edge-runtime/src/config.rs`:

```rust
use std::collections::BTreeMap;

use anyhow::{bail, Result};
use chrono::Utc;
use edge_core::{
    DataQuality, DeviceShadow, EdgeConfigPackage, TelemetrySample, TelemetryValue,
};

use crate::CollectionReport;

#[derive(Clone, Debug)]
pub struct AppliedEdgeConfig {
    package: EdgeConfigPackage,
}

impl AppliedEdgeConfig {
    pub fn apply(package: EdgeConfigPackage) -> Result<Self> {
        if package.edge_id.trim().is_empty() {
            bail!("edge id is required");
        }
        if package.version.trim().is_empty() {
            bail!("config version is required");
        }
        Ok(Self { package })
    }

    pub fn version(&self) -> &str {
        &self.package.version
    }

    pub fn package(&self) -> &EdgeConfigPackage {
        &self.package
    }
}

pub struct ConfiguredSimulatedRuntime {
    applied: AppliedEdgeConfig,
    shadows: BTreeMap<String, DeviceShadow>,
}

impl ConfiguredSimulatedRuntime {
    pub fn new(applied: AppliedEdgeConfig) -> Self {
        let mut shadows = BTreeMap::new();
        for device in &applied.package().devices {
            shadows.insert(
                device.device_id.clone(),
                DeviceShadow::new(&applied.package().edge_id, &device.device_id),
            );
        }
        Self { applied, shadows }
    }

    pub async fn collect_once(&mut self) -> Result<CollectionReport> {
        let mut samples_collected = 0;
        for mapping in &self.applied.package().point_mappings {
            let sample = TelemetrySample::new(
                &mapping.device_id,
                &mapping.point_id,
                TelemetryValue::Float(1.0),
                DataQuality::Good,
                Utc::now(),
            );
            if let Some(shadow) = self.shadows.get_mut(&mapping.device_id) {
                shadow.update(sample);
                samples_collected += 1;
            }
        }
        Ok(CollectionReport { samples_collected })
    }

    pub fn reported_version(&self) -> &str {
        self.applied.version()
    }

    pub fn shadow(&self, device_id: &str) -> Option<&DeviceShadow> {
        self.shadows.get(device_id)
    }
}
```

Update `crates/edge-runtime/src/lib.rs`:

```rust
pub mod config;

pub use config::{AppliedEdgeConfig, ConfiguredSimulatedRuntime};
```

- [ ] **Step 4: Run tests and verify green**

Run:

```bash
cargo test -p edge-runtime --test config_apply
cargo test -p edge-runtime
```

Expected: all `edge-runtime` tests pass.

- [ ] **Step 5: Commit**

Run:

```bash
git add crates/edge-runtime/src/lib.rs crates/edge-runtime/src/config.rs crates/edge-runtime/tests/config_apply.rs
git commit -m "feat: apply edge config packages"
```

## Task 4: Cloud API Crate

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/cloud-api/Cargo.toml`
- Create: `crates/cloud-api/src/lib.rs`
- Create: `crates/cloud-api/src/main.rs`
- Create: `crates/cloud-api/src/api.rs`
- Create: `crates/cloud-api/src/state.rs`
- Test: `crates/cloud-api/tests/api.rs`

- [ ] **Step 1: Add failing API tests**

Create `crates/cloud-api/tests/api.rs`:

```rust
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use cloud_api::{app, AppState};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn summary_endpoint_returns_initial_counts() {
    let response = app(AppState::default())
        .oneshot(Request::get("/api/summary").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn release_endpoint_accepts_valid_edge_config_package() {
    let payload = json!({
        "edge_id": "edge-dev",
        "version": "2026.06.26-001",
        "device_models": [],
        "devices": [{"device_id": "pump-1", "device_type": "pump"}],
        "protocol_connections": [{"connection_id": "sim-main", "protocol": "Simulated", "endpoint": null}],
        "point_mappings": [{
            "point_id": "pressure",
            "device_id": "pump-1",
            "semantic_id": "pressure",
            "protocol_connection_id": "sim-main",
            "address": {"kind": "simulated", "value": "pressure"},
            "value_type": "Float",
            "unit": "MPa",
            "range": null,
            "interval_ms": 1000
        }],
        "collection_tasks": [{
            "task_id": "pump-main",
            "device_id": "pump-1",
            "point_ids": ["pressure"],
            "interval_ms": 1000,
            "enabled": true
        }],
        "algorithms": []
    });

    let response = app(AppState::default())
        .oneshot(
            Request::post("/api/releases")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}
```

- [ ] **Step 2: Run tests and verify red**

Run:

```bash
cargo test -p cloud-api
```

Expected: Cargo fails because `cloud-api` is not a workspace member.

- [ ] **Step 3: Add workspace dependencies and `cloud-api` crate**

Modify root `Cargo.toml`:

```toml
[workspace]
members = [
    "crates/cloud-api",
    "crates/cloud-control",
    "crates/edge-core",
    "crates/edge-runtime",
]

[workspace.dependencies]
axum = "0.8"
tower = "0.5"
tower-http = { version = "0.6", features = ["fs", "trace"] }
```

Create `crates/cloud-api/Cargo.toml`:

```toml
[package]
name = "cloud-api"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
anyhow.workspace = true
axum.workspace = true
cloud-control = { path = "../cloud-control" }
edge-core = { path = "../edge-core" }
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
tower-http.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true

[dev-dependencies]
tower.workspace = true
```

- [ ] **Step 4: Implement minimal API routes**

Create `state.rs`, `api.rs`, `lib.rs`, and `main.rs`:

```rust
use std::sync::{Arc, Mutex};

use cloud_control::CloudControlStore;

#[derive(Clone, Default)]
pub struct AppState {
    pub store: Arc<Mutex<CloudControlStore>>,
}
```

```rust
use axum::{extract::State, http::StatusCode, routing::{get, post}, Json, Router};
use cloud_control::{ReleaseService, ReleaseStatus};
use edge_core::EdgeConfigPackage;
use serde::Serialize;

use crate::AppState;

#[derive(Serialize)]
pub struct SummaryResponse {
    pub edge_count: usize,
    pub pending_release_count: usize,
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/api/summary", get(summary))
        .route("/api/releases", post(create_release))
        .with_state(state)
}

async fn summary(State(_state): State<AppState>) -> Json<SummaryResponse> {
    Json(SummaryResponse {
        edge_count: 0,
        pending_release_count: 0,
    })
}

async fn create_release(
    State(state): State<AppState>,
    Json(package): Json<EdgeConfigPackage>,
) -> Result<(StatusCode, Json<ReleaseResponse>), (StatusCode, Json<ErrorResponse>)> {
    let mut store = state.store.lock().expect("store mutex poisoned");
    let release = ReleaseService::create_release(&mut store, package)
        .map_err(|errors| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    message: errors
                        .into_iter()
                        .map(|error| error.message)
                        .collect::<Vec<_>>()
                        .join("; "),
                }),
            )
        })?;

    Ok((
        StatusCode::CREATED,
        Json(ReleaseResponse {
            release_id: release.release_id.to_string(),
            edge_id: release.edge_id,
            desired_version: release.desired_version,
            status: release.status,
        }),
    ))
}

#[derive(Serialize)]
pub struct ReleaseResponse {
    pub release_id: String,
    pub edge_id: String,
    pub desired_version: String,
    pub status: ReleaseStatus,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub message: String,
}
```

```rust
pub mod api;
pub mod state;

pub use api::app;
pub use state::AppState;
```

```rust
use anyhow::Result;
use cloud_api::{app, AppState};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    axum::serve(listener, app(AppState::default())).await?;
    Ok(())
}
```

- [ ] **Step 5: Run API tests and workspace tests**

Run:

```bash
cargo test -p cloud-api
cargo test --workspace
```

Expected: `cloud-api` tests and all workspace tests pass.

- [ ] **Step 6: Commit**

Run:

```bash
git add Cargo.toml Cargo.lock crates/cloud-api
git commit -m "feat: add cloud console API"
```

## Task 5: React Console Scaffold And API Client

**Files:**
- Create: `web/console/package.json`
- Create: `web/console/index.html`
- Create: `web/console/tsconfig.json`
- Create: `web/console/vite.config.ts`
- Create: `web/console/src/main.tsx`
- Create: `web/console/src/App.tsx`
- Create: `web/console/src/api/client.ts`
- Create: `web/console/src/api/types.ts`
- Test: `web/console/src/api/client.test.ts`

- [ ] **Step 1: Create failing frontend API-client test**

Create `web/console/src/api/client.test.ts`:

```ts
import { describe, expect, it, vi } from 'vitest';
import { fetchSummary } from './client';

describe('fetchSummary', () => {
  it('loads cloud summary from the API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ edge_count: 2, pending_release_count: 1 }),
    });

    const result = await fetchSummary(fetchMock as unknown as typeof fetch);

    expect(result.edge_count).toBe(2);
    expect(result.pending_release_count).toBe(1);
  });
});
```

- [ ] **Step 2: Run test and verify red**

Run:

```bash
cd web/console
npm test -- --run src/api/client.test.ts
```

Expected: command fails because the frontend project and test script do not exist.

- [ ] **Step 3: Add frontend scaffold**

Create `package.json`:

```json
{
  "name": "edgeops-console",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite --host 127.0.0.1",
    "build": "tsc && vite build",
    "test": "vitest"
  },
  "dependencies": {
    "@vitejs/plugin-react": "^4.3.0",
    "vite": "^5.4.0",
    "typescript": "^5.5.0",
    "react": "^18.3.0",
    "react-dom": "^18.3.0",
    "lucide-react": "^0.468.0"
  },
  "devDependencies": {
    "@testing-library/react": "^16.0.0",
    "@testing-library/jest-dom": "^6.4.0",
    "@types/react": "^18.3.0",
    "@types/react-dom": "^18.3.0",
    "jsdom": "^25.0.0",
    "vitest": "^2.0.0"
  }
}
```

Create minimal `index.html`, `tsconfig.json`, `vite.config.ts`, `src/main.tsx`, and `src/App.tsx` so Vite can render a root app shell.

- [ ] **Step 4: Add API types and client**

Create `src/api/types.ts`:

```ts
export interface SummaryResponse {
  edge_count: number;
  pending_release_count: number;
}
```

Create `src/api/client.ts`:

```ts
import type { SummaryResponse } from './types';

export async function fetchSummary(fetcher: typeof fetch = fetch): Promise<SummaryResponse> {
  const response = await fetcher('/api/summary');
  if (!response.ok) {
    throw new Error(`Failed to load summary: ${response.status}`);
  }
  return response.json() as Promise<SummaryResponse>;
}
```

- [ ] **Step 5: Install and run frontend test**

Run:

```bash
cd web/console
npm install
npm test -- --run src/api/client.test.ts
```

Expected: Vitest passes.

- [ ] **Step 6: Commit**

Run:

```bash
git add web/console/package.json web/console/package-lock.json web/console/index.html web/console/tsconfig.json web/console/vite.config.ts web/console/src
git commit -m "feat: scaffold cloud console frontend"
```

## Task 6: Console App Shell And Navigation

**Files:**
- Create: `web/console/src/layout/AppShell.tsx`
- Create: `web/console/src/layout/AppShell.css`
- Create: `web/console/src/pages/DashboardPage.tsx`
- Create: `web/console/src/pages/EdgeNodesPage.tsx`
- Create: `web/console/src/pages/DeviceModelsPage.tsx`
- Create: `web/console/src/pages/ProtocolConnectionsPage.tsx`
- Create: `web/console/src/pages/PointMappingsPage.tsx`
- Create: `web/console/src/pages/CollectionTasksPage.tsx`
- Create: `web/console/src/pages/AlgorithmsPage.tsx`
- Create: `web/console/src/pages/ReleasesPage.tsx`
- Create: `web/console/src/pages/RuntimeStatusPage.tsx`
- Create: `web/console/src/pages/AuditLogPage.tsx`
- Modify: `web/console/src/App.tsx`
- Test: `web/console/src/layout/AppShell.test.tsx`

- [ ] **Step 1: Write failing app-shell test**

Create `web/console/src/layout/AppShell.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { AppShell } from './AppShell';

describe('AppShell', () => {
  it('renders the core management navigation', () => {
    render(<AppShell activePage="点位配置"><div>content</div></AppShell>);

    expect(screen.getByText('EdgeOps Cloud')).toBeInTheDocument();
    expect(screen.getByText('工作台')).toBeInTheDocument();
    expect(screen.getByText('边端管理')).toBeInTheDocument();
    expect(screen.getByText('点位配置')).toBeInTheDocument();
    expect(screen.getByText('配置发布')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run test and verify red**

Run:

```bash
cd web/console
npm test -- --run src/layout/AppShell.test.tsx
```

Expected: fails because `AppShell` is missing.

- [ ] **Step 3: Implement dense operational app shell**

Create `AppShell.tsx` with a left nav, top status bar, and content area. Use lucide icons for navigation. Keep cards at 8px radius or less.

```tsx
import { Activity, Cpu, Database, FileClock, GitBranch, LayoutDashboard, ListChecks, RadioTower, ScrollText, Settings2 } from 'lucide-react';
import './AppShell.css';

const navItems = [
  ['工作台', LayoutDashboard],
  ['边端管理', RadioTower],
  ['设备模型', Cpu],
  ['协议连接', GitBranch],
  ['点位配置', Database],
  ['采集任务', ListChecks],
  ['算法配置', Settings2],
  ['配置发布', FileClock],
  ['运行状态', Activity],
  ['审计日志', ScrollText],
] as const;

export function AppShell({
  activePage,
  children,
}: {
  activePage: string;
  children: React.ReactNode;
}) {
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <strong>EdgeOps Cloud</strong>
          <span>边云一体化管理台</span>
        </div>
        <nav>
          {navItems.map(([label, Icon]) => (
            <button className={label === activePage ? 'nav-item active' : 'nav-item'} key={label}>
              <Icon size={16} />
              <span>{label}</span>
            </button>
          ))}
        </nav>
      </aside>
      <main className="main">
        <header className="topbar">
          <div>
            <h1>{activePage}</h1>
            <p>云端配置 / {activePage}</p>
          </div>
          <div className="status-strip">
            <span className="status online">edge-dev 在线</span>
            <span className="status version">配置 v2026.06.26</span>
          </div>
        </header>
        <section className="content">{children}</section>
      </main>
    </div>
  );
}
```

- [ ] **Step 4: Add CSS and page shells**

Create compact SaaS styling in `AppShell.css`. Add one page component per navigation item with page-specific heading content. Update `App.tsx` to render the dashboard by default.

- [ ] **Step 5: Run tests and build**

Run:

```bash
cd web/console
npm test -- --run src/layout/AppShell.test.tsx
npm run build
```

Expected: test and build pass.

- [ ] **Step 6: Commit**

Run:

```bash
git add web/console/src
git commit -m "feat: add cloud console app shell"
```

## Task 7: Point Configuration And Release Pages

**Files:**
- Create: `web/console/src/components/DataTable.tsx`
- Create: `web/console/src/components/Drawer.tsx`
- Create: `web/console/src/pages/PointMappingsPage.css`
- Modify: `web/console/src/pages/PointMappingsPage.tsx`
- Modify: `web/console/src/pages/ReleasesPage.tsx`
- Test: `web/console/src/pages/PointMappingsPage.test.tsx`

- [ ] **Step 1: Write failing point mapping page test**

Create `web/console/src/pages/PointMappingsPage.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { PointMappingsPage } from './PointMappingsPage';

describe('PointMappingsPage', () => {
  it('shows point table and editor drawer fields', () => {
    render(<PointMappingsPage />);

    expect(screen.getByText('点位配置表')).toBeInTheDocument();
    expect(screen.getByText('pressure')).toBeInTheDocument();
    expect(screen.getByText('holding_register:40001')).toBeInTheDocument();
    expect(screen.getByText('编辑点位 pressure')).toBeInTheDocument();
    expect(screen.getByText('采集周期')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run test and verify red**

Run:

```bash
cd web/console
npm test -- --run src/pages/PointMappingsPage.test.tsx
```

Expected: fails because `PointMappingsPage` content is not implemented.

- [ ] **Step 3: Implement reusable table and drawer**

Create a simple `DataTable` with stable column widths and a `Drawer` component for right-side editing. Use semantic `<table>` markup for dense scan-friendly configuration data.

- [ ] **Step 4: Implement point mapping page**

Render a point table containing:

```ts
const rows = [
  {
    pointId: 'pressure',
    deviceId: 'pump-1',
    protocol: 'Modbus TCP',
    address: 'holding_register:40001',
    type: 'float32',
    interval: '1000ms',
    unit: 'MPa',
    status: '启用',
  },
  {
    pointId: 'running',
    deviceId: 'pump-1',
    protocol: 'Modbus TCP',
    address: 'coil:00001',
    type: 'bool',
    interval: '1000ms',
    unit: '-',
    status: '启用',
  },
];
```

The drawer must show sections: 基础信息, 协议映射, 采集策略, 数据治理.

- [ ] **Step 5: Implement release page**

Render the release flow with:

- Draft version `2026.06.26-001`.
- Validation status.
- Change summary.
- Edge apply results table showing desired and reported version.
- Primary action button labeled `发布配置`.

- [ ] **Step 6: Run tests and build**

Run:

```bash
cd web/console
npm test -- --run src/pages/PointMappingsPage.test.tsx
npm run build
```

Expected: test and build pass.

- [ ] **Step 7: Commit**

Run:

```bash
git add web/console/src/components web/console/src/pages
git commit -m "feat: add point config and release views"
```

## Task 8: Serve Built Console From Rust Cloud API

**Files:**
- Modify: `crates/cloud-api/Cargo.toml`
- Modify: `crates/cloud-api/src/api.rs`
- Modify: `crates/cloud-api/src/main.rs`
- Test: `crates/cloud-api/tests/static_assets.rs`

- [ ] **Step 1: Write failing static asset route test**

Create `crates/cloud-api/tests/static_assets.rs`:

```rust
use axum::{body::Body, http::{Request, StatusCode}};
use cloud_api::{app, AppState};
use tower::ServiceExt;

#[tokio::test]
async fn api_still_responds_when_static_console_is_configured() {
    let response = app(AppState::default())
        .oneshot(Request::get("/api/summary").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
```

- [ ] **Step 2: Run test and verify red or current pass without static serving**

Run:

```bash
cargo test -p cloud-api --test static_assets
```

Expected: if the API route already works, the test passes and still protects the API while static serving is added. If imports fail, fix the crate exports before proceeding.

- [ ] **Step 3: Add static file fallback**

Update `api.rs` so `/api/*` routes remain API routes and non-API paths serve `web/console/dist` when it exists:

```rust
use tower_http::services::{ServeDir, ServeFile};

pub fn app(state: AppState) -> Router {
    let api = Router::new()
        .route("/api/summary", get(summary))
        .route("/api/releases", post(create_release))
        .with_state(state);

    let static_files = ServeDir::new("web/console/dist")
        .not_found_service(ServeFile::new("web/console/dist/index.html"));

    api.fallback_service(static_files)
}
```

- [ ] **Step 4: Build frontend and run cloud API tests**

Run:

```bash
cd web/console
npm run build
cd ../..
cargo test -p cloud-api
```

Expected: frontend builds and `cloud-api` tests pass.

- [ ] **Step 5: Commit**

Run:

```bash
git add crates/cloud-api web/console/dist
git commit -m "feat: serve cloud console from rust api"
```

## Task 9: Documentation And Smoke Verification

**Files:**
- Modify: `README.md`
- Modify: `docs/architecture.md`
- Create: `docs/cloud-console.md`

- [ ] **Step 1: Document local run commands**

Add commands:

```bash
cargo run -p cloud-api
cd web/console && npm run dev
```

Document that first-version persistence is in-memory and resets on restart.

- [ ] **Step 2: Document configuration loop**

Create `docs/cloud-console.md` covering:

- Edge registration.
- Device model.
- Protocol connection.
- Point mapping.
- Collection task.
- Config release.
- Simulated apply.
- Runtime status.
- Agent boundary.

- [ ] **Step 3: Run final verification**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
cd web/console
npm test -- --run
npm run build
```

Expected: all commands exit 0.

- [ ] **Step 4: Commit**

Run:

```bash
git add README.md docs/architecture.md docs/cloud-console.md
git commit -m "docs: document cloud console workflow"
```

## Self-Review

- Spec coverage: the plan covers dashboard, edge management, device model contracts, protocol connections, point mappings, collection tasks, release publishing, runtime status, audit records, and Agent safety boundaries.
- Architecture consistency: cloud generates versioned config packages; edge runtime applies and reports versions; Agent remains advisory.
- Deferred scope: real database, real MQTT sync, real industrial protocol drivers, authentication, and RAG ingestion remain outside this first implementation slice.
- Type consistency: `EdgeConfigPackage`, `TelemetryPointMapping`, `ProtocolConnection`, `CollectionTask`, `ReleaseRecord`, and `ReleaseStatus` are named consistently across tests, services, and API snippets.
- Verification: every task includes red/green tests or smoke verification commands before commit.

