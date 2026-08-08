# Physical Field Acceptance

This procedure verifies the production path on the target edge host: a real serial request reaches
the connected device, Runtime executes the released collection graph, the result is acknowledged by
the configured MQTT broker, and Cloud records the applied configuration and Runtime health. A PTY,
`/dev/null`, a simulated adapter, or configuration-only validation is useful preflight evidence but
is not physical acceptance.

Before travelling to site, run `scripts/run-lab-serial-acceptance.sh`. It produces repeatable PTY
evidence for Modbus RTU, DL/T 645, and IEC 101 through the production serial factory, Runtime data
configuration, JSON payload builder, and MQTT QoS 1 acknowledgement path. A passing report removes
software framing and integration uncertainty, but its `physicalDeviceExercised` field remains
`false` and it cannot be used as site sign-off.

Run the complete protocol matrix before protocol-specific laboratory or field work:

```bash
scripts/run-protocol-matrix-acceptance.sh
```

The matrix executes every Runtime industrial protocol in an isolated test target and writes one
machine-readable result per protocol, including capability assertions, test count, transport
evidence, source digest and log. It currently covers Modbus TCP/RTU, OPC UA, DL/T 645, IEC 101/104,
BACnet/IP, Siemens S7, Omron FINS over UDP/TCP and the custom serial frame DSL. TCP/UDP loopback servers and
operating-system PTYs exercise the production adapters, but the report deliberately records
`physicalDeviceExercised: false`. Use
`EDGEOPS_PROTOCOL_MATRIX_FILTER=opc-ua-client,siemens-s7` to repeat a subset while diagnosing a
failure. The full matrix is mandatory in `scripts/run-release-gates.sh`.

Ethernet collection can be exercised locally with the independent containerized Modbus device on
port `1502`:

```bash
docker compose -f deploy/modbus-device/compose.yaml up -d --build
```

Unlike an in-process mock, it makes the production Runtime open a real TCP connection to a separate
Modbus implementation, decode Modbus frames, execute the released graph, and publish through the
configured MQTT broker. The repository's `modbus-tcp-simulator` remains useful for adapter-focused
tests. Both are laboratory evidence, and a software-generated register source still means
`physicalDeviceExercised: false`.

Run `scripts/run-lab-modbus-tcp-acceptance.sh` to build and start the independent device, exercise
the production adapter, stop and restart the device during collection, and retain test, Runtime and
container logs plus a machine-readable report. The report records collection success/failure and
latency, changing values, reconnect/circuit/quality counters, recovery after the injected outage,
and the explicit physical-device limitation. Its default recording publisher is useful for graph
verification but is not broker evidence. Supply a real broker to require QoS 1 PUBACK receipts and
an empty RocksDB outbox:

```bash
EDGEOPS_MODBUS_TCP_LAB_MQTT_BROKER=mqtt://127.0.0.1:1883 \
  scripts/run-lab-modbus-tcp-acceptance.sh
```

For a target Modbus TCP asset, run the same production path for 24 hours and retain the generated
report. The physical flag is an operator assertion and must never be used for the container or an
in-process simulator:

```bash
cargo run -p edge-runtime --bin modbus-tcp-endurance -- \
  --endpoint 10.20.30.40:502 \
  --duration-seconds 86400 \
  --interval-ms 1000 \
  --minimum-cycles 85000 \
  --maximum-failure-ratio 0.01 \
  --require-recovery \
  --physical-device-exercised \
  --mqtt-broker mqtts://broker.example.internal:8883 \
  --mqtt-version 5.0 \
  --mqtt-username velaedge-runtime \
  --mqtt-password-env VELAEDGE_MQTT_PASSWORD \
  --mqtt-ca-path /etc/velaedge/runtime/broker-ca.pem \
  --rocksdb-path /var/lib/velaedge/acceptance/modbus-tcp.rocksdb \
  --report /var/lib/velaedge/acceptance/modbus-tcp-24h.json
```

`--require-recovery` is appropriate only when the approved test plan includes a controlled endpoint
or network interruption. Without such an interruption, omit it rather than fabricating recovery
evidence. The endurance report proves Runtime PUBACK processing; site sign-off still requires the
broker-side consumer or audit receipt described below.

### Generic product-package endurance

`field-endurance` runs an exported `EdgeConfigPackage` without replacing its protocol connection,
point mapping, collection period, calculation graph, MQTT sink or topic. It uses the same
`ConfiguredEdgeRuntime`, production serial/TCP/UDP adapters, persistent multi-broker MQTT publisher
and RocksDB outbox as the deployed Runtime. This is the preferred 24-hour runner for IEC 101/104,
OPC UA, BACnet/IP, Siemens S7, Omron FINS and mixed-product packages; the older
`modbus-tcp-endurance` command remains a convenient fixed-address pump fixture.

