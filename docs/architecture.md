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
- `AlgorithmSpec`: descriptor for rule, WASM, ONNX, and Python algorithms.

### `crates/edge-runtime`

Deterministic edge runtime:

- `ProtocolAdapter`: trait for protocol-specific telemetry collection.
- `SimulatedProtocolAdapter`: test and demo adapter for MVP validation.
- `LocalStore`: trait for local persistence.
- `JsonlLocalStore`: simple inspectable local store for telemetry samples.
- `EdgeRuntime`: collection pipeline that reads telemetry, persists it, and updates `DeviceShadow`.

Real device protocols should be added as separate crates that implement `ProtocolAdapter`, then registered by the runtime.

### `crates/cloud-control`

Cloud control-plane primitives:

- `FleetRegistry`: stores edge node metadata.
- `ConfigPackage`: versioned deployment package for edge-specific device specs and algorithms.
- `AgentCommandDraft`: output from an Agent that becomes an `edge-core::CommandCandidate`.

This crate plans and governs. It does not execute protocol actions.

## Command Lifecycle

1. Cloud user or Agent creates a command draft.
2. Cloud converts the draft into `CommandCandidate` and records intent.
3. Cloud sync sends the command to the target edge node.
4. Edge validates command id, target device, parameters, ranges, risk, and confirmation requirements.
5. Edge protocol adapter executes only after policy approval.
6. Edge records command result locally and reports the result to cloud.

## Storage Direction

The MVP uses JSONL because each line can be inspected, replayed, and tested easily. Production storage can evolve behind the `LocalStore` trait:

- SQLite for simple relational local history.
- RocksDB for high-write embedded buffering.
- Parquet for batch-friendly history.
- Hybrid JSONL plus object upload for low-cost offline capture.

## Protocol Direction

Each real protocol adapter should keep low-level driver details isolated:

```text
protocol adapter -> normalized TelemetrySample -> edge runtime -> local store + shadow + cloud sync
```

Recommended first adapters:

- Modbus TCP/RTU for baseline register collection.
- OPC UA for semantic industrial servers.
- Siemens S7 for PLC integration.
- Omron FINS for Omron PLCs.
- BACnet for building automation.
- MQTT for devices that can publish telemetry directly.

## Agent Direction

Recommended Agent services:

- Fleet Ops Agent: health summary, capacity hints, and operational triage.
- Protocol Expert Agent: assists point-table and manual interpretation.
- Config Planning Agent: drafts config packages and rollout plans.
- Algorithm Orchestration Agent: recommends edge algorithms and input mappings.
- Maintenance Agent: explains alarms with manuals, SOPs, fault codes, and history.
- Safety Review Agent: reviews risky configuration or command changes.

All Agent outputs stay advisory until converted into governed configuration or policy-checked command candidates.

