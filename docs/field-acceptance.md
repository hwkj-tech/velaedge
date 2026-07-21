# Physical Field Acceptance

This procedure verifies the production path on the target edge host: a real serial request reaches
the connected device, Runtime executes the released collection graph, the result is acknowledged by
the configured MQTT broker, and Cloud records the applied configuration and Runtime health. A PTY,
`/dev/null`, a simulated adapter, or configuration-only validation is useful preflight evidence but
is not physical acceptance.

## Site prerequisites

- Record the site/work-order ID, operator, device model, meter/slave address, wiring, and approved
  configuration version before testing.
- Wire RS-485 A/B and reference ground according to the device manual. Apply termination and bias
  only where the bus topology requires it, and confirm the USB/serial adapter direction control.
- Stop other processes using the port. The Runtime account must have read/write access to the
  character device (normally through a dedicated `dialout`/`uucp` group and a persistent udev rule).
- Install a server certificate for Cloud, a client certificate for Runtime, and their trust CAs.
  The certificate subjects and SHA-256 fingerprints are retained in the report; private keys and
  MQTT password values are never copied into evidence.
- Configure every MQTT sink for QoS 1. Production acceptance requires `mqtts://`, a readable CA
  path, and any password environment variable named by `password_env`.

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

## Physical run

Ensure the device is in an approved operating state before transmitting protocol requests. The
harness snapshots the configured production catalog, starts isolated Cloud HTTP/EdgeLink listeners
on `18101`/`19101` in empty-bootstrap mode, verifies the package edge exists in that snapshot,
creates a release from the exact package, issues a one-time edge token, and runs the release Runtime
once with mTLS and MQTT enabled. It does not stop or modify the normal service on `8082`.

```bash
export EDGEOPS_FIELD_SITE_ID=WO-2026-0719-SH01
export EDGEOPS_FIELD_OPERATOR=field-engineer-name
scripts/run-field-hardware-acceptance.sh
```

Use `EDGEOPS_FIELD_HTTP_PORT` and `EDGEOPS_FIELD_GATEWAY_PORT` if the isolated ports are occupied.
Use `EDGEOPS_FIELD_WORK_DIR` to place evidence on an encrypted or retained volume. A controlled lab
may explicitly set `EDGEOPS_FIELD_ALLOW_INSECURE_MQTT=1`; such evidence must not be approved as a
production security acceptance.

## Pass criteria

The command exits zero only when all of these are true:

1. The package is valid and contains at least one enabled, non-empty data configuration bound to
   the selected physical serial connection.
2. EdgeLink server and Runtime identities pass trust-chain, private-key, and expiry checks.
3. Cloud becomes ready with SQLite, required API authentication, and an mTLS EdgeLink listener.
4. Runtime receives the exact package version, stores it in RocksDB, performs collection through
   the physical serial adapter, records `samples_collected > 0`, and exits without a protocol error.
5. At least one message reaches a configured MQTT sink and receives the broker acknowledgement.
6. EdgeLink acknowledges Runtime state, Cloud records the same reported version as `已应用`, and
   Runtime status reports connected Cloud sync and RocksDB state.
7. Cloud handles SIGTERM gracefully.

Retain the entire `target/field-acceptance-*` directory. `report.json` includes package and Cloud
snapshot SHA-256 values, the matched catalog edge, certificate metadata, serial settings, MQTT
brokers/topics without secrets, collected sample and message counts, release/runtime IDs, Runtime
status, and release acknowledgement. `runtime.log`, `cloud.log`, the isolated SQLite snapshot, and
the Runtime RocksDB directory remain available for incident review.

For site sign-off, attach a wiring photo, device model/serial number, broker-side topic receipt or
consumer evidence, and the report directory checksum to the work order. The repository's automated
PTY, mTLS, VelaMQ, recovery, and performance reports complement this evidence; none replaces it.