First execute a short laboratory or commissioning run. MQTT may be skipped only for this preflight;
the report will carry an explicit broker-evidence limitation:

```bash
cargo run -p edge-runtime --bin field-endurance -- \
  --config /secure/staging/site-edge-config.json \
  --duration-seconds 60 \
  --scheduler-interval-ms 100 \
  --minimum-cycles 20 \
  --maximum-failure-ratio 0.01 \
  --maximum-progress-gap-seconds 300 \
  --require-changing-point pump-1/pressure \
  --skip-mqtt \
  --report /secure/evidence/preflight/report.json
```

For physical sign-off, retain the released package unchanged, make its MQTT password environment
variables available to the process, and provide a complete operator-attested asset identity. Stop
every other publisher that uses the same `edge_id` and `config_version`, then run the coordinated
campaign from a new, empty evidence directory:

```bash
cargo run --release -p edge-runtime --bin field-campaign -- \
  --config /secure/staging/site-edge-config.json \
  --output-dir /var/lib/velaedge/acceptance/iec104-vendor-a \
  --native-broker-audit /secure/evidence/velamq-iec104-vendor-a-audit.json \
  --native-broker-audit-wait-seconds 300 \
  --duration-seconds 86400 \
  --scheduler-interval-ms 100 \
  --maximum-failure-ratio 0.01 \
  --maximum-progress-gap-seconds 300 \
  --require-changing-point plc-1/line_speed \
  --physical-device-exercised \
  --site-id WO-2026-0042 \
  --operator operator-a \
  --device-connection-id iec104-primary \
  --device-manufacturer Vendor-A \
  --device-model RTU-104 \
  --device-serial RTU-ASSET-001
```

For a managed field host, use the equivalent checked launcher instead of maintaining this argument
list manually. Install `deploy/systemd/edgeops-field-campaign@.service`, populate one copy of
`deploy/env/field-campaign.env.example` for the exact asset, then start the matching service
instance. `scripts/run-field-campaign.sh` rejects relative evidence paths, missing package files,
invalid numeric limits, incomplete physical identity, and any run without explicit physical-device
confirmation. It preserves MQTT password values only in the protected environment file and passes
their variable names through the released package. The service's `ExecStartPre` invokes
`run-field-campaign --preflight-only`; the released binary validates the production Runtime graph,
selected physical connection, expanded MQTT output routes, credential variables, CA files and fresh
artifact paths without opening protocol or MQTT sessions and without creating the evidence
directory. A failed preflight therefore cannot consume or contaminate a 24-hour evidence window.

The campaign derives every enabled visual-graph output Topic from the package, opens one consumer
per MQTT sink, and waits for every exact Topic SUBACK before starting the production Runtime. It
supports MQTT 3.1.1 and 5.0 with the package's authentication and TLS settings and ignores retained
messages, payloads for another edge/version, and MQTT DUP retransmissions. After Runtime stops, the
consumer remains active for the configured 60-second drain grace only when Runtime recorded at
least one successful publish; a startup or no-progress failure closes the receipt session
immediately. The new evidence directory then
contains the exact `configuration-package.json`, `runtime-report.json`, `broker-receipt.json`,
`native-broker-audit.json`, `manifest.json`, and Runtime RocksDB state. The schema v3 manifest binds
all four evidence artifacts by SHA-256, records
the failed phase when applicable, and passes only when Runtime's successful publish count equals the
broker's live delivery count. `--native-broker-audit` points to a schema v1 structured audit exported
by VelaMQ or the target broker adapter. The path may be absent when the command starts: after the
Runtime and consumer receipt window close, the campaign enters `native_broker_audit`, updates its
atomic `manifest.json`, and waits up to `--native-broker-audit-wait-seconds` (300 seconds by default)
for a non-empty export only after a valid, non-empty consumer receipt exists. Without such a receipt,
the campaign records why the audit was skipped and fails immediately instead of consuming the audit
timeout. This lets site automation export an audit that covers the completed window
without racing the campaign process. The campaign then copies its exact bytes and rejects an audit
that does not match this campaign's edge, version, package digest, time window, message count,
consumer routes, and Topics.

The campaign installs `SIGINT` and `SIGTERM` handlers before MQTT subscription readiness. An
operator interrupt or service-manager shutdown during subscription setup, Runtime endurance,
receipt drain, or native-audit wait stops the active phase, closes the receipt session, preserves
any completed artifacts, and atomically marks `manifest.json` as `failed` with phase `interrupted`.
An interrupted directory is diagnostic evidence only and is always rejected by the site gate.

