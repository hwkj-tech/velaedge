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
- `AlgorithmSpec`: deterministic point-driven DSL for window, change, deadband, expression, merge, and threshold computation nodes.
- `EdgeConfigPackage`: edge-targeted configuration bundle with devices, protocol connections, MQTT uplinks, point mappings, collection tasks, algorithms, data configs, and their visual graph.

### `crates/edge-runtime`

Deterministic edge runtime:

- `ProtocolAdapter`: trait for protocol-specific telemetry collection.
- Runtime capability config: declares enabled serial collection protocols, MQTT northbound uplink support, and the local storage backend.
- `ModbusRtuAdapter`: real serial Modbus RTU register collection through `tokio-serial`.
- `Dlt645Adapter`: DL/T 645-2007 meter addressing, read-data framing, checksum validation, `+0x33` decoding, and typed BCD telemetry over the shared serial bus.
- `SimulatedProtocolAdapter`: isolated test and demo adapter.
- `LocalStore`: trait for local persistence.
- `JsonlLocalStore`: simple inspectable local store for telemetry samples.
- `RocksEdgeRuntimeStore`: desired/applied config, active version, and offline runtime state.
- `EdgeRuntime`: collection pipeline that reads telemetry, persists it, and updates `DeviceShadow`.
- `ConfiguredEdgeRuntime`: validates and applies an `EdgeConfigPackage`, schedules collection, executes chained DSL nodes, and publishes one or more MQTT outputs.

Real device protocols should be added as separate crates that implement `ProtocolAdapter`, then registered by the runtime.

### `crates/cloud-control`

Cloud control-plane primitives:

- `FleetRegistry`: stores edge node metadata.
- `ConfigPackage`: versioned deployment package for edge-specific device specs and algorithms.
- `AgentCommandDraft`: output from an Agent that becomes an `edge-core::CommandCandidate`.
- `ConfigAuthoringService`: creates cloud-side point mappings, collection tasks, and edge-targeted config packages.
- `ReleaseService`: validates config packages, records desired versions, and tracks reported edge versions.
- `SqliteCloudStore`: persists fleet, config, releases, audit, runtime status, MQTT uplinks, and discovery evidence.

This crate plans and governs. It does not execute protocol actions.

### `crates/cloud-api`

Cloud API and console hosting:

- `GET /api/summary`: fleet and release summary for the console.
- `POST /api/releases`: accepts an `EdgeConfigPackage` and creates a release through `cloud-control`.
- `GET/PUT /api/edges/{edge_id}/mqtt-uplink`: manages runtime northbound publishing to velaMQ.
- `POST /api/edges/{edge_id}/discovery/run`: validates a bounded read-only discovery job and dispatches it through the target Runtime's registered EdgeLink session. Offline runtimes return `503`, busy sessions return `409`, and command timeout returns `504`.
- `gateway`: EdgeLink session handling, online runtime registration, correlated command dispatch, and result persistence for runtime-initiated cloud connections.
- Static fallback: serves `web/console/dist` so the built React console is available from the same service.

The executable cloud service uses SQLite by default and hydrates its in-process coordination state at startup. Projects, products, point-set catalogs, product versions, edge bindings, releases, runtime state, and discovery evidence share this SQLite-backed ownership boundary.

### `web/console`

Built-in management UI:

- Dashboard, projects, products, reusable point sets, product-owned collection graphs, edge binding, runtime monitoring, releases, audit, and Agent assistant views.
- Products own reusable point-set bindings, computation graphs, MQTT outputs, and versioned edge configuration.
- Edge instances bind a product version, receive a generated enrollment token, and report runtime apply and health state.

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
4. Runtime maintains a runtime-initiated EdgeLink session; Cloud validates and dispatches a maximum 128-point discovery request over that session.
5. Runtime resolves the selected connection from the active RocksDB configuration, performs real read-only Modbus RTU register probes, and reports only observed evidence to cloud.
6. Cloud Agent converts discovery evidence into point mapping candidates for user review.
7. Cloud user maps semantic telemetry points to protocol addresses.
8. Cloud user groups point mappings into collection tasks and attaches calculation nodes.
9. Cloud user configures the MQTT northbound uplink to velaMQ.
10. Cloud creates a versioned `EdgeConfigPackage`.
11. Release validation checks references, duplicate ids, graph topology, and edge target consistency.
12. Cloud Edge Gateway sends the desired version through EdgeLink after the runtime connects.
13. Edge runtime validates locally, persists desired/applied state, and reports the applied version.
14. Cloud compares desired and reported versions and records audit events.

The cloud console owns authoring, validation, release planning, and auditability. The edge runtime owns real protocol execution, local storage, policy checks, and offline behavior.

### Management API identity boundary

The Axum management API has a route-level Bearer-token RBAC boundary that is independent from
EdgeLink device authentication. Local development defaults to `disabled`; deployment mode
`required` fails startup unless at least one unique token of 24 or more characters is configured.
Only token digests are retained in the authorization registry.

- `viewer`: GET/HEAD/OPTIONS management access.
- `operator`: viewer access plus configuration create/update, release actions, discovery, and Agent operations.
- `admin`: operator access plus DELETE operations and edge access-token generation.

Authorization executes before request extraction and handler logic, so rejected calls cannot mutate
the in-memory store, SQLite, audit log, gateway command queue, or Agent conversation state. Static
console assets remain public. The console establishes a session through `/api/auth/me`, stores the
token only in browser `sessionStorage`, and supplies it to every API request. Agent conversation
ownership and all knowledge/proposal audit actors use `ApiPrincipal.subject` in authenticated mode;
client actor fields cannot spoof ownership or attribution.

## Storage Direction

Storage is split by side:

