# Edge Intelligence Platform

Rust workspace for a cloud-edge integrated device platform. The project starts with deterministic edge execution, shared device models, local storage, and cloud control-plane primitives. Agent intelligence is intentionally modeled as planning and governance: it can draft configuration or command candidates, but edge policy remains the final gate before any device action.

## Workspace

- `crates/edge-core`: shared domain contracts for device specs, telemetry, commands, policies, device shadows, algorithm specs, and cloud envelopes.
- `crates/edge-runtime`: edge runtime components, including protocol adapter traits, simulated telemetry collection, JSONL local storage, and an executable MVP.
- `crates/cloud-control`: cloud-side fleet registry, versioned config packages, and Agent command drafts that convert into policy-checkable edge commands.
- `configs/edge.sample.toml`: sample edge runtime configuration.
- `configs/cloud.sample.toml`: sample cloud control-plane configuration.
- `docs/architecture.md`: architecture notes and extension guidance.

## Quick Start

Run the tests:

```bash
cargo test --workspace
```

Run one simulated edge collection cycle:

```bash
cargo run -p edge-runtime -- --edge-id edge-dev --device-id pump-1 --storage data/telemetry.jsonl
```

Run the cloud control-plane MVP:

```bash
cargo run -p cloud-control -- --edge-id edge-dev
```

## Design Direction

The first implementation keeps the control path simple and auditable:

1. Device semantics live in `edge-core` as `DeviceSpec`, telemetry points, command specs, and algorithm descriptors.
2. Edge collection happens through `ProtocolAdapter`, so real Modbus, OPC UA, S7, FINS, BACnet, and MQTT adapters can be added without changing cloud models.
3. Edge local storage starts with JSONL for inspectability. Production deployments can replace it with SQLite, RocksDB, Parquet, or a hybrid store behind `LocalStore`.
4. Agent output is represented as `AgentCommandDraft`, then converted into `CommandCandidate` and validated by `PolicyEngine`.

## Next Milestones

- Add real protocol adapter crates: `protocol-modbus`, `protocol-opcua`, `protocol-s7`, `protocol-fins`, and `protocol-bacnet`.
- Add MQTT over TLS cloud sync with idempotent command delivery.
- Add versioned config apply and rollback inside `edge-runtime`.
- Add WASM and ONNX algorithm package runners.
- Add persistent cloud services over PostgreSQL, TimescaleDB, object storage, and vector search.