A physical campaign rejects `Simulated`, missing identity fields, and MQTT sinks that do not use QoS
1. It also fails unless all configured points were observed, required changing points changed, all
used protocol connections finish connected, every used protocol records continuous successful
collection activity, every MQTT sink continuously receives PUBACK, and the outbox is empty. The
configured maximum progress gap covers startup, intervals between counter increases, and the final
tail; a counter reset or any longer collection/publish gap fails the report. Use standalone
`field-mqtt-receipt` and `field-endurance` only when diagnosing the broker or Runtime separately;
the coordinated campaign is the normal evidence-producing path.

The configured duration is a hard upper bound, not a condition that can be extended indefinitely to
chase a missing minimum-cycle count. During the window, the Runtime also stops early when any used
protocol connection or MQTT sink has no successful progress for longer than the configured maximum
gap. The failed report retains the stalled connection or sink, observed gap, protocol metrics,
outbox state and recent errors, so an unreachable device or Broker is diagnosed before an entire
24-hour slot is wasted.

Point selectors use `device_id/point_id`, so identically named points on different PLCs remain
unambiguous. The default minimum-cycle criterion is 90% of the cycles implied by the shortest
enabled product collection period; pass `--minimum-cycles` when the approved work order requires a
stricter count. `--require-recovery` must only accompany a recorded, approved outage injection.
The JSON report contains configuration SHA-256, actual duration, per-point first/last/distinct
values, p95 cycle latency, per-protocol collection/write attempt and success counters,
quality/circuit/reconnect counters, per-sink MQTT status, retained acknowledgement count and outbox
state. The interoperability gate rejects a report that is connected only at the final sample but
has no successful collection activity. Runtime PUBACK is not a broker-side consumer receipt.
`field-campaign` writes the normalized receipt atomically only after every configured output Topic
has delivered data. It also parses and hash-binds VelaMQ's structured native audit into the campaign
manifest. The multi-vendor gate validates both artifacts: the subscriber receipt proves end-consumer
delivery, while the Broker audit independently identifies the Broker instance and audit record.

### Campaign inventory preflight

Before installing systemd instances, describe every physical asset campaign in one schema v1 JSON
plan and validate it against the same deployment policy used by the final interoperability gate:

```json
{
  "schemaVersion": 1,
  "siteId": "WO-2026-0042",
  "physicalDeviceConfirmed": true,
  "campaigns": [
    {
      "campaignId": "modbus-tcp-vendor-a-pump-100",
      "operator": "operator-a",
      "configPath": "/var/lib/edgeops-field-campaign/modbus-a/input/configuration-package.json",
      "outputDir": "/var/lib/edgeops-field-campaign/modbus-a/evidence-run-001",
      "nativeBrokerAuditPath": "/var/lib/edgeops-field-campaign/modbus-a/inbox/audit-run-001.json",
      "physicalDevice": {
        "connectionId": "modbus-main",
        "manufacturer": "Vendor A",
        "model": "Pump 100",
        "serialNumber": "ASSET-001"
      },
      "durationSeconds": 86400,
      "maximumFailureRatio": 0.01,
      "maximumProgressGapSeconds": 300,
      "changingPoints": ["pump-1/pressure"]
    }
  ]
}
```

Run the no-I/O plan gate on the exact field host after package, CA and MQTT secret environment
provisioning, but before starting any campaign:

```bash
cargo run --release -p edge-runtime --bin field-campaign-plan -- \
  --plan /etc/edgeops/field-campaign/site-plan.json \
  --policy /opt/edgeops/deploy/field-acceptance-policy.json \
  --output /var/lib/edgeops-field-campaign/site-plan-report.json
```

The report binds plan and policy by SHA-256, resolves each package's selected physical connection
and expanded multi-output MQTT routes, validates Runtime and scheduler construction, and checks
referenced MQTT credential variables and CA files. It rejects simulated or unused connections,
non-QoS-1 routes, weaker thresholds than policy, stale evidence or audit paths, duplicate campaign
IDs, physical identities, edge IDs, MQTT client IDs, RocksDB paths, and nested evidence directories.
Only fully valid campaigns count toward each protocol's manufacturer/model requirement. Each row
contains the exact non-secret systemd environment and a `requiredSecretEnvironment` list; secret
values are never copied into the report. The command does not create evidence directories or open
protocol/MQTT sessions.

### Site campaign status

Use the same plan and policy to observe the whole site's execution state while campaigns are
running. The status command is read-only: it does not open southbound or MQTT sessions and does not
read Runtime password values or CA file contents. It reports every campaign as `pending`, `running`,
`passed`, `failed`, or `invalid` and always writes a machine-readable snapshot:

