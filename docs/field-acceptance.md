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

Ethernet collection can be exercised locally with `modbus-tcp-simulator` on port `1502`. Unlike an
in-process mock, it makes the production Runtime open a real TCP connection, decode Modbus frames,
execute the released graph, and publish through the configured MQTT broker. This is useful end-to-end
laboratory evidence, but the simulated register source still means `physicalDeviceExercised: false`.
Run `scripts/run-lab-modbus-tcp-acceptance.sh` to retain a machine-readable report containing source
hashes, the exercised production components, Modbus functions, MQTT acknowledgement requirement,
and the explicit physical-device limitation.

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

## Physical run

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

## Pass criteria

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
   non-empty broker-side consumer/audit receipt is copied into the evidence bundle and hashed.
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

For site sign-off, attach a wiring photo and the verified report bundle to the work order. Device
identity and broker-side receipt are now mandatory report evidence; the manifest replaces an
informal directory checksum with file-level verification. The repository's automated PTY, mTLS,
VelaMQ, recovery, and performance reports complement this evidence; none replaces it.
