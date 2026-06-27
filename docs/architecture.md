# Architecture

## System Boundaries

The platform is built around a strict cloud-edge-device separation:

- Cloud owns fleet management, metadata, configuration governance, rollout planning, audit views, Agent reasoning, and RAG.
- Edge owns device connections, protocol adapters, local command validation, local shadow state, buffering, storage, and offline operation.
- Devices own physical behavior, firmware or PLC logic, hard interlocks, and low-level protection.

The Agent layer must not directly write protocol registers or bypass runtime checks. It produces plans, explanations, configuration drafts, and command candidates. Every command candidate still passes through cloud governance and edge policy validation.

## Crate Responsibilities

### `crates/edge-core`

Shared contracts used by both edge and cloud:

- `DeviceSpec`: semantic model for telemetry, commands, events, and versions.
- `TelemetrySample`: normalized telemetry value with quality and timestamp.
- `CommandCandidate`: command intent that can be audited and policy-checked.
- `PolicyEngine`: deterministic safety validation for command candidates.
- `DeviceShadow`: latest known local device state.
- `CloudEnvelope`: versioned message wrapper for edge-cloud communication.
- `EdgeLinkMessage`: private runtime-cloud message contract carried over a runtime-initiated TCP session.
- `AlgorithmSpec`: descriptor for rule, WASM, ONNX, and Python algorithms.
- `EdgeConfigPackage`: edge-targeted configuration bundle with devices, protocol connections, MQTT uplinks, point mappings, collection tasks, and algorithms.

### `crates/edge-runtime`

Deterministic edge runtime:

- `ProtocolAdapter`: trait for protocol-specific telemetry collection.
- Runtime capability config: declares enabled serial collection protocols, MQTT northbound uplink support, and the local storage backend.
- `SimulatedProtocolAdapter`: test and demo adapter for MVP validation.
- `LocalStore`: trait for local persistence.
- `JsonlLocalStore`: simple inspectable local store for telemetry samples.
- `EdgeRuntime`: collection pipeline that reads telemetry, persists it, and updates `DeviceShadow`.
- `ConfiguredSimulatedRuntime`: first-version config apply path that validates an `EdgeConfigPackage`, records the applied version, and produces simulated telemetry.

Real device protocols should be added as separate crates that implement `ProtocolAdapter`, then registered by the runtime.

### `crates/cloud-control`

Cloud control-plane primitives:

- `FleetRegistry`: stores edge node metadata.
- `ConfigPackage`: versioned deployment package for edge-specific device specs and algorithms.
- `AgentCommandDraft`: output from an Agent that becomes an `edge-core::CommandCandidate`.
- `ConfigAuthoringService`: creates cloud-side point mappings, collection tasks, and edge-targeted config packages.
- `ReleaseService`: validates config packages, records desired versions, and tracks reported edge versions.

This crate plans and governs. It does not execute protocol actions.

### `crates/cloud-api`

Cloud API and console hosting:

- `GET /api/summary`: fleet and release summary for the console.
- `POST /api/releases`: accepts an `EdgeConfigPackage` and creates a release through `cloud-control`.
- `GET/PUT /api/edges/{edge_id}/mqtt-uplink`: manages runtime northbound publishing to velaMQ.
- `POST /api/edges/{edge_id}/discovery/run`: starts a controlled point discovery job and returns Agent mapping suggestions.
- `gateway`: EdgeLink session handling for runtime-initiated cloud connections.
- Static fallback: serves `web/console/dist` so the built React console is available from the same service.

The first API state is in-memory and intended for design validation. A database-backed implementation should preserve the same release, gateway session, runtime status, and audit contracts.

### `web/console`

Built-in management UI:

- Workbench, edge management, device models, protocol connections, point mappings, collection tasks, algorithms, MQTT uplink, point discovery, releases, runtime status, audit log, and Agent assistant views.
- Point configuration page owns the central cloud-side mapping workflow.
- Release page shows validation, change summary, desired versions, reported versions, and edge apply status.

## Command Lifecycle