```bash
cargo run --release -p edge-runtime --bin field-campaign-status -- \
  --plan /etc/edgeops/field-campaign/site-plan.json \
  --policy /opt/edgeops/deploy/field-acceptance-policy.json \
  --output /var/lib/edgeops-field-campaign/site-status.json
```

Pending and running campaigns return success so monitoring jobs can refresh the snapshot without
raising a deployment incident. Failed or invalid campaigns return unsuccessfully. Use
`--require-complete` in the final sign-off job; it also returns unsuccessfully until every planned
campaign has passed and the complete protocol policy is satisfied.

For completed campaigns, the command verifies the schema v3 manifest and the SHA-256-bound
configuration package, Runtime report, Broker consumer receipt, and native Broker audit. Accepted
evidence must match the plan's site, operator, physical connection, manufacturer, model, serial
number, edge ID, configuration version, and package digest. Therefore a valid 24-hour result cannot
be substituted under another planned asset or a changed package.

For managed field hosts, install
`deploy/systemd/edgeops-field-campaign-status.service` and its timer. The timer refreshes
`/var/lib/edgeops-field-campaign/site-status.json` every minute. The JSON file is replaced
atomically and is safe for a local dashboard or monitoring collector to read while a refresh is in
progress. The periodic service runs the observation form without `--require-complete`; the `site`
release profile applies the strict completion flag using `EDGEOPS_FIELD_CAMPAIGN_PLAN`.

### Multi-vendor interoperability gate

Run `field-campaign` once per physical vendor device and required protocol, then retain each complete
campaign directory. A campaign package may contain multiple protocol connections, but
`physicalDevice.connectionId` binds the attested asset to exactly one connection. The gate counts
only that connection's protocol and verifies it against the original configuration package. Run
separate campaigns for other physical devices or protocol connections.
The interoperability gate consumes those directories, verifies their manifests and SHA-256-bound
configuration package, Runtime report, broker receipt and native broker audit, and prevents a
short laboratory run, a software simulator, duplicate report bytes, or repeated use of one physical
device from being counted as coverage. Without `--policy`, the compatibility default requires
DL/T 645-2007, IEC 101, IEC 104 and OPC UA to each have passing 24-hour evidence from at least two
distinct manufacturers. DL/T 645 reports
may use the Runtime label `DL/T645`, the catalog slug `dlt645-2007`, or the standard display name;
the gate normalizes all three to one protocol identity:

```bash
cargo run --release -p edge-runtime --bin field-interoperability-gate -- \
  --campaign-dir /retained/dlt645/vendor-a \
  --campaign-dir /retained/dlt645/vendor-b \
  --campaign-dir /retained/iec101/vendor-a \
  --campaign-dir /retained/iec101/vendor-b \
  --campaign-dir /retained/iec104/vendor-a \
  --campaign-dir /retained/iec104/vendor-b \
  --campaign-dir /retained/opcua/vendor-c \
  --campaign-dir /retained/opcua/vendor-d \
  --output /retained/interoperability/dlt645-iec101-iec104-opcua-matrix.json
```

Production site sign-off uses the versioned policy in
[`deploy/field-acceptance-policy.json`](../deploy/field-acceptance-policy.json). It covers every
non-simulated southbound protocol and defines manufacturer and model thresholds separately. This is
important for vendor-specific protocols: Siemens S7 and Omron FINS each require two distinct PLC
models from their single protocol vendor, while Modbus and BACnet/IP can begin with one identified
physical model. Run the gate directly with `--policy`, or let the `site` release profile load it:

```bash
# Add one --campaign-dir argument for every requirement in the policy.
cargo run --release -p edge-runtime --bin field-interoperability-gate -- \
  --policy deploy/field-acceptance-policy.json \
  --campaign-dir /retained/modbus-tcp/model-a \
  --campaign-dir /retained/s7/s7-1200 \
  --campaign-dir /retained/s7/s7-1500 \
  --campaign-dir /retained/fins/cp1 \
  --campaign-dir /retained/fins/nx1 \
  --output /retained/interoperability/full-site-matrix.json
```

The interoperability report schema is version 4. Every protocol row records required and observed manufacturer and
model counts plus the accepted run identities. Model identity is the normalized
`manufacturer / model` pair, so two serial numbers of the same model do not satisfy a two-model
requirement.

For imported evidence that predates `field-campaign`, the position-matched artifact form remains
available. Every report also requires one native broker audit export:

