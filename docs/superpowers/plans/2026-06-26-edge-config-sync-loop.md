# Edge Config Sync Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make cloud-published configuration consumable by an edge runtime through a tested desired-config and reported-config loop.

**Architecture:** Cloud API exposes edge-scoped sync endpoints that return the latest desired config package and accept a reported version. Edge runtime adds a small trait-driven sync client and runner that applies a desired package with `AppliedEdgeConfig`, builds `ConfiguredSimulatedRuntime`, collects once, and reports the applied version. The MVP keeps transport abstract inside `edge-runtime` so HTTP/MQTT can be added without coupling runtime tests to a network client.

**Tech Stack:** Rust 2021, Axum, existing `cloud-control`, `edge-core`, and `edge-runtime` crates, Tokio tests.

---

## File Structure

- Modify `crates/cloud-api/src/api.rs`: add edge sync routes and response/request DTOs.
- Modify `crates/cloud-api/tests/api.rs`: cover desired-config fetch and reported-config update.
- Create `crates/edge-runtime/src/sync.rs`: define `EdgeConfigSyncClient`, sync result, and apply/report orchestration.
- Modify `crates/edge-runtime/src/lib.rs`: export the sync module.
- Add `crates/edge-runtime/tests/config_sync.rs`: cover edge runtime pull/apply/report behavior with an in-memory client.

## Tasks

### Task 1: Cloud API Edge Sync Endpoints

**Files:**
- Modify: `crates/cloud-api/src/api.rs`
- Test: `crates/cloud-api/tests/api.rs`

- [ ] **Step 1: Write failing API tests**

Add tests that:
- `GET /api/edges/edge-dev/desired-config` returns the latest package version and point mapping after a draft update.
- `POST /api/edges/edge-dev/reported-config` marks the matching release applied when the edge reports the desired version.

- [ ] **Step 2: Verify tests fail**

Run:

```bash
cargo test -p cloud-api --test api edge_desired_config_endpoint_returns_latest_package edge_reported_config_endpoint_marks_release_applied
```

Expected: fail with `404` or missing route.

- [ ] **Step 3: Implement routes**

Add:
- `GET /api/edges/{edge_id}/desired-config`
- `POST /api/edges/{edge_id}/reported-config`

`desired-config` should return `{ edgeId, desiredVersion, package }` for the latest config package for that edge.

`reported-config` should find the release for `edge_id + reportedVersion`, call `ReleaseService::mark_reported`, and return the normal release list.

- [ ] **Step 4: Verify cloud API tests pass**

Run:

```bash
cargo test -p cloud-api --test api
```

Expected: all API tests pass.

### Task 2: Edge Runtime Sync Runner

**Files:**
- Create: `crates/edge-runtime/src/sync.rs`
- Modify: `crates/edge-runtime/src/lib.rs`
- Test: `crates/edge-runtime/tests/config_sync.rs`

- [ ] **Step 1: Write failing runtime sync test**

Add a test with an in-memory client that returns a desired `EdgeConfigPackage`. The test calls `sync_once`, then asserts:
- the runtime reported version equals the desired version
- the runtime collected the configured point
- the client received the reported version

- [ ] **Step 2: Verify test fails**

Run:

```bash
cargo test -p edge-runtime --test config_sync
```

Expected: fail because sync module/types do not exist.

- [ ] **Step 3: Implement trait-driven sync**

Create:
- `EdgeDesiredConfig { desired_version, package }`
- `EdgeConfigSyncClient` async trait with `fetch_desired_config(edge_id)` and `report_applied_version(edge_id, version)`
- `EdgeConfigSyncReport { applied_version, samples_collected }`
- `sync_once(edge_id, client)` that applies the package, builds `ConfiguredSimulatedRuntime`, collects once, reports the runtime version, and returns the report.

- [ ] **Step 4: Verify runtime tests pass**

Run:

```bash
cargo test -p edge-runtime
```

Expected: all edge runtime tests pass.

### Task 3: Full Verification And Commit

**Files:**
- Modify as needed based on compiler/formatter output.

- [ ] **Step 1: Format and test**

Run:

```bash
cargo fmt --all -- --check
cargo test --workspace
npm test -- --run
npm run build
```

Expected: all commands pass.

- [ ] **Step 2: Commit**

Run:

```bash
git add docs/superpowers/plans/2026-06-26-edge-config-sync-loop.md crates/cloud-api/src/api.rs crates/cloud-api/tests/api.rs crates/edge-runtime/src crates/edge-runtime/tests/config_sync.rs
git commit -m "feat: add edge config sync loop"
```

Expected: commit succeeds with a clean worktree.

## Self-Review

- The plan implements the next missing cloud-edge runtime loop without adding production broker or HTTP client dependencies to edge-runtime.
- The cloud API tests exercise real Axum routes and shared store behavior.
- The runtime sync tests exercise real config application, collection, and reported-version behavior.
