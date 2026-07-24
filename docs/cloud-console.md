# VelaEdge Console Workflow

The cloud console is the first built-in management surface for the edge-cloud platform. It is an operations tool for configuring edge collection, releasing versioned config packages, and checking edge apply status.

## Run Locally

Build the console and serve it from Rust:

```bash
cd web/console
npm install
npm run build
cd ../..
cargo run -p cloud-api
```

Open `http://127.0.0.1:8080`.

For frontend-only iteration:

```bash
cd web/console
npm run dev
```

Cloud metadata is stored in SQLite by the cloud service. Runtime-local desired config, queues, and applied-state records are designed for RocksDB on the edge side.

## Production First Run

Production deployments use `EDGEOPS_BOOTSTRAP_MODE=empty`, so the first console load contains no
sample project, product, edge, metrics, or configuration. Initialize the tenant in this order:

1. Open **项目管理** and create the first project. The selected project and environment in the
   header update from the persisted SQLite record.
2. Open **点位管理** and create reusable point sets. Define protocol address, value type, scale,
   unit, and collection interval for every point in the set.
3. Open **产品管理**, create a product under the project, bind one or more point sets, and configure
   its visual collection graph and MQTT outputs. Product drafts are versioned before release.
4. Open **边端管理**, create the expected edge identity, select its product, and issue the one-time
   enrollment token. The Runtime connects to EdgeLink and reports heartbeat, metrics, capabilities,
   desired version, and applied version.
5. Publish a product version to the edge and confirm its apply acknowledgement in **运行状态**.

When there is no edge yet, the console does not query edge-scoped endpoints or invent an
`edge-dev` instance. Empty tables remain actionable and lead directly to their create dialogs.

## Configuration Loop

1. Edge registration
   Register edge nodes, assign site or group metadata, track runtime version, heartbeat, desired config version, and reported config version.

2. Device model
   Define semantic telemetry, commands, events, units, ranges, and data types before choosing protocol-specific addresses.

3. Protocol connection
   Create reusable southbound collection connections such as Simulated, Modbus RTU, DL/T645, IEC 101, or custom serial adapters. Secret fields must be governed and not displayed in clear text after save.

4. Point mapping
   Map a semantic telemetry id to a protocol connection and address, for example `voltage_a -> Modbus RTU -> holding_register:40001`. Configure data type, scale, unit, interval, quality rule, and range.

5. Point discovery
   Run controlled read-only serial discovery from cloud. Runtime reports discovered addresses and samples, then the Agent produces point mapping suggestions for user confirmation.

6. Collection task
   Group point mappings into runtime tasks with interval, timeout, retry, deadband, and cache policy.

7. Collection orchestration
   Build a directed, acyclic data flow from reusable point inputs through deterministic calculation
   nodes to one or more MQTT outputs. The current palette contains window aggregation, moving
   average, statistical summary, change reporting, deadband filtering, debounce, continuous
   condition, expression, scale/offset, clamp, rate of change, multi-point merge, conditional
   routing, and alarm event nodes. A continuous-condition node emits once after its comparison
   remains true for the configured duration, and may emit again only after the condition resets.
   Conditional nodes expose named outputs, one output may fan out to several downstream nodes,
   and separate branches may publish to different `sink_id` and topic templates. Runtime executes
   the same persisted graph and DSL; the browser is not the execution engine.

8. MQTT uplink
   Configure the runtime's northbound MQTT publishing sink to velaMQ. MQTT is used for serial telemetry upload, not as a device-side acquisition protocol.

9. Config release
   Generate an `EdgeConfigPackage`, validate references, review change summary, and publish to selected edge nodes.

10. Runtime apply
   Runtime validates the package, persists desired and applied versions in RocksDB, executes configured serial adapters and calculation graphs, buffers MQTT output in the acknowledged outbox, and reports the applied version.

11. Runtime status
   Compare desired and reported versions, heartbeat, protocol adapter capability, local storage status, and config apply history. CPU, memory, disk, and process uptime are sampled by the connected Runtime from its host; offline catalog rows may retain their last reported snapshot but the console does not fabricate a new live sample.