```bash
cargo run --release -p edge-runtime --bin field-interoperability-gate -- \
  --report /retained/dlt645/vendor-a/field-24h.json \
  --package /retained/dlt645/vendor-a/configuration-package.json \
  --broker-receipt /retained/dlt645/vendor-a/broker-receipt.json \
  --native-broker-audit /retained/dlt645/vendor-a/velamq-audit.json \
  --report /retained/dlt645/vendor-b/field-24h.json \
  --package /retained/dlt645/vendor-b/configuration-package.json \
  --broker-receipt /retained/dlt645/vendor-b/broker-receipt.json \
  --native-broker-audit /retained/dlt645/vendor-b/velamq-audit.json \
  --report /retained/iec101/vendor-a/field-24h.json \
  --package /retained/iec101/vendor-a/configuration-package.json \
  --broker-receipt /retained/iec101/vendor-a/broker-receipt.json \
  --native-broker-audit /retained/iec101/vendor-a/velamq-audit.json \
  --report /retained/iec101/vendor-b/field-24h.json \
  --package /retained/iec101/vendor-b/configuration-package.json \
  --broker-receipt /retained/iec101/vendor-b/broker-receipt.json \
  --native-broker-audit /retained/iec101/vendor-b/velamq-audit.json \
  --report /retained/iec104/vendor-a/field-24h.json \
  --package /retained/iec104/vendor-a/configuration-package.json \
  --broker-receipt /retained/iec104/vendor-a/broker-receipt.json \
  --native-broker-audit /retained/iec104/vendor-a/velamq-audit.json \
  --report /retained/iec104/vendor-b/field-24h.json \
  --package /retained/iec104/vendor-b/configuration-package.json \
  --broker-receipt /retained/iec104/vendor-b/broker-receipt.json \
  --native-broker-audit /retained/iec104/vendor-b/velamq-audit.json \
  --report /retained/opcua/vendor-c/field-24h.json \
  --package /retained/opcua/vendor-c/configuration-package.json \
  --broker-receipt /retained/opcua/vendor-c/broker-receipt.json \
  --native-broker-audit /retained/opcua/vendor-c/velamq-audit.json \
  --report /retained/opcua/vendor-d/field-24h.json \
  --package /retained/opcua/vendor-d/configuration-package.json \
  --broker-receipt /retained/opcua/vendor-d/broker-receipt.json \
  --native-broker-audit /retained/opcua/vendor-d/velamq-audit.json \
  --output /retained/interoperability/dlt645-iec101-iec104-opcua-matrix.json
```

The four repeated options are position-matched. Campaign directories require schema v3 manifests
and reject unsafe artifact paths, incomplete or failed manifests, malformed or cross-campaign Broker
audits, and any artifact modified after the manifest was written. Each broker receipt uses this schema; one receipt
may cover multiple brokers and output branches:

```json
{
  "schemaVersion": 1,
  "edgeId": "edge-iec104-a",
  "configVersion": "v1.2.3",
  "packageSha256": "<64 lowercase hexadecimal characters>",
  "firstReceivedAt": "2026-07-18T00:00:00Z",
  "lastReceivedAt": "2026-07-19T00:00:00Z",
  "messageCount": 172800,
  "routes": [
    {
      "broker": "mqtts://velamq-a.example:8883",
      "consumerId": "field-audit-a",
      "messageCount": 86400,
      "topics": ["factory/edge-iec104-a/telemetry"]
    },
    {
      "broker": "mqtts://velamq-b.example:8883",
      "consumerId": "field-audit-b",
      "messageCount": 86400,
      "topics": ["archive/edge-iec104-a/telemetry"]
    }
  ]
}
```

The matching native Broker audit uses this schema. `routes` must describe the same consumer routes,
counts, and Topics as the receipt; ordering is not significant.

```json
{
  "schemaVersion": 1,
  "broker": "VelaMQ",
  "brokerInstanceId": "velamq-node-a",
  "auditId": "audit-20260718-iec104-a",
  "exportedAt": "2026-07-19T00:00:01Z",
  "edgeId": "edge-iec104-a",
  "configVersion": "v1.2.3",
  "packageSha256": "<64 lowercase hexadecimal characters>",
  "firstObservedAt": "2026-07-18T00:00:00Z",
  "lastObservedAt": "2026-07-19T00:00:00Z",
  "messageCount": 172800,
  "routes": [
    {
      "broker": "mqtts://velamq-a.example:8883",
      "consumerId": "field-audit-a",
      "messageCount": 86400,
      "topics": ["factory/edge-iec104-a/telemetry"]
    },
    {
      "broker": "mqtts://velamq-b.example:8883",
      "consumerId": "field-audit-b",
      "messageCount": 86400,
      "topics": ["archive/edge-iec104-a/telemetry"]
    }
  ]
}
```

