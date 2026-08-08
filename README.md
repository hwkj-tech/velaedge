# VelaEdge

VelaEdge is Yunliu Tech's Rust-based cloud-edge intelligence platform. It combines deterministic edge collection and computation, versioned cloud configuration, runtime governance, local storage, EdgeLink connectivity, and VelaMQ uplink. Agent intelligence is intentionally modeled as planning and governance: it can draft configuration or command candidates, but edge policy remains the final gate before any device action.

## Workspace

- `crates/edge-core`: shared domain contracts for device specs, telemetry, commands, policies, device shadows, algorithm specs, and cloud envelopes.
- `crates/edge-runtime`: deterministic edge execution, Modbus TCP/RTU, OPC UA, DL/T 645-2007, IEC 60870-5-101/104 read/write, RocksDB config state, algorithm DSL execution, scheduling, EdgeLink connectivity, and MQTT uplink.
- `crates/cloud-control`: cloud-side fleet registry, SQLite persistence, versioned config packages, internal synchronization state, and governed Agent command drafts.
- `crates/cloud-api`: Axum management API, SQLite-backed state, the runtime-initiated EdgeLink gateway, and static hosting for the management console.
- `web/console`: React/Vite console for projects, products, point sets, collection graphs, edge nodes, runtime monitoring, audit, and Agent assistance.
- `configs/edge.sample.toml`: sample edge runtime configuration.
- `configs/cloud.sample.toml`: sample cloud control-plane configuration.
- `deploy/systemd` and `deploy/env`: production process units and environment templates.
- `deploy/modbus-device`: independent containerized Modbus TCP slave for protocol-level
  integration and Runtime acceptance.
- `deploy/industrial-device-lab`: stateful Siemens S7, Omron FINS, IEC 104 and BACnet/IP
  TCP/UDP container devices whose addresses match the built-in manufacturer product templates.
- `docs/architecture.md`: architecture notes and extension guidance.
- `docs/cloud-console.md`: cloud console workflow and Agent safety boundary.

## Quick Start

Run the tests:

```bash
cargo test --workspace
```

Run the consolidated local release gate (Rust, console unit tests, real-browser console workflow,
EdgeLink mTLS, certificate lifecycle, SQLite recovery, performance, and optional real VelaMQ):

```bash
VELAMQ_REPO=/path/to/velamq-rs scripts/run-release-gates.sh
```

The local release gate always runs a controlled field-input preflight using the repository fixture.
It validates package shape, serial binding, QoS 1 routing, and both EdgeLink certificate chains while
recording `physicalDeviceExercised: false`. The strict `site` profile consumes a validated site
campaign plan bound to retained evidence from real target devices and fails closed unless every required
protocol satisfies the versioned per-protocol manufacturer/model 24-hour evidence matrix in
`deploy/field-acceptance-policy.json`.

Use `EDGEOPS_RELEASE_PROFILE=site`, `EDGEOPS_FIELD_CAMPAIGN_PLAN`, and the broker source path from
[`docs/field-acceptance.md`](docs/field-acceptance.md) for the fail-closed physical sign-off.

Run one local simulated edge collection cycle:

```bash
cargo run -p edge-runtime -- --allow-simulated \
  --edge-id edge-dev --device-id pump-1 --storage data/telemetry.jsonl
```

This command starts the real Rust Runtime and executes the normal collection, local persistence,
calculation, EdgeLink, and MQTT code paths. Only the southbound device values come from the
time-varying simulated protocol adapter. Runtime heartbeats sample host CPU, memory, filesystem
usage, and process uptime from the operating system; they are not fixed console fixtures. A
physical serial-port run is still required for field acceptance.

For a protocol-level integration run with an independently implemented device, start the
containerized Modbus TCP slave:

```bash
docker compose -f deploy/modbus-device/compose.yaml up -d --build
docker compose -f deploy/modbus-device/compose.yaml ps
```

The container runs a standard Modbus server library and changes pump pressure, flow, and coil state
over time. The Rust Runtime is not simulated: it connects to the container over a real TCP socket,
uses Modbus functions 01/03, and follows the same automatically synchronized configuration path used for field
devices. The repository's Rust `modbus-tcp-simulator` remains available for adapter-focused tests,
but it is not required by this containerized run.

