# EdgeLink Cloud Server Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the runtime-facing HTTP/MQTT direction with an EdgeLink private TCP protocol foundation where edge runtime actively connects to a cloud server process that internally contains API, gateway, Agent, control, and store modules.

**Architecture:** The cloud remains a single deployable process for the MVP, but code boundaries stay separate: `cloud-api` hosts the console HTTP API and an EdgeLink gateway module, `cloud-control` owns governance/state, and Agent logic remains advisory. Runtime-cloud traffic uses a length-prefixed EdgeLink frame carried over a runtime-initiated TCP session; TLS/mTLS is the production security mode, while plaintext is only allowed for local tests and development.

**Tech Stack:** Rust 2021, Tokio TCP, serde JSON frame payloads, UUID/chrono message metadata, future `tokio-rustls` TLS wrapping, existing Axum console API, existing edge-core contracts.

---

## File Structure

- Create: `crates/edge-core/src/edgelink.rs`
  - Shared EdgeLink message types, payload enum, ACK model, hello/heartbeat/config/metrics/events/commands variants, and length-prefixed frame encode/decode helpers.
- Modify: `crates/edge-core/src/lib.rs`
  - Export EdgeLink contracts.
- Add test: `crates/edge-core/tests/edgelink.rs`
  - Verify frame round trip, corrupted/incomplete frame rejection, and hello/ack payload contracts.
- Create: `crates/cloud-api/src/gateway.rs`
  - Cloud-side EdgeLink gateway session handler that reads the first runtime `Hello`, validates it, sends `Ack`, and returns a session summary.
- Modify: `crates/cloud-api/src/lib.rs`
  - Export the gateway module for tests and future server startup wiring.
- Add test: `crates/cloud-api/tests/gateway.rs`
  - Verify a runtime-initiated TCP connection receives an ACK and records the edge/runtime identity.
- Create: `crates/edge-runtime/src/edgelink_client.rs`
  - Runtime-side client that connects to a cloud gateway address, sends `Hello`, waits for `Ack`, and returns a connection report.
- Modify: `crates/edge-runtime/src/lib.rs`
  - Export the client.
- Modify: `crates/edge-runtime/src/main.rs`
  - Add `--cloud-gateway-addr` for the new private TCP path while leaving the existing HTTP MVP option as temporary development compatibility.
- Add test: `crates/edge-runtime/tests/edgelink_client.rs`
  - Verify the client sends a valid `Hello` frame and handles the cloud `Ack`.
- Modify: `docs/architecture.md`
  - Document that management UI uses HTTP and runtime-cloud control/data uses EdgeLink TCP + TLS/mTLS.
- Modify: `docs/superpowers/plans/2026-06-26-sqlite-rocksdb-real-console.md`
  - Replace runtime-facing HTTP/MQTT language with EdgeLink private TCP/TLS and keep HTTP only for the console/admin API.

---

## Task 1: Shared EdgeLink Contracts And Frame Codec

**Files:**
- Create: `crates/edge-core/src/edgelink.rs`
- Modify: `crates/edge-core/src/lib.rs`
- Add test: `crates/edge-core/tests/edgelink.rs`

- [x] **Step 1: Write failing tests**

Create `crates/edge-core/tests/edgelink.rs` with tests that:
- build an `EdgeLinkMessage::hello("edge-dev", "runtime-dev", "0.1.0", Some("2026.06.26-001".to_string()), vec!["protocol:modbus-tcp".to_string()])`;
- encode it with `encode_edgelink_frame`;
- decode it with `decode_edgelink_frame`;
- assert message kind, edge id, runtime id, and sequence are preserved;
- assert incomplete frames and invalid JSON fail.