The gate computes the package and receipt SHA-256 itself. Package edge/version and digest must
match the report; receipt edge/version/digest must match both. Route message counts must sum to the
receipt total, the total must equal Runtime's QoS 1 publish-success count, and every Runtime sink
with successful publishes must have its broker and last observed Topic in a receipt route. The
native audit must overlap the receipt window, be exported after that window, and match the receipt's
identity, totals, routes, and Topics. The schema v4 result matrix retains both digests, the Broker
instance, audit ID, export time, and normalized route summaries for later audit.

Each input must be a passed `physical_field_endurance` report with complete site/operator/device
identity including its protocol connection, a valid package SHA-256, configured and observed duration of at least 86,400 seconds,
global and per-used-connection failure ratios no greater than 1%, all point/protocol criteria passing, connected MQTT QoS 1 sinks,
PUBACK success and an empty RocksDB outbox. Manufacturer comparison is case-insensitive; multiple
models from one manufacturer still count as one manufacturer. A report may contain multiple protocols,
but only the protocol of `physicalDevice.connectionId` is attributed to that asset and counted. The
same manufacturer/model/serial identity cannot be counted twice for that protocol. Runtime report
schema v4 records independent attempt/success/failure counts, failure ratio, final connection state
and final circuit-breaker state for every protocol connection used by an enabled data configuration.
Every such connection and each publishing MQTT sink also records its largest
observed success gap, configured allowance, counter-reset state, and continuity result. Every
connection and sink must pass independently, so accumulated counters cannot hide a long stall and a
healthy connection cannot dilute another connection's failures. Legacy schema v1/v2/v3 evidence
must be rerun. The site policy caps both collection and
PUBACK progress gaps at 300 seconds, and a report cannot relax that threshold.
Every supplied report must be valid; the gate records all
rejection reasons in its machine-readable output and exits non-zero until the complete matrix is
satisfied.

Use a reviewed policy file for production. Repeated `--require-protocol`,
`--minimum-manufacturers-per-protocol`, `--minimum-models-per-protocol`,
`--minimum-duration-seconds`, `--maximum-failure-ratio`, and
`--maximum-progress-gap-seconds` remain available for legacy or diagnostic
runs. Keep the broker export's native signed/audit record alongside
the normalized receipt when the broker provides one; the JSON contract makes the gate deterministic
but does not replace broker-side authenticity controls.

The Runtime merges adjacent points by station, function and address before device I/O. Read windows
are capped at 2000 bits or 125 registers; command orchestration uses FC15/FC16 only when writable
points are contiguous and share the same connection, station and data area. Other writes remain
independent FC05/FC06 operations. Verify these invariants, plus timeout recovery, before a field run:

```bash
cargo test -p edge-runtime --test modbus_rtu --test modbus_tcp --test command_runtime \
  --test configured_runtime
```

For register points, preserve the device manual's data representation in the released point set:
integer/float encoding, byte order inside each 16-bit register, multi-register word order, scale,
offset, and optional Boolean bit index. The TCP and RTU suites above exercise mixed per-point layouts
inside one bounded read window and verify that writes apply the inverse engineering transform. A
register bit point is read-only until an atomic Modbus mask-write path is available; acceptance must
reject a package that marks such a point writable.

Set a documented engineering range for at least one numeric point and retain one controlled
out-of-range sample in the acceptance evidence. Runtime must keep the decoded value, mark its broad
quality as `Uncertain`, and report `uncertain_out_of_range` as the detailed code. MQTT data-config
payloads with quality enabled must contain matching `quality` and `quality_code` maps. Runtime and
Cloud protocol monitoring must expose the most recent detailed code and monotonically increasing
Good/Uncertain/Bad value counters. A timeout, invalid response, decode failure, invalid mapping, or
open circuit must update the matching bad-quality code even when no usable value can be published.

For each data configuration, `timeout_ms` bounds one collection attempt and `retry_count` controls
how many replacement connections may be opened. Retries use bounded exponential backoff. Runtime
health must retain the timeout/error counters, increment the reconnect counter, and return the
connection to healthy only after a successful retry.

Each protocol connection also owns a process-wide circuit breaker shared by collection and MQTT
command execution. Before field sign-off, set a short approved test threshold, disconnect the
device or block its TCP endpoint, and verify that: the configured consecutive-failure threshold
opens the breaker; subsequent collection and write attempts are rejected without additional device
I/O; Runtime health increments `circuit_open_count` and `circuit_rejected_count`; and only one real
half-open probe is admitted after the cooldown. Restore the device and confirm the configured number
of successful probes closes the breaker. Then restore the production threshold and cooldown in a
new released configuration. A Runtime executor rebuild or an MQTT command arriving between
collection cycles must not reset the breaker state.