Bind a cloud product connection to `tcp://127.0.0.1:1502`, save it for an enrolled Runtime, and
configure an MQTT sink. The Runtime then reads the socket through the production Modbus TCP adapter,
executes the active calculation graph, persists its RocksDB outbox, and publishes to the selected
broker. Capture the resulting messages independently with:

```bash
cargo run -p edge-runtime --bin mqtt-capture -- \
  --host 127.0.0.1 --port 1883 \
  --topic 'factory/+/pump/+/status' --count 5
```

This is real process, network, protocol, configuration, storage, and broker integration evidence.
The register values are generated by a software Modbus device, so it remains laboratory evidence
rather than physical-device acceptance.

The repeatable Modbus TCP laboratory gate builds an independently implemented Modbus device
container, runs the production Runtime adapter against its real TCP socket, interrupts and restarts
the device to verify recovery, and retains a machine-readable evidence report. The default run uses
an in-process recording MQTT publisher and reports that no broker was exercised. Point it at an
actual broker to require QoS 1 PUBACK evidence and a drained RocksDB outbox:

```bash
scripts/run-lab-modbus-tcp-acceptance.sh

EDGEOPS_MODBUS_TCP_LAB_MQTT_BROKER=mqtt://127.0.0.1:1883 \
scripts/run-lab-modbus-tcp-acceptance.sh
```

Start the independent Siemens S7, Omron FINS, IEC 104 and BACnet/IP device lab and exercise it with
the production Runtime adapters:

```bash
docker compose -f deploy/industrial-device-lab/compose.yaml up -d --build --wait
scripts/run-container-protocol-device-acceptance.sh

# Air-gapped rerun after the image has been built once. The report records the exact image ID.
EDGEOPS_CONTAINER_PROTOCOL_NO_BUILD=1 scripts/run-container-protocol-device-acceptance.sh
```

The persistent lab endpoints are S7 at `127.0.0.1:11102`, FINS TCP/UDP at
`127.0.0.1:19600`, IEC 104 at `127.0.0.1:12404`, and BACnet/IP at
`127.0.0.1:14780/udp`. All four devices expose changing telemetry and writable command points. See
[`deploy/industrial-device-lab/README.md`](deploy/industrial-device-lab/README.md) for the exact
point map and port overrides. This proves protocol-level integration, not physical PLC
interoperability.

### Three-protocol console demo

With the local Cloud, enrolled Runtime, and MQTT broker running, create the complete Modbus TCP,
Siemens S7, and Omron FINS demo product with one idempotent command:

```bash
docker compose -f deploy/modbus-device/compose.yaml up -d --build --wait
docker compose -f deploy/industrial-device-lab/compose.yaml up -d --build --wait
VELAEDGE_API_BASE=http://127.0.0.1:8082 scripts/bootstrap-industrial-demo.sh
```

The manifest in `deploy/demo/industrial-line-demo.json` creates project `industrial-demo-lab`,
product `industrial-line-demo`, three reusable point sets, 13 point mappings, three collection
flows, four calculation rules, seven MQTT outputs, and three command flows. Product version
`v2.1.0` is published and bound to `edge-draft-1`; the bootstrap script verifies the materialized
Runtime package before returning `status: ready`.

Inspect changing QoS 1 messages from all three protocols:

```bash
cargo run -q -p edge-runtime --bin mqtt-capture -- \
  --topic 'factory/edge-draft-1/#' --count 10 --timeout-seconds 15
```

Exercise a real MQTT-to-Modbus write with protocol readback verification:

```bash
cargo run -q -p edge-runtime --bin mqtt-command -- \
  --topic 'factory/edge-draft-1/commands/modbus' \
  --reply-topic 'factory/edge-draft-1/commands/reply/demo-modbus-stop' \
  --payload '{"commandId":"demo-modbus-stop","values":{"modbus_running":false}}'
```

