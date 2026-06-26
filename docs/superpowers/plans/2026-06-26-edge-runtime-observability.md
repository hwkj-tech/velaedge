# Edge Runtime Observability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the MVP runtime observability loop from edge metrics generation through cloud API ingestion to an API-backed console status page.

**Architecture:** `edge-core` owns shared metric/event contracts. `edge-runtime` generates deterministic simulated snapshots from applied config. `cloud-control` stores latest runtime snapshots and recent events. `cloud-api` exposes report/query endpoints. The console loads runtime status from the API and renders summary, edge, protocol, and event views.

**Tech Stack:** Rust 2021, serde, chrono, Axum, Tokio tests, React + TypeScript + Vitest.

---

## File Structure

- Create `crates/edge-core/src/observability.rs` for shared runtime metrics and event models.
- Modify `crates/edge-core/src/lib.rs` to export observability types.
- Add `crates/edge-core/tests/observability.rs` for serialization and model behavior.
- Create `crates/edge-runtime/src/metrics.rs` for simulated runtime metrics collection.
- Modify `crates/edge-runtime/src/lib.rs` to export collector types.
- Add `crates/edge-runtime/tests/metrics.rs` for collector behavior.
- Create `crates/cloud-control/src/runtime_status.rs` for latest snapshot/event store helpers.
- Modify `crates/cloud-control/src/lib.rs` and `crates/cloud-control/src/store.rs`.
- Add `crates/cloud-control/tests/runtime_status.rs`.
- Modify `crates/cloud-api/src/api.rs` and `crates/cloud-api/src/state.rs` for runtime status endpoints and seed data.
- Extend `crates/cloud-api/tests/api.rs`.
- Modify `web/console/src/api/types.ts`, `web/console/src/api/client.ts`, `web/console/src/api/client.test.ts`.
- Modify `web/console/src/App.tsx` and `web/console/src/pages/RuntimeStatusPage.tsx`; add tests.

## Tasks

### Task 1: Core Observability Contracts

- [ ] Write failing `edge-core` tests for snapshot/event serialization.
- [ ] Run `cargo test -p edge-core --test observability` and verify missing types fail.
- [ ] Implement `observability.rs` with health, snapshot, nested metric groups, event severity/category.
- [ ] Export types from `edge-core/src/lib.rs`.
- [ ] Run `cargo test -p edge-core`.
- [ ] Commit `feat: add runtime observability models`.

### Task 2: Edge Runtime Metrics Collector

- [ ] Write failing `edge-runtime` test that applies a config and creates a snapshot with config version, healthy status, protocol metrics, and collection metrics.
- [ ] Run `cargo test -p edge-runtime --test metrics` and verify missing collector fails.
- [ ] Implement `SimulatedRuntimeMetricsCollector`.
- [ ] Run `cargo test -p edge-runtime`.
- [ ] Commit `feat: add edge runtime metrics collector`.

### Task 3: Cloud Store And API Runtime Status

- [ ] Write failing `cloud-control` tests for upserting latest snapshot and appending events.
- [ ] Implement store fields and helper methods.
- [ ] Write failing `cloud-api` tests for `POST /api/edges/{edge_id}/runtime-metrics`, `POST /api/edges/{edge_id}/runtime-events`, and `GET /api/runtime-status`.
- [ ] Implement API DTOs/routes and seed runtime status in `AppState::default`.
- [ ] Run `cargo test -p cloud-control` and `cargo test -p cloud-api`.
- [ ] Commit `feat: add cloud runtime status APIs`.

### Task 4: Console API And Runtime Page

- [ ] Write failing API client tests for `fetchRuntimeStatus`.
- [ ] Implement TypeScript runtime status types and client call.
- [ ] Write failing page/App tests that render API-backed runtime status.
- [ ] Replace static runtime page with summary tiles, edge table, protocol table, and event table.
- [ ] Wire `RuntimeStatusPage` through `App.tsx`.
- [ ] Run `npm test -- --run` and `npm run build`.
- [ ] Commit `feat: show runtime observability in console`.

### Task 5: Final Verification

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo test --workspace`.
- [ ] Run `npm test -- --run`.
- [ ] Run `npm run build`.
- [ ] Confirm `git status --short` is clean.

## Self-Review

- Scope matches the approved MVP and avoids production broker/SSE work for this iteration.
- All production behavior has a red/green test path.
- The runtime page remains operational-tool focused, not a marketing screen.