1. Cloud user or Agent creates a command draft.
2. Cloud converts the draft into `CommandCandidate` and records intent.
3. Cloud Control approves and records the command.
4. Cloud Edge Gateway routes the command over the target runtime's EdgeLink session.
5. Edge validates command id, target device, parameters, ranges, risk, and confirmation requirements.
6. Edge protocol adapter executes only after policy approval.
7. Edge records command result locally and reports the result to cloud.

## Configuration Lifecycle

1. Cloud user registers or selects an edge node.
2. Cloud user defines semantic device models.
3. Cloud user configures reusable serial protocol connections.
4. Runtime may run controlled serial point discovery and report evidence to cloud.
5. Cloud Agent converts discovery evidence into point mapping candidates for user review.
6. Cloud user maps semantic telemetry points to protocol addresses.
7. Cloud user groups point mappings into collection tasks and attaches algorithms.
8. Cloud user configures the MQTT northbound uplink to velaMQ.
9. Cloud creates a versioned `EdgeConfigPackage`.
10. Release validation checks references, duplicate ids, and edge target consistency.
11. Cloud Edge Gateway sends the desired version through EdgeLink after the runtime connects.
12. Edge runtime validates locally, applies it, and reports the applied version.
13. Cloud compares desired and reported versions and records audit events.

The cloud console owns authoring, validation, release planning, and auditability. The edge runtime owns real protocol execution, local storage, policy checks, and offline behavior.

## Storage Direction

The MVP started with JSONL because each line can be inspected, replayed, and tested easily. The production direction is split by side:

- Cloud uses SQLite for fleet metadata, config versions, Agent suggestions, audit records, gateway sessions, latest runtime status, and release state.
- Edge runtime uses RocksDB for desired/applied config, active rule version, local shadow, offline telemetry/events/metrics queues, and idempotency records.
- JSONL can remain as a development adapter behind `LocalStore`.
- Parquet for batch-friendly history.
- Hybrid RocksDB plus object upload for low-cost offline capture.

## Protocol Direction

Runtime-cloud transport and device protocol adapters are separate concerns.

Runtime-cloud management and data flow:

```text
edge runtime -> EdgeLink TCP session -> Cloud Edge Gateway -> Cloud Control / Agent / SQLite
```

- Runtime actively connects to Cloud; edge nodes do not expose an inbound HTTP server.
- Production transport is EdgeLink over TCP + TLS 1.3 with mTLS certificates.
- The EdgeLink frame is a 4-byte big-endian length prefix followed by a versioned JSON message.
- HTTP is retained for the management console/admin API and temporary development compatibility only.
- Cloud Edge Gateway and Cloud Agent Service may run in one cloud process for the MVP, but their code responsibilities stay separate.

Each real device protocol adapter should keep low-level driver details isolated:

```text
serial protocol adapter -> normalized TelemetrySample -> edge runtime -> RocksDB outbox -> MQTT uplink -> velaMQ
                                                    -> local shadow + EdgeLink runtime status
```

Recommended first adapters:

- Modbus RTU for baseline RS-485 register collection.
- DL/T645 for electric meters.
- IEC 101 for power and telemetry devices.
- Custom serial framing for project-specific instruments.
- Modbus TCP and OPC UA can remain future adapters when Ethernet devices enter scope.
- Siemens S7 for PLC integration.
- Omron FINS for Omron PLCs.
- BACnet for building automation.

MQTT is not modeled as a southbound device protocol in the current runtime. It is a northbound publishing sink used after serial collection, so the same data can enter velaMQ and downstream systems.

## Agent Direction

Recommended Agent services:

- Fleet Ops Agent: health summary, capacity hints, and operational triage.
- Protocol Expert Agent: assists point-table and manual interpretation.
- Config Planning Agent: drafts config packages and rollout plans.
- Algorithm Orchestration Agent: recommends edge algorithms and input mappings.
- Maintenance Agent: explains alarms with manuals, SOPs, fault codes, and history.
- Safety Review Agent: reviews risky configuration or command changes.

All Agent outputs stay advisory until converted into governed configuration or policy-checked command candidates.