Equivalent command topics are `factory/edge-draft-1/commands/s7` and
`factory/edge-draft-1/commands/fins`. Their writable fields are `s7_start_command` and
`fins_start_command`. The Runtime health console is available at `http://127.0.0.1:19090/`; the
management console for this local stack is at `http://127.0.0.1:8082/`.

For an active Siemens, Omron, OPC UA, IEC 104, BACnet/IP, Modbus or mixed product package, use the
coordinated field campaign. It establishes every broker subscription before starting the production
Runtime, preserves the package's collection periods, calculation graph, MQTT routes and protocol
settings, and writes a hash-bound evidence bundle:

```bash
cargo run --release -p edge-runtime --bin field-campaign -- \
  --config /secure/staging/site-edge-config.json \
  --output-dir /var/lib/velaedge/acceptance/vendor-a \
  --native-broker-audit /secure/evidence/velamq-delivery-audit.json \
  --native-broker-audit-wait-seconds 300 \
  --duration-seconds 86400 \
  --physical-device-exercised \
  --site-id WO-2026-0042 --operator operator-a \
  --device-connection-id s7-primary \
  --device-manufacturer Siemens --device-model S7-1500 --device-serial S7-ASSET-001
```

The output directory contains the unchanged package, Runtime report, normalized broker receipt,
native broker audit export, RocksDB state and a SHA-256 manifest. Runtime report schema v4 binds the attested physical asset to
the selected package connection, so a mixed package cannot accidentally count one device toward
multiple protocol requirements. Campaign manifest schema v3 hash-binds all four evidence artifacts;
the strict site gate rejects a missing, empty or modified native broker audit export.
`field-campaign-status` tracks an exact multi-device plan through pending, running, passed, failed,
and invalid states. The supplied systemd timer atomically refreshes the site snapshot every minute,
while the `site` release profile requires `EDGEOPS_FIELD_CAMPAIGN_PLAN` and runs the same gate with
`--require-complete` before sign-off.

