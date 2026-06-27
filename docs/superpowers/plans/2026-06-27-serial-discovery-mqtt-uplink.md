# Serial Discovery And MQTT Uplink Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a first usable cloud-edge loop where runtime declares serial southbound protocol capability, performs controlled point discovery, cloud stores discovery results and Agent-style mapping suggestions, and MQTT is modeled as a northbound velaMQ uplink instead of a device protocol.

**Architecture:** Shared contracts live in `edge-core`. Cloud persists discovery and uplink state in the existing SQLite-backed store and exposes it through `cloud-api`. Runtime keeps deterministic behavior: it reads protocol capability configuration, reports it through EdgeLink Hello, and can generate a simulated serial discovery report for MVP verification.

**Tech Stack:** Rust 2021, Cargo workspace, Serde, Axum, SQLx SQLite, Tokio TCP EdgeLink, RocksDB, React, TypeScript, Vite, Vitest.

---

## Task 1: Shared Contracts

**Files:**
- Modify: `crates/edge-core/src/config.rs`
- Modify: `crates/edge-core/src/edgelink.rs`
- Modify: `crates/edge-core/src/lib.rs`
- Test: `crates/edge-core/tests/config_contract.rs`
- Test: `crates/edge-core/tests/edgelink.rs`

- [ ] Add failing tests for:
  - `ProtocolType` no longer contains MQTT as a southbound protocol.
  - `ProtocolConnection::modbus_rtu_serial(...)` captures serial settings.
  - `MqttUplinkConfig::velamq(...)` models northbound MQTT publishing.
  - `DiscoveryReport` and `PointMappingSuggestion` serialize through EdgeLink.
- [ ] Run:
  - `cargo test -p edge-core --test config_contract`
  - `cargo test -p edge-core --test edgelink`
- [ ] Implement the minimal models and EdgeLink payload variants.
- [ ] Re-run the same tests and commit.

## Task 2: Cloud Store And API

**Files:**
- Modify: `crates/cloud-control/src/store.rs`
- Modify: `crates/cloud-control/src/sqlite.rs`
- Modify: `crates/cloud-api/src/api.rs`
- Modify: `crates/cloud-api/src/state.rs`
- Test: `crates/cloud-control/tests/sqlite_store.rs`
- Test: `crates/cloud-api/tests/api.rs`

- [ ] Add failing tests for storing and listing:
  - MQTT uplink configs by edge id.
  - Discovery reports by edge id.
  - Mapping suggestions by edge id.
- [ ] Add API tests for:
  - `GET /api/edges/{edge_id}/mqtt-uplink`
  - `PUT /api/edges/{edge_id}/mqtt-uplink`
  - `POST /api/edges/{edge_id}/discovery/run`
  - `GET /api/edges/{edge_id}/discovery/suggestions`
- [ ] Implement in-memory store methods, SQLite tables, and API handlers.
- [ ] Re-run targeted cloud tests and commit.

## Task 3: Runtime Capability And Discovery

**Files:**
- Create: `crates/edge-runtime/src/capability.rs`
- Create: `crates/edge-runtime/src/discovery.rs`
- Modify: `crates/edge-runtime/src/edgelink_client.rs`
- Modify: `crates/edge-runtime/src/main.rs`
- Modify: `crates/edge-runtime/src/lib.rs`
- Test: `crates/edge-runtime/tests/edgelink_client.rs`
- Test: `crates/edge-runtime/tests/discovery.rs`

- [ ] Add failing tests for:
  - runtime capabilities include `protocol:modbus-rtu`, `transport:serial`, and `uplink:mqtt`.
  - EdgeLink hello sends configured capabilities.
  - simulated serial discovery creates deterministic discovered points.
- [ ] Implement capability config parsing from CLI/env defaults.
- [ ] Implement simulated discovery report generation.
- [ ] Re-run targeted runtime tests and commit.

## Task 4: Management Console

**Files:**
- Modify: `web/console/src/api/types.ts`
- Modify: `web/console/src/api/client.ts`
- Modify: `web/console/src/App.tsx`
- Modify: `web/console/src/layout/AppShell.tsx`
- Create: `web/console/src/pages/MqttUplinkPage.tsx`
- Create: `web/console/src/pages/MqttUplinkPage.test.tsx`
- Create: `web/console/src/pages/DiscoveryPage.tsx`
- Create: `web/console/src/pages/DiscoveryPage.test.tsx`

- [ ] Add failing page and API client tests.
- [ ] Implement MQTT 上报 page with velaMQ broker, client id, QoS, topic template, outbox policy.
- [ ] Implement 点位探测 page with discovery trigger, discovered point list, and Agent mapping suggestions.
- [ ] Re-run frontend tests and commit.

## Task 5: Verification

**Commands:**
- `cargo test`
- `cd web/console && npm test -- --run`
- `cd web/console && npm run build`
- `git diff --check`

- [ ] Validate in browser at `http://127.0.0.1:8080/`.
- [ ] Confirm MQTT appears only as northbound uplink, not device protocol.
- [ ] Confirm discovery suggestions are visible and do not publish automatically.
