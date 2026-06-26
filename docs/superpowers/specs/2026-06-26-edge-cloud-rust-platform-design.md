# Edge Cloud Rust Platform Design

## Goal

Build a Rust-based edge-cloud integrated platform for industrial and IoT scenarios. The edge runtime performs deterministic device collection, local algorithms, storage, safety checks, and cloud synchronization. The cloud control plane manages fleets, device models, configuration versions, algorithm packages, commands, audit records, and Agent-assisted operations.

## Architecture Boundaries

- Cloud owns modeling, fleet management, configuration governance, Agent reasoning, RAG, audit views, and rollout orchestration.
- Edge owns protocol connections, command execution, policy checks, local shadow state, local buffering, local algorithms, and offline behavior.
- Devices own physical action, firmware logic, hard interlocks, and low-level protection.
- Agent services generate plans, explanations, configuration drafts, and command candidates. They never directly write protocol registers or bypass edge policy checks.

## Workspace Layout

- `crates/edge-core`: shared domain models, device specs, telemetry, commands, policies, shadows, algorithm descriptors, and cloud message envelopes.
- `crates/edge-runtime`: edge collection pipeline, protocol adapter traits, simulated protocol adapter, local JSONL storage, and executable runtime entrypoint.
- `crates/cloud-control`: cloud-side fleet/config/command planning primitives and executable control-plane entrypoint.
- `configs`: example edge and cloud configuration files.
- `docs/architecture.md`: project architecture, module responsibilities, and extension guidance.

## First Release Scope

The first release is a compiling MVP scaffold with behavior covered by Rust tests:

- Semantic device model instead of raw protocol-only point mapping.
- Protocol adapter abstraction with a simulator implementation.
- Edge runtime collection pipeline from adapter read to local storage.
- Device shadow updates from telemetry samples.
- Policy guard for command candidates.
- Cloud-side configuration package model with versioned edge deployment target.
- README and example configuration for the next engineering iteration.

## Non-Goals For First Release

- Real Modbus, OPC UA, Siemens S7, FINS, or BACnet drivers.
- Production MQTT broker integration.
- Full Kubernetes deployment manifests.
- LLM/RAG integration runtime.
- OTA binary update mechanism.

These are represented by stable interfaces and documentation, then implemented in later milestones.

## Safety Model

All commands flow through a deterministic lifecycle:

1. Cloud or Agent creates a command candidate.
2. Cloud validates permissions and records intent.
3. Edge receives the command with an idempotency key.
4. Edge policy engine checks range, risk, online state, and confirmation requirements.
5. Protocol adapter executes only after policy approval.
6. Edge records the result and reports it back to cloud.

The MVP implements the shared model and policy guard. Protocol write execution remains behind an interface.