See [`docs/field-acceptance.md`](docs/field-acceptance.md#generic-product-package-endurance) for
changing-point assertions, controlled recovery tests, MQTT QoS 1 requirements and evidence limits.

The short gate is laboratory evidence only. Use `field-endurance` for released product packages;
`modbus-tcp-endurance` is the smaller fixed-address Modbus pump fixture. Set
`--physical-device-exercised` only after the asset identity, wiring and work authorization have been
verified.

Run the cloud control-plane MVP:

```bash
cargo run -p cloud-control -- --edge-id edge-dev
```

Run the cloud API with the built-in management console:

```bash
cargo run -p cloud-api
```

Then open `http://127.0.0.1:8080`. The API also serves:

```text
GET  /api/summary
POST /api/releases
```

`POST /api/releases` remains a compatibility endpoint for older integrations. The built-in console
does not expose a manual publication workflow: saving a complete valid configuration creates an
internal revision and synchronizes it to an online Runtime immediately, or on its next reconnect.

The cloud process stores state in `sqlite://data/cloud-agent.sqlite` by default and also listens for runtime-initiated EdgeLink TCP sessions on `127.0.0.1:18080`. Set `EDGEOPS_HTTP_ADDR` and `EDGEOPS_GATEWAY_ADDR` to override the two listeners independently.
Set `EDGEOPS_CONSOLE_DIST` to the installed console asset directory when the Cloud binary is
deployed without the source tree. `scripts/run-deployment-smoke-acceptance.sh` verifies the provided
systemd/environment artifacts with a required-auth, empty-bootstrap, mTLS-enabled real Cloud process.

The unauthenticated local quick start uses `EDGEOPS_BOOTSTRAP_MODE=demo` implicitly so the console
has an immediately usable sample fleet. Production `required` authentication defaults to
`EDGEOPS_BOOTSTRAP_MODE=empty`; set either mode explicitly in managed deployments. Empty mode never
injects demo projects, products, edges, or configuration into a new or existing database.

Production health probes are available at `/health/live` and `/health/ready`. The readiness probe
checks SQLite and remains accessible when API authentication is required. See
[`docs/deployment.md`](docs/deployment.md) for graceful shutdown, backup/restore, certificate
rotation, and release-gate guidance.

Run `scripts/run-cloud-recovery-acceptance.sh` to exercise the real binary, public probes,
graceful SIGTERM handling, online SQLite backup, atomic restore, and post-restore startup.

Management API authentication is disabled only for the local quick start. Production deployments
should enable the fail-closed Bearer-token RBAC layer and terminate HTTPS at the service or a trusted
reverse proxy:

```bash
export EDGEOPS_API_AUTH_MODE=required
export EDGEOPS_BOOTSTRAP_MODE=empty
export EDGEOPS_VIEWER_TOKEN='replace-with-at-least-24-characters'
export EDGEOPS_OPERATOR_TOKEN='replace-with-a-different-operator-token'
export EDGEOPS_ADMIN_TOKEN='replace-with-a-different-administrator-token'
export EDGEOPS_VIEWER_SUBJECT='operations-observer'       # optional
export EDGEOPS_OPERATOR_SUBJECT='configuration-operator' # optional
export EDGEOPS_ADMIN_SUBJECT='platform-administrator'    # optional
cargo run -p cloud-api
```

At least one credential must be configured in `required` mode; short or duplicate tokens stop the
service at startup. Tokens are read from environment variables, reduced to SHA-256 digests in the
authorization registry, and never returned by the API. `viewer` can use read endpoints, `operator`
can also create/update/publish, and `admin` additionally controls deletion, edge access-token
generation, and Agent proposal review. Proposal creators cannot approve or reject their own
proposals, and high-risk approvals require an attributable review note. `GET /api/auth/me` returns
the authenticated subject and effective role. EdgeLink runtime
sessions remain on their independent mTLS plus edge-token authentication path.

The bundled console validates `/api/auth/me` before loading management data. In `required` mode it
shows a dedicated token login surface, keeps the bearer token in `sessionStorage` for the current
browser session only, injects it into every API request, displays the effective subject/role, and
supports explicit logout. In local `disabled` mode the same check returns the synthetic
`local-development` administrator and enters the console without a login prompt.

For a mutual-TLS EdgeLink listener, configure all three paths together before starting `cloud-api`:

```bash
export EDGEOPS_GATEWAY_TLS_CERT=/etc/edgeops/server.pem
export EDGEOPS_GATEWAY_TLS_KEY=/etc/edgeops/server-key.pem
export EDGEOPS_GATEWAY_TLS_CLIENT_CA=/etc/edgeops/runtime-ca.pem
```

Leaving all three unset retains the plaintext listener for local development. A partial TLS configuration is rejected at startup.

Run one runtime-to-cloud EdgeLink synchronization cycle:

```bash
cargo run -p edge-runtime -- \
  --edge-id edge-dev \
  --runtime-id runtime-dev \
  --runtime-db data/edge-runtime.rocksdb \
  --cloud-gateway-addr 127.0.0.1:18080
```

Run the production-style runtime-initiated session loop so Cloud can dispatch bounded discovery
commands over the same connection (the Runtime never opens an inbound HTTP server):

```bash
cargo run -p edge-runtime -- \
  --edge-id edge-dev \
  --runtime-id runtime-dev \
  --runtime-db data/edge-runtime.rocksdb \
  --cloud-gateway-addr 127.0.0.1:18080 \
  --edgelink-daemon \
  --edgelink-command-wait-ms 30000 \
  --edgelink-reconnect-ms 1000
```

For mTLS, add `--edgelink-tls-ca`, `--edgelink-tls-cert`, `--edgelink-tls-key`, and optionally `--edgelink-tls-server-name`. The three certificate paths are atomic: Runtime rejects partial TLS configuration instead of falling back to plaintext. Use `--access-token-env EDGEOPS_EDGE_TOKEN` to read the one-time edge access token from an environment variable without exposing it in the process command line; the legacy `--access-token` option remains available for local compatibility and conflicts with `--access-token-env`.

### EdgeLink mTLS process acceptance

The isolated process harness starts the real `cloud-api` binary with a temporary SQLite database and mTLS listener, then exercises the real `edge-runtime` binary with a temporary RocksDB store. It proves that a certificate-authenticated runtime without an edge access token is rejected, while an authorized runtime receives the pending configuration, applies it locally, reports the applied version, registers capabilities, and publishes runtime metrics. Southbound health is intentionally not required by this transport gate: unavailable devices must be reported as degraded or critical instead of being fabricated as healthy. Protocol-specific gates verify successful collection separately:

```bash
scripts/run-edgelink-mtls-acceptance.sh
```

The bundled certificates are test fixtures only. The harness does not modify the service on ports `8080` and `18080`; it uses `18081` and `18082` by default and retains `report.json`, SQLite, RocksDB, runtime logs, and rejection evidence under `target/edgelink-acceptance-*`. Override the two acceptance ports with `EDGELINK_ACCEPTANCE_HTTP_PORT` and `EDGELINK_ACCEPTANCE_GATEWAY_PORT` when needed. Production certificate releases are validated and atomically activated with `scripts/edgelink-certificates.sh`; run `scripts/run-certificate-lifecycle-acceptance.sh` to verify rotation, rollback, expiry rejection, and private-key matching on the deployment host.

Run the console in Vite development mode:

```bash
cd web/console
npm install
npm run dev
```

Projects, products, point sets, product versions and bindings, fleet nodes, edge config packages,
internal synchronization revisions, Agent proposals, audit records, runtime metrics/events, MQTT uplinks, and discovery reports
are persisted in SQLite.

## Design Direction

The first implementation keeps the control path simple and auditable:

1. Device semantics live in `edge-core` as `DeviceSpec`, telemetry points, command specs, algorithm descriptors, and edge-facing config packages.
2. Cloud users author device models, protocol connections, point mappings, collection tasks, and versioned product configuration in the console. Saving a complete valid change automatically synchronizes the newest revision; incomplete configuration remains editable and is not sent to Runtime.
3. Edge collection happens through `ProtocolAdapter`. Modbus RTU/TCP, OPC UA batch Read/Subscription and explicit typed Write with persistent sessions and bounded Browse discovery, BACnet/IP discovery and ReadPropertyMultiple, Siemens S7 TCP, Omron FINS over UDP/TCP, DL/T 645-2007, IEC 60870-5-101/104 read/write, and the governed custom-serial frame DSL are implemented; 24-hour hardware interoperability remains extension work. BACnet points use `device_instance:object_type:object_instance:property[:array_index]`, for example `42:analog_input:0:present_value`; Runtime keeps a B/IP UDP session, discovers unknown device addresses with Who-Is, batches reads with ReadPropertyMultiple, and falls back to individual reads when needed. OPC UA points use standard NodeIds such as `ns=2;s=Line1.Pressure`; connection settings cover security policy, message mode, anonymous/username/X.509 authentication, PKI trust, and bounded timeouts. Browse discovery accepts a root NodeId and depth limit, follows continuation points, excludes namespace 0 by default, and returns only readable scalar variables with inferred types and live samples. Writable OPC UA points require an exact Built-in Type; Runtime uses the standard Write service and command flows may request Read-back verification. Siemens S7 points use DB/M/I/Q notation such as `DB1.REAL4` or `M10.3`. Omron FINS points use CIO/WR/HR/DM/AR notation such as `CIO0.5` or `DM100`, with UDP/TCP transport selection, FINS/TCP node handshake, connection-level network/node/unit routing and configurable two-word order. DL/T 645 point addresses use `meter_address:data_identifier[:decimal_places]`, for example `123456789012:02010100:1`; Cloud and Console expose one shared common-DI catalog, while Runtime de-duplicates identical reads and serializes multi-meter polling on one RS-485 bus. IEC 101 point addresses use `link_address:common_address:ioa`, for example `1:2:1001`; the adapter uses one-byte link addresses, two-byte common addresses, three-byte IOAs, FT1.2 link reset/confirmed data/class-2 polling, common single/double-point, normalized/scaled/float telemetry with CP24/CP56 time tags, and confirmed single/double/short-float controls. IEC 104 points use `common_address:ioa`, for example `1:1001`; the adapter performs STARTDT, persistent-session general interrogation, sequence acknowledgement, quality/timestamp mapping, and reconnects after bounded transport failures. Both IEC 101 and IEC 104 connection settings expose `cp56TimeZoneOffsetMinutes`, which interprets a station's fixed local CP56Time2a clock before Runtime normalizes it to UTC (`480` for China Standard Time). Explicitly writable points bind `C_SC_NA_1`, `C_DC_NA_1` or `C_SE_NC_1`, optionally require select-before-operate, and only succeed after a matching positive activation confirmation. Custom serial points use `custom_serial_frame` addresses whose JSON DSL limits execution to a request HEX frame, SUM8/XOR8/Modbus CRC16 checksums, an optional response prefix, a bounded value range, a typed endian-aware decoder, and numeric scale/offset. It cannot execute scripts, loops, or arbitrary code. MQTT is a northbound sink, not a southbound collection protocol.
   Custom serial JSON without `schemaVersion` remains v1 Raw. New v2 configurations additionally support SLIP/COBS framing and CRC-16/CCITT-FALSE; checksums cover the decoded payload, so Runtime frames requests after checksum generation and decodes responses before checksum validation.
4. Edge desired/applied config and offline runtime state use RocksDB. JSONL remains available as an inspectable telemetry development adapter behind `LocalStore`.
5. Agent suggestions are persisted as `AgentProposal` records with one-time human approval or rejection and audit attribution. Review alone never mutates or synchronizes Runtime configuration. Device command drafts remain `AgentCommandDraft` values and become `CommandCandidate` values only through deterministic governance and policy checks.
6. Runtime system health is sampled in-process from the host and reported over the runtime-initiated EdgeLink session. Simulated point generation never substitutes fixed CPU, memory, disk, or uptime values.

The Agent chat backend supports a configurable OpenAI-compatible model gateway. Without model
configuration it stays in deterministic local-analysis mode, so the console remains usable without
an external dependency. Configure the provider only in the cloud process environment:

```bash
export EDGEOPS_AGENT_ENDPOINT='https://model-gateway.example/v1/chat/completions'
export EDGEOPS_AGENT_MODEL='your-approved-model'
export EDGEOPS_AGENT_API_KEY='replace-at-deployment' # optional for private gateways
export EDGEOPS_AGENT_TIMEOUT_MS=15000                # optional, 1000..120000
```

`GET /api/agent/provider` reports the active mode without exposing credentials. `POST
/api/agent/chat` accepts a question and optional project/edge scope, then sends only a bounded,
secret-free operational summary to the model. Model output is advisory text: it cannot apply or
synchronize configuration, dispatch EdgeLink commands, write device registers, or bypass proposal review.

The governed knowledge API (`/api/agent/knowledge`) stores global or project-scoped manuals, SOPs,
fault-code notes, and point-table guidance in SQLite. Chat performs bounded lexical retrieval over
enabled documents, excludes documents from other projects, removes lines containing common secret
markers, and returns document citations with every answer. Knowledge create/update/delete actions
are audited. Retrieval content is always labeled as untrusted context and never becomes executable
configuration.

Agent conversations are also cloud-owned records. `POST /api/agent/chat` accepts an optional
`conversationId`, while `GET /api/agent/conversations` lists only the authenticated principal's
sessions in the selected project. Request actor fields remain wire-compatible for local mode but
cannot override the server principal when authentication is enabled. The last 12 messages are supplied as bounded,
untrusted context; SQLite retains at most 200 messages per conversation. Creation and deletion are
audited, cross-operator reads return `404`, and conversation project/edge scope cannot be changed
after the first message.

## Active Delivery Milestones

- Keep project, product, point-set, version, automatic synchronization, exact-version acknowledgement, and failed-apply retention workflows covered by acceptance tests.
- Keep deployment-level VelaMQ acceptance in the release gate. The isolated harness now passes against the real broker with a private CA, authenticated MQTT over TLS, QoS 1 acknowledgement, exact subscriber readback, and unauthenticated-client rejection. Runtime multi-broker routing assigns each `sink_id` an independent connection and publishes through the ordered, broker-acknowledged RocksDB outbox.
- Validate the completed bounded `DiscoveryRequest` EdgeLink path against real hardware fixtures. Cloud dispatches only to an online Runtime session, Runtime resolves the connection from its active RocksDB config, performs read-only Modbus RTU probes, and returns correlated reports or a deterministic failure. Offline runtimes return `503`; no simulated evidence is fabricated.
- Keep the configurable OpenAI-compatible Agent gateway, deterministic fallback, project-scoped
  governed knowledge retrieval, operator/project-scoped persistent conversations, bounded context,
  citations, proposal review, audit, and edge policy gates covered by acceptance tests.
- Keep the completed certificate lifecycle, disaster recovery, deployment runbook, console sign-in,
  RBAC, Agent principal-propagation, and repeatable Cloud/Runtime performance gates in the release
  gate. The field-hardware harness now enforces serial, mTLS, MQTT QoS 1, config ACK, and evidence
  requirements; production completion still requires running it against the target physical device.

### MQTT / velaMQ deployment acceptance

The runtime includes a broker acceptance command that uses the production MQTT publisher. It
creates an isolated subscription, waits for SUBACK, publishes a JSON probe with the requested QoS,
waits for the broker acknowledgement, and verifies the exact payload through subscriber readback:

```bash
cargo run -p edge-runtime --bin mqtt-acceptance -- \
  --broker mqtt://127.0.0.1:1883 \
  --client-id-prefix edgeops-site-acceptance \
  --qos 1
```

For an authenticated deployment with a private CA, keep the password only in the runtime process
environment and pass its variable name through configuration:

```bash
export EDGEOPS_MQTT_PASSWORD='replace-at-deployment'
cargo run -p edge-runtime --bin mqtt-acceptance -- \
  --broker mqtts://velamq.example:8883 \
  --client-id-prefix edgeops-site-acceptance \
  --username edge-device \
  --password-env EDGEOPS_MQTT_PASSWORD \
  --tls-ca-path /etc/edgeops/velamq-ca.pem \
  --qos 1
```

Use `mqtts://` for a broker certificate trusted by the host. A successful run prints a JSON report
with `payload_verified: true` and exits with status 0; subscription rejection, missing QoS ACK,
payload mismatch, and timeout all exit non-zero. This repository does not bundle a VelaMQ broker,
so deployment acceptance runs against the target checkout or environment and retains the JSON
report as release evidence.

The cloud configuration stores only `username`, `password_env`, and the runtime-local
`tls_ca_path`. The password value is never serialized into SQLite, config packages, RocksDB,
the MQTT outbox, API responses, or audit records.

For a repeatable acceptance run against a local VelaMQ source checkout, use the isolated harness.
It starts a temporary single-node VelaMQ with independent Raft/RocksDB/API/MQTT ports, provisions
TCP and TLS listeners through the real management API, verifies that unauthenticated MQTT is
rejected, then runs the production Runtime publisher over TLS with QoS 1 and payload readback:

```bash
VELAMQ_REPO=/path/to/velamq-rs scripts/run-real-velamq-acceptance.sh
```

The harness does not modify or stop an existing VelaMQ instance. It retains `report.json`, the
generated CA, rejection evidence, and broker logs under `target/velamq-acceptance-*`.
The latest verified local run completed an authenticated TLS/QoS 1 round trip with exact payload
readback in 4 ms; release pipelines should run the harness again rather than reuse old evidence.

On Unix, `scripts/run-lab-serial-acceptance.sh` exercises Modbus RTU, DL/T 645, and IEC 101 through
the production `TokioSerialBusFactory` against OS pseudo terminals. Each case verifies the request
frame at the device side, production checksum/decoding, Runtime data-configuration execution, JSON
payload construction, and a QoS 1 MQTT PUBACK. The command retains `report.json` and the test log
under `target/serial-lab-acceptance-*`. Physical RS-485 wiring, adapter direction control,
non-zero baud-rate ioctls, and representative site devices remain physical acceptance checks.