## Site prerequisites

- Record the site/work-order ID, operator, device model, device asset/serial number, meter/slave
  address, wiring, and approved configuration version before testing.
- Wire RS-485 A/B and reference ground according to the device manual. Apply termination and bias
  only where the bus topology requires it, and confirm the USB/serial adapter direction control.
- Stop other processes using the port. The Runtime account must have read/write access to the
  character device (normally through a dedicated `dialout`/`uucp` group and a persistent udev rule).
- Install a server certificate for Cloud, a client certificate for Runtime, and their trust CAs.
  The certificate subjects and SHA-256 fingerprints are retained in the report; private keys and
  MQTT password values are never copied into evidence.
- Configure every MQTT sink for QoS 1. Production acceptance requires `mqtts://`, a readable CA
  path, and any password environment variable named by `password_env`.
- Start a broker-side consumer or export the broker audit record to a file. The physical harness
  requires this receipt in addition to Runtime's PUBACK ledger, so a client-only acknowledgement
  cannot be mistaken for end-consumer evidence.

Export an existing validated package, then change its serial endpoint, point addresses, MQTT broker,
TLS CA, credentials variable name, and topic templates for the target site:

```bash
curl -fsS http://127.0.0.1:8082/api/edges/edge-dev/desired-config \
  | jq '.package' > /secure/staging/site-edge-config.json
```

The package `edge_id` may be any enrolled edge identity, but that identity must exist in the SQLite
database supplied through `EDGEOPS_FIELD_CLOUD_DB_SOURCE`. Its version should be a unique release
identifier for the work order. The full harness creates an integrity-checked online snapshot and
never writes to the source database.

## Preflight

Preflight validates JSON shape, serial-device access, supported protocol binding, enabled point
collection, MQTT sink references, TLS/QoS 1 policy, password environment names, and both EdgeLink
certificate/key chains. It does not start Cloud or Runtime and cannot prove device communication.

Start from the versioned environment checklist, but keep the populated copy outside the repository
because it contains production paths and an MQTT password:

```bash
install -m 600 deploy/env/field-acceptance.env.example \
  /secure/staging/field-acceptance.env
${EDITOR:-vi} /secure/staging/field-acceptance.env
set -a
source /secure/staging/field-acceptance.env
set +a
```

The template intentionally defaults to preflight mode and leaves physical-device confirmation off.
Do not change those two values until the wiring, device identity, approved operating state, and
work-order authorization have been checked at the device.

```bash
export EDGEOPS_FIELD_CONFIG=/secure/staging/site-edge-config.json
export EDGEOPS_FIELD_CLOUD_DB_SOURCE=/var/lib/edgeops/cloud-agent.sqlite
export EDGEOPS_FIELD_SERIAL_PORT=/dev/ttyUSB0
export EDGEOPS_FIELD_SERVER_CERT=/etc/edgeops/tls/current/server.pem
export EDGEOPS_FIELD_SERVER_KEY=/etc/edgeops/tls/current/server-key.pem
export EDGEOPS_FIELD_RUNTIME_CA=/etc/edgeops/tls/current/runtime-ca.pem
export EDGEOPS_FIELD_RUNTIME_CERT=/etc/edgeops/runtime/client.pem
export EDGEOPS_FIELD_RUNTIME_KEY=/etc/edgeops/runtime/client-key.pem
export EDGEOPS_FIELD_SERVER_CA=/etc/edgeops/runtime/server-ca.pem
export EDGEOPS_FIELD_SERVER_NAME=edgeops-gateway.internal
export EDGEOPS_MQTT_PASSWORD='read-from-the-site-secret-store'
EDGEOPS_FIELD_PREFLIGHT_ONLY=1 scripts/run-field-hardware-acceptance.sh
```

The generated `report.json` has `mode: "preflight"` and
`physicalDeviceExercised: false`. Controlled CI can exercise the preflight parser with `/dev/null`
only by also setting `EDGEOPS_FIELD_ALLOW_TEST_SERIAL=1`; full acceptance always rejects it.

For a repeatable repository-owned preflight, run:

```bash
scripts/run-field-preflight-acceptance.sh
```

This command uses a minimal Modbus RTU-to-MQTT QoS 1 package and the EdgeLink test certificate
chain. It is included in the local release gate and never sets `physicalDeviceExercised` to true.

## Legacy serial diagnostic run

This harness is retained for a short, authorized serial-path diagnosis. Although its report records
a physical device identity, it predates the per-protocol schema v4 endurance format and is not a
final site-sign-off artifact. Production approval must use one `field-campaign` directory per
required device and `field-interoperability-gate` through the strict `site` release profile.

