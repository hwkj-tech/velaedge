# Point Driven Algorithm DSL Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a point-driven edge algorithm DSL that cloud can configure visually, runtime can execute deterministically, and MQTT uplink can report as normal telemetry/events.

**Architecture:** Cloud owns DSL authoring, validation, persistence, and release. Runtime receives the DSL in the edge config package, subscribes to collected point samples, executes deterministic built-in operations, emits virtual-point samples and events, and forwards reportable outputs through the existing MQTT uplink. The frontend exposes template-driven visual editing with a DSL preview instead of arbitrary code runtimes.

**Tech Stack:** Rust workspace (`edge-core`, `edge-runtime`, `cloud-api`, `cloud-control`), React/Vite console, SQLite cloud store, RocksDB runtime store, existing MQTT uplink abstraction.

---

### Task 1: Shared DSL Model

**Files:**
- Modify: `crates/edge-core/src/model.rs`
- Modify: `crates/edge-core/src/lib.rs`
- Test: `crates/edge-core/tests/config_contract.rs`

- [ ] Add `AlgorithmKind`, `AlgorithmDsl`, input bindings, trigger, step, output, and report policy structs.
- [ ] Replace code-runtime focused algorithm metadata with DSL-focused metadata while keeping point input/output ids easy to query.
- [ ] Verify serialization and config package round trip.

### Task 2: Runtime DSL Engine

**Files:**
- Create: `crates/edge-runtime/src/algorithm.rs`
- Modify: `crates/edge-runtime/src/lib.rs`
- Modify: `crates/edge-runtime/src/configured_runtime.rs`
- Modify: `crates/edge-runtime/src/main.rs`
- Test: `crates/edge-runtime/tests/algorithm_dsl.rs`
- Test: `crates/edge-runtime/tests/config_sync.rs`

- [ ] Implement deterministic execution for change report, window aggregate, expression aggregate, and threshold rule.
- [ ] Feed collected samples into the engine after each collection task.
- [ ] Emit derived virtual-point samples and events into the existing reporting path.
- [ ] Keep raw protocol collection independent of algorithm execution.

### Task 3: Cloud API DSL Persistence

**Files:**
- Modify: `crates/cloud-api/src/api.rs`
- Modify: `crates/cloud-api/src/state.rs`
- Test: `crates/cloud-api/tests/api.rs`

- [ ] Update algorithm create/save requests to accept DSL fields.
- [ ] Validate point ids, output ids, trigger constraints, and expression variables against the selected edge config.
- [ ] Persist DSL algorithms into the latest edge config package and return display-friendly responses.

### Task 4: Frontend Visual Editor

**Files:**
- Modify: `web/console/src/api/types.ts`
- Modify: `web/console/src/api/client.ts`
- Modify: `web/console/src/pages/AlgorithmsPage.tsx`
- Test: `web/console/src/pages/AlgorithmsPage.test.tsx`
- Test: `web/console/src/App.test.tsx`

- [ ] Replace runtime selection with algorithm type/template selection.
- [ ] Add point selector, trigger controls, parameter panels, output virtual-point controls, report policy controls, and read-only DSL preview.
- [ ] Save the generated DSL through the existing cloud client flow.

### Task 5: Verification

**Files:**
- Generated: `web/console/dist/*`

- [ ] Run targeted Rust tests for edge-core, edge-runtime, and cloud-api.
- [ ] Run all frontend tests.
- [ ] Run `npm run build`.
- [ ] Run full `cargo test`.
- [ ] Browser-smoke the algorithm editor create/save flow.
- [ ] Commit the completed implementation.
