# Edge Cloud Rust Platform Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a compiling Rust workspace for the edge-cloud platform MVP, including shared models, edge runtime collection, local storage, cloud control primitives, examples, docs, and tests.

**Architecture:** The workspace is split into focused crates. `edge-core` defines stable shared contracts. `edge-runtime` implements deterministic edge behavior over those contracts. `cloud-control` models cloud-side fleet and deployment planning without reaching into device execution internals.

**Tech Stack:** Rust 2021, Cargo workspace, `serde`, `chrono`, `uuid`, `thiserror`, `anyhow`, `tokio`, `async-trait`, `clap`, `tracing`, and JSONL local storage.

---

## File Structure

- Create `Cargo.toml` as the workspace manifest.
- Create `crates/edge-core` as the shared library crate.
- Create `crates/edge-runtime` as the edge runtime library and binary crate.
- Create `crates/cloud-control` as the cloud control library and binary crate.
- Create `configs/edge.sample.toml` and `configs/cloud.sample.toml`.
- Create `docs/architecture.md`.
- Create `README.md`.

## Tasks

### Task 1: Workspace Skeleton

**Files:**
- Create: `Cargo.toml`
- Create: `crates/edge-core/Cargo.toml`
- Create: `crates/edge-core/src/lib.rs`
- Create: `crates/edge-runtime/Cargo.toml`
- Create: `crates/edge-runtime/src/lib.rs`
- Create: `crates/edge-runtime/src/main.rs`
- Create: `crates/cloud-control/Cargo.toml`
- Create: `crates/cloud-control/src/lib.rs`
- Create: `crates/cloud-control/src/main.rs`

- [ ] **Step 1: Create empty workspace and crates**

Run:

```bash
cargo init --lib crates/edge-core
cargo init --lib crates/edge-runtime
cargo init --lib crates/cloud-control
```

- [ ] **Step 2: Replace generated manifests with workspace-aware manifests**

Set the root workspace dependencies and crate-local dependencies so all crates share versioned libraries.

- [ ] **Step 3: Run workspace metadata verification**

Run:

```bash
cargo metadata --format-version 1 --no-deps
```

Expected: Cargo prints workspace metadata for the three crates.

### Task 2: Core Domain Models

**Files:**
- Modify: `crates/edge-core/src/lib.rs`
- Create: `crates/edge-core/src/model.rs`
- Create: `crates/edge-core/src/policy.rs`
- Create: `crates/edge-core/src/shadow.rs`
- Create: `crates/edge-core/src/message.rs`
- Test: `crates/edge-core/tests/domain.rs`

- [ ] **Step 1: Write failing tests for device specs, shadow updates, policies, and message envelopes**

Tests must cover:

- A `DeviceSpec` can expose telemetry point metadata by id.
- A `DeviceShadow` stores the latest sample value by telemetry id.
- A `PolicyEngine` rejects out-of-range command parameters.
- A `CloudEnvelope` preserves message identity, edge id, schema version, and payload.

- [ ] **Step 2: Run tests and verify they fail because the model is missing**

Run:

```bash
cargo test -p edge-core
```

Expected: compilation fails because the referenced model modules do not exist.

- [ ] **Step 3: Implement the minimal core model**

Implement strongly typed structs and enums with `serde` support. Keep execution logic out of `edge-core`.

- [ ] **Step 4: Run tests and verify they pass**

Run:

```bash
cargo test -p edge-core
```

Expected: all `edge-core` tests pass.

### Task 3: Edge Runtime Pipeline

**Files:**
- Modify: `crates/edge-runtime/src/lib.rs`
- Create: `crates/edge-runtime/src/protocol.rs`
- Create: `crates/edge-runtime/src/storage.rs`
- Create: `crates/edge-runtime/src/runtime.rs`
- Test: `crates/edge-runtime/tests/pipeline.rs`

- [ ] **Step 1: Write failing tests for collection and local storage**

Tests must cover:

- A `SimulatedProtocolAdapter` returns configured telemetry samples.
- `EdgeRuntime::collect_once` updates the device shadow.
- `JsonlLocalStore` persists one telemetry record per line.

- [ ] **Step 2: Run tests and verify they fail because runtime types are missing**

Run:

```bash
cargo test -p edge-runtime
```

Expected: compilation fails because protocol, runtime, and storage modules do not exist.

- [ ] **Step 3: Implement protocol trait, simulator, JSONL store, and collection pipeline**

Use async traits for protocol adapters and storage. Store JSONL records as serialized telemetry samples.

- [ ] **Step 4: Run tests and verify they pass**

Run:

```bash
cargo test -p edge-runtime
```

Expected: all `edge-runtime` tests pass.

### Task 4: Cloud Control Primitives

**Files:**
- Modify: `crates/cloud-control/src/lib.rs`
- Create: `crates/cloud-control/src/fleet.rs`
- Create: `crates/cloud-control/src/config.rs`
- Create: `crates/cloud-control/src/agent.rs`
- Test: `crates/cloud-control/tests/control.rs`

- [ ] **Step 1: Write failing tests for fleet registration and Agent plans**

Tests must cover:

- A fleet registry stores and retrieves edge node metadata.
- A configuration package targets a specific edge id and version.
- An Agent-generated command candidate is converted into a policy-checkable edge command.

- [ ] **Step 2: Run tests and verify they fail because cloud modules are missing**

Run:

```bash
cargo test -p cloud-control
```

Expected: compilation fails because fleet, config, and agent modules do not exist.

- [ ] **Step 3: Implement minimal cloud control models**

Keep cloud models focused on planning and governance. Do not add direct device protocol execution to this crate.

- [ ] **Step 4: Run tests and verify they pass**

Run:

```bash
cargo test -p cloud-control
```

Expected: all `cloud-control` tests pass.

### Task 5: Documentation And Examples

**Files:**
- Create: `README.md`
- Create: `docs/architecture.md`
- Create: `configs/edge.sample.toml`
- Create: `configs/cloud.sample.toml`

- [ ] **Step 1: Document how the project is structured**

Describe the crate boundaries, edge-cloud responsibilities, Agent safety boundary, and next milestones.

- [ ] **Step 2: Add sample configuration files**

Include one edge id, one simulated device, two telemetry points, local storage path, and cloud sync placeholders.

- [ ] **Step 3: Verify documentation references real files and crates**

Run:

```bash
rg "edge-core|edge-runtime|cloud-control|configs/edge.sample.toml|docs/architecture.md" README.md docs configs
```

Expected: references match actual project paths.

### Task 6: Workspace Verification

**Files:**
- Modify as needed based on compiler or formatter feedback.

- [ ] **Step 1: Format**

Run:

```bash
cargo fmt --all -- --check
```

Expected: formatter check passes.

- [ ] **Step 2: Test**

Run:

```bash
cargo test --workspace
```

Expected: all workspace tests pass.

- [ ] **Step 3: Build**

Run:

```bash
cargo build --workspace
```

Expected: all crates build successfully.

## Self-Review

- The plan covers shared models, edge runtime, cloud control primitives, docs, and verification.
- No task depends on a real industrial driver, production broker, LLM service, or Kubernetes cluster.
- The MVP keeps Agent reasoning advisory and routes command candidates through policy-checkable command models.