- [x] **Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p edge-core --test edgelink
```

Expected: fails because `edge_core::edgelink` does not exist.

- [x] **Step 3: Implement minimal contracts and codec**

Implement:
- `EdgeLinkMessage`
- `EdgeLinkPayload`
- `EdgeLinkMessageKind`
- `EdgeLinkAck`
- `EdgeLinkHello`
- `EdgeLinkHeartbeat`
- `CommandCandidate` as the command request payload
- `EdgeLinkCommandResult`
- `encode_edgelink_frame`
- `decode_edgelink_frame`

Use a 4-byte big-endian frame length followed by serde JSON bytes. Limit decoded frames to 16 MiB.

- [x] **Step 4: Verify tests pass**

Run:

```bash
cargo test -p edge-core --test edgelink
```

Expected: all EdgeLink tests pass.

- [x] **Step 5: Commit**

```bash
git add crates/edge-core/src/edgelink.rs crates/edge-core/src/lib.rs crates/edge-core/tests/edgelink.rs
git commit -m "feat: add edgelink protocol contracts"
```

---

## Task 2: Cloud Gateway Handshake

**Files:**
- Create: `crates/cloud-api/src/gateway.rs`
- Modify: `crates/cloud-api/src/lib.rs`
- Add test: `crates/cloud-api/tests/gateway.rs`

- [x] **Step 1: Write failing gateway test**

Create `crates/cloud-api/tests/gateway.rs` with an async test that:
- starts a `TcpListener` on `127.0.0.1:0`;
- accepts one runtime connection;
- calls `handle_edgelink_session`;
- connects with a `TcpStream`;
- writes an EdgeLink `Hello`;
- reads an EdgeLink `Ack`;
- asserts the session summary contains `edge-dev` and `runtime-dev`.

- [x] **Step 2: Run test and verify failure**

Run:

```bash
cargo test -p cloud-api --test gateway
```

Expected: fails because `cloud_api::gateway` does not exist.

- [x] **Step 3: Implement gateway session handler**

Implement `handle_edgelink_session(stream, peer_addr)` that:
- reads exactly one frame;
- decodes it;
- rejects non-hello first messages;
- sends an ACK with the received `message_id`;
- returns `EdgeGatewaySession { edge_id, runtime_id, peer_addr }`.

- [x] **Step 4: Verify gateway test passes**

Run:

```bash
cargo test -p cloud-api --test gateway
```

Expected: gateway handshake test passes.

- [x] **Step 5: Commit**

```bash
git add crates/cloud-api/src/gateway.rs crates/cloud-api/src/lib.rs crates/cloud-api/tests/gateway.rs
git commit -m "feat: add cloud edgelink gateway handshake"
```

---

## Task 3: Runtime EdgeLink Client

**Files:**
- Create: `crates/edge-runtime/src/edgelink_client.rs`
- Modify: `crates/edge-runtime/src/lib.rs`
- Modify: `crates/edge-runtime/src/main.rs`
- Add test: `crates/edge-runtime/tests/edgelink_client.rs`

- [x] **Step 1: Write failing runtime client test**

Create `crates/edge-runtime/tests/edgelink_client.rs` with an async test that:
- starts a local TCP listener;
- records the first frame from the runtime client;
- replies with an ACK for that message id;
- calls `connect_edgelink_once("127.0.0.1:port", "edge-dev", "runtime-dev", "0.1.0", Some("2026.06.26-001"))`;
- asserts the connection report has `acked == true`;
- asserts the server observed a `Hello` payload with the same edge/runtime ids.

- [x] **Step 2: Run test and verify failure**

Run:

```bash
cargo test -p edge-runtime --test edgelink_client
```

Expected: fails because `edge_runtime::edgelink_client` does not exist.

- [x] **Step 3: Implement runtime client**

Implement `connect_edgelink_once` that:
- connects to the gateway with `TcpStream`;
- sends a `Hello` frame;
- waits for an `Ack`;
- returns `EdgeLinkConnectReport { edge_id, runtime_id, gateway_addr, acked }`.

- [x] **Step 4: Wire CLI flag**

Add `--cloud-gateway-addr` to `edge-runtime`. If provided, runtime sends an EdgeLink hello and exits after the handshake for the MVP.

- [x] **Step 5: Verify runtime tests pass**

Run:

```bash
cargo test -p edge-runtime --test edgelink_client
cargo test -p edge-runtime
```

Expected: runtime client tests and existing runtime tests pass.

- [x] **Step 6: Commit**

```bash
git add crates/edge-runtime/src/edgelink_client.rs crates/edge-runtime/src/lib.rs crates/edge-runtime/src/main.rs crates/edge-runtime/tests/edgelink_client.rs
git commit -m "feat: add runtime edgelink client handshake"
```

---

## Task 4: Architecture And Existing Plan Alignment

**Files:**
- Modify: `docs/architecture.md`
- Modify: `docs/superpowers/plans/2026-06-26-sqlite-rocksdb-real-console.md`
- Modify: `docs/superpowers/plans/2026-06-27-edgelink-cloud-server-foundation.md`

- [x] **Step 1: Update architecture docs**

Document:
- Browser/admin calls use HTTP API.
- Runtime-cloud calls use EdgeLink over TCP + TLS/mTLS.
- Runtime actively connects to Cloud.
- Cloud Edge Gateway and Cloud Agent may be deployed in one process but stay separate modules.
- HTTP runtime sync is temporary development compatibility.

- [x] **Step 2: Update persistent console plan**

Change the Edge Runtime Cloud Loop section so RocksDB queues flush through EdgeLink, not HTTP/MQTT.

- [x] **Step 3: Verify docs contain the new boundary**

Run:

```bash
rg "EdgeLink|cloud-gateway|HTTP runtime sync|MQTT" docs/architecture.md docs/superpowers/plans/2026-06-26-sqlite-rocksdb-real-console.md
```

Expected: `EdgeLink` appears for runtime-cloud transport, and HTTP is only described as console/admin or temporary compatibility.

- [x] **Step 4: Commit**

```bash
git add docs/architecture.md docs/superpowers/plans/2026-06-26-sqlite-rocksdb-real-console.md docs/superpowers/plans/2026-06-27-edgelink-cloud-server-foundation.md
git commit -m "docs: align cloud runtime transport with edgelink"
```

---

## Final Verification

Run:

```bash
cargo fmt --all -- --check
cargo test --workspace
```

Expected: all Rust formatting and workspace tests pass.