- Cloud uses SQLite for projects, products, point sets, product bindings, fleet metadata, edge config versions, audit records, latest runtime status, MQTT uplinks, discovery evidence, and release state.
- Edge runtime uses RocksDB for desired/applied config, active runtime version, and an ordered MQTT outbox. Failed messages survive restart and are replayed in sequence. QoS 1 entries are removed after matching PUBACK and QoS 2 entries after matching PUBCOMP. Multi-broker routing is implemented per `sink_id`; `mqtt-acceptance` performs a deployment-level SUBACK, publish ACK, and payload readback check against the target velaMQ environment.
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
- `cloud-api` enables mTLS when `EDGEOPS_GATEWAY_TLS_CERT`, `EDGEOPS_GATEWAY_TLS_KEY`, and `EDGEOPS_GATEWAY_TLS_CLIENT_CA` are configured together; partial configuration fails closed.
- Runtime uses the same fail-closed rule for `--edgelink-tls-ca`, `--edgelink-tls-cert`, and `--edgelink-tls-key`; metrics, events, config deployment reports, and optional MQTT collection all remain inside that TLS session.
- The EdgeLink frame is a 4-byte big-endian length prefix followed by a versioned JSON message.
- HTTP is retained for the management console/admin API and temporary development compatibility only.
- Cloud Edge Gateway and Cloud Agent Service may run in one cloud process for the MVP, but their code responsibilities stay separate.

Each real device protocol adapter should keep low-level driver details isolated:

```text
serial protocol adapter -> normalized TelemetrySample -> edge runtime -> RocksDB outbox -> MQTT uplink -> velaMQ

The custom serial adapter is intentionally a bounded frame DSL rather than a script runtime. Cloud validates the request frame, checksum policy, response prefix, value offset/length, encoding, and scale before publishing; Runtime validates the same contract again before any serial I/O and rejects checksum, prefix, bounds, UTF-8, or telemetry-type mismatches without emitting a sample.
                                                    -> local shadow + EdgeLink runtime status
```

Each MQTT `sink_id` is routed through its own broker connection. The runtime verifies the message broker and client identity against that route before publish, so multiple visual-graph outputs can target different brokers without silently crossing connections.

MQTT authentication uses a username plus an environment-variable reference for the password. The cloud and edge databases persist the reference, never the secret value. A runtime-local CA path may be attached to `mqtts://` sinks for private PKI; partial credential pairs and CA paths on plaintext brokers fail closed before collection starts.

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

## MQTT Deployment Gate

The release gate starts an isolated real VelaMQ node, provisions its TCP/TLS listeners and auth
records through the management API, and exercises the Runtime production MQTT client over TLS.
Acceptance requires private-CA validation, unauthenticated-client rejection, QoS acknowledgement,
and exact subscriber payload readback. Evidence is retained under
`target/velamq-acceptance-*`; broker unit mocks are not accepted as deployment evidence.

The automated serial transport gate uses a Unix PTY to exercise the production tokio-serial
factory, RAW binary mode, timeout behavior, and complete Modbus RTU frame exchange. Hardware
acceptance still verifies the USB/RS-485 adapter, line termination, direction control, configured
baud/parity, slave timing, and representative field devices.

## Agent Direction

Recommended Agent services:

- Fleet Ops Agent: health summary, capacity hints, and operational triage.
- Protocol Expert Agent: assists point-table and manual interpretation.
- Config Planning Agent: drafts config packages and rollout plans.
- Algorithm Orchestration Agent: recommends edge algorithms and input mappings.
- Maintenance Agent: explains alarms with manuals, SOPs, fault codes, and history.
- Safety Review Agent: reviews risky configuration or command changes.

All Agent outputs stay advisory until converted into governed configuration or policy-checked command candidates.
`AgentProposal` is the persisted governance envelope for configuration suggestions, point mappings,
rollout plans, and command candidates. Its terminal review transition is auditable and deliberately
has no release or command side effect. The console uses `/api/agent/proposals` to create and list
proposals and explicit `/approve` or `/reject` review endpoints; SQLite stores the proposal and audit
record in one transaction.

`AgentConversation` is the persisted dialogue envelope. It fixes the operator, optional project,
and optional edge scope at creation, retains a bounded message history with citations, and supplies
only the last 12 messages to later model calls as untrusted context. Conversation list/read/delete
operations enforce operator ownership before returning data; creation and deletion have attributable
audit records. Authenticated deployments derive that ownership from the server-injected principal;
the client field is used only by the intentionally unauthenticated local-development mode.

The cloud API contains a model-gateway adapter rather than embedding model logic in the edge runtime.
`POST /api/agent/chat` validates an optional project and edge scope, builds a bounded operational
context, and either calls an OpenAI-compatible endpoint or uses deterministic local analysis when no
provider is configured. The supplied JSON is labeled untrusted and contains fleet counts, pending
governance counts, edge identity/configuration summaries, and runtime health only. Access tokens,
MQTT credentials, certificate material, secret references, and raw protocol payloads are excluded.
Provider status is available through `GET /api/agent/provider`; the API never returns the endpoint or
API key.

The model adapter exposes no execution tools. The retrieval layer may supply manuals, point tables,
fault codes, and SOPs. The current governed retrieval layer persists global and project-scoped
`KnowledgeDocument` records in SQLite and performs bounded lexical recall over enabled documents.
It excludes cross-project records, strips lines with common secret markers, caps excerpts and result
count, and returns citations to the console. Retrieved text remains untrusted advisory context. Any
model-generated change must first become an `AgentProposal` or `AgentCommandDraft`, then pass the
existing human review, cloud validation, release, and edge policy paths. The retrieval contract can
later adopt embeddings without changing these governance boundaries.