Ensure the device is in an approved operating state before transmitting protocol requests. The
harness snapshots the configured production catalog, starts isolated Cloud HTTP/EdgeLink listeners
on `18101`/`19101` in empty-bootstrap mode, verifies the package edge exists in that snapshot,
creates a release from the exact package, issues a one-time edge token, and runs the release Runtime
once with mTLS and MQTT enabled. It does not stop or modify the normal service on `8082`.

```bash
export EDGEOPS_FIELD_SITE_ID=WO-2026-0719-SH01
export EDGEOPS_FIELD_OPERATOR=field-engineer-name
export EDGEOPS_FIELD_DEVICE_MODEL='Acme PowerMeter PM-800'
export EDGEOPS_FIELD_DEVICE_SERIAL='PM800-SH01-00042'
export EDGEOPS_FIELD_PHYSICAL_DEVICE_CONFIRMED=1
export EDGEOPS_FIELD_BROKER_RECEIPT=/secure/staging/velamq-consumer-receipt.json
scripts/run-field-hardware-acceptance.sh
```

Use `EDGEOPS_FIELD_HTTP_PORT` and `EDGEOPS_FIELD_GATEWAY_PORT` if the isolated ports are occupied.
Use `EDGEOPS_FIELD_WORK_DIR` to place evidence on an encrypted or retained volume. A controlled lab
may explicitly set `EDGEOPS_FIELD_ALLOW_INSECURE_MQTT=1`; such evidence must not be approved as a
production security acceptance.

For a product graph with multiple MQTT output branches, set the minimum expected distinct routes
before running. A route is the pair `[sink_id, expanded topic]`, so two branches that publish to the
same topic count once:

```bash
export EDGEOPS_FIELD_MIN_MQTT_ROUTES=2
scripts/run-field-hardware-acceptance.sh
```

## Legacy diagnostic pass criteria

The command exits zero only when all of these are true:

1. The package is valid and contains at least one enabled, non-empty data configuration bound to
   the selected physical serial connection.
2. EdgeLink server and Runtime identities pass trust-chain, private-key, and expiry checks.
3. Cloud becomes ready with SQLite, required API authentication, and an mTLS EdgeLink listener.
4. Runtime receives the exact package version, stores it in RocksDB, performs collection through
   the physical serial adapter, records `samples_collected > 0`, and exits without a protocol error.
5. Every Runtime-reported MQTT publication has a matching RocksDB broker-acknowledgement receipt.
   Each receipt matches a released sink, has a non-empty expanded topic and positive payload size,
   and the number of distinct `[sink_id, topic]` routes meets `EDGEOPS_FIELD_MIN_MQTT_ROUTES`. A
   structured Broker audit is cross-checked against that receipt, copied into the evidence bundle,
   and hashed.
6. EdgeLink acknowledges Runtime state, Cloud records the same reported version as `已应用`, and
   Runtime status reports connected Cloud sync and RocksDB state.
7. The report records the physical device model, serial/asset number, operator and explicit operator
   confirmation. Cloud handles SIGTERM gracefully.

Retain the entire `target/field-acceptance-*` directory. `report.json` includes package and Cloud
snapshot SHA-256 values, the matched catalog edge, certificate metadata, serial settings, MQTT
brokers/topics without secrets, collected sample and message counts, release/runtime IDs, Runtime
status, release acknowledgement, `mqttDistinctRoutes`, and bounded `mqttAcknowledgements` route
metadata. Payload bytes and credentials are not copied into the receipt ledger. `runtime.log`,
`cloud.log`, `mqtt-acknowledgements.json`, the isolated SQLite snapshot, and the Runtime RocksDB
directory remain available for incident review.

Every run also stores the validated package as `configuration-package.json` and creates
`evidence-manifest.json`. The manifest contains the relative path, byte count and SHA-256 digest of
every retained evidence file, including Runtime RocksDB files. Verify a bundle independently with:

```bash
scripts/verify-field-acceptance-report.sh --require-physical /retained/evidence/report.json
```

The verifier rejects preflight reports in physical mode, unsafe evidence paths, PTYs/test character
devices, missing identity or broker receipt, insufficient MQTT routes, mismatched Runtime/release
versions, and any modified or missing evidence file.

Attach this diagnostic bundle and a wiring photo to the work order only as supporting evidence.
Final site sign-off uses the schema v4 `field-endurance` reports and the policy-bound
`field-interoperability-gate`; this legacy bundle cannot replace either. The repository's automated
PTY, mTLS, VelaMQ, recovery, and performance reports also remain complementary evidence.