## Page Map

- Dashboard: read-only fleet health, configuration coverage, event, and throughput monitoring.
- 项目管理: project isolation, environment, owner, and resource counts.
- 产品管理: product identity, point-set binding, visual collection graph, MQTT outputs, versioning,
  validation, release, and rollback.
- 点位管理: reusable point-set lists and batch point editing for serial addresses, types, scales,
  units, and independent collection intervals.
- 边端管理: explicit edge enrollment, product binding, one-time token issuance, MQTT broker
  settings, configuration target version, and edge monitoring entry.
- 运行状态: heartbeat, CPU, memory, disk, protocol capabilities, local-store state, desired/applied
  version convergence, and apply history.
- 审计日志: immutable records for catalog changes, token operations, releases, reviews, and apply
  results.
- Agent 助手: scoped chat, governed knowledge, explainable suggestions, and approval-only proposals.

## Agent Boundary

Agent capabilities are intentionally advisory:

- It may draft point mappings, explain failures, summarize release risk, or suggest rollout plans.
- It must not publish configurations without user approval.
- It must not directly write protocol registers or execute physical device commands.
- Any command candidate must pass cloud governance and edge-side policy validation.

The chat UI uses `GET /api/agent/provider` to show whether the backend is running in deterministic
local-analysis mode or through a configured OpenAI-compatible model gateway. `POST /api/agent/chat`
accepts the operator question plus optional project and edge scope. The backend validates that scope
and supplies only a bounded operational summary; credentials, certificates, secret references, raw
protocol frames, and MQTT passwords are never included. Configure the gateway with
`EDGEOPS_AGENT_ENDPOINT`, `EDGEOPS_AGENT_MODEL`, optional `EDGEOPS_AGENT_API_KEY`, and optional
`EDGEOPS_AGENT_TIMEOUT_MS`. These values stay server-side and are not returned to the browser.

Model calls are non-streaming and failure is explicit: provider errors are returned to the console
instead of silently pretending that a model answered. When no provider is configured, the backend
uses deterministic context-based analysis and labels the response accordingly.

The Agent page includes a project scope selector and a managed knowledge list. Operators can add,
edit, disable, or delete knowledge through modal forms backed by `/api/agent/knowledge`. Global
documents are available to every project; project documents are isolated to their owning project.
Answers render the exact retrieved title, source identifier, and bounded excerpt as citations.
Disabled documents and documents from another project do not enter the model context. All knowledge
mutations create audit records.

The conversation toolbar creates a clean session, restores prior sessions in the selected project,
continues a session by sending its `conversationId`, and uses a two-step delete action. Sessions are
stored in SQLite, scoped to the authenticated console principal, and restored after cloud restart.
Switching projects resets the active session so messages cannot be accidentally continued under a
different project scope.

The management API exposes a fail-closed Bearer-token RBAC layer and `/api/auth/me`. In `required`
mode, viewer credentials are read-only, operators may author and publish, and only admins may delete
resources, rotate edge access tokens, or review Agent proposals. The console authenticates before loading data, retains the
token only for the browser session, displays the effective subject/role, and supports logout. Agent
conversation ownership plus knowledge/proposal audit actors come from the authenticated principal;
request fields cannot override them. Disabled local mode remains available for the quick start.

Generated suggestions can be saved as governed proposals from the Agent chat. Each proposal has
an immutable ID, kind, risk level, optional project/edge scope, structured payload, creator, and
review lifecycle. Reviewers may approve or reject a pending proposal exactly once and must leave
an attributable audit record. Creators cannot review their own proposals, and high-risk approvals
require a non-empty review note. Approval means only that the proposal may enter the normal manual
configuration workflow; it does not create a release, publish a config, or dispatch an EdgeLink
command. Proposals and their review records survive cloud restarts in SQLite.

This boundary keeps intelligence useful without weakening deterministic runtime safety.
