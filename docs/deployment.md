# EdgeOps Deployment And Recovery

## Production Baseline

Run `cloud-api` and each `edge-runtime` as separate operating-system services. Keep the cloud
SQLite database, runtime RocksDB directory, TLS private keys, and MQTT credentials on persistent
volumes. Do not use the bundled test certificates outside acceptance tests.

Required cloud settings:

```bash
export EDGEOPS_CLOUD_DB='sqlite:///var/lib/edgeops/cloud-agent.sqlite?mode=rwc'
export EDGEOPS_HTTP_ADDR='0.0.0.0:8080'
export EDGEOPS_GATEWAY_ADDR='0.0.0.0:18080'
export EDGEOPS_API_AUTH_MODE='required'
export EDGEOPS_BOOTSTRAP_MODE='empty'
export EDGEOPS_VIEWER_TOKEN='replace-with-a-secret-of-at-least-24-characters'
export EDGEOPS_OPERATOR_TOKEN='replace-with-an-independent-secret'
export EDGEOPS_ADMIN_TOKEN='replace-with-an-independent-secret'
export EDGEOPS_GATEWAY_TLS_CERT='/etc/edgeops/tls/current/server.pem'
export EDGEOPS_GATEWAY_TLS_KEY='/etc/edgeops/tls/current/server-key.pem'
export EDGEOPS_GATEWAY_TLS_CLIENT_CA='/etc/edgeops/tls/current/runtime-ca.pem'
```

`empty` is the production bootstrap mode: it preserves only records already stored in SQLite and
does not create sample projects, products, edges, metrics, or configuration. When bootstrap mode is
unset, required API authentication also selects `empty` automatically; unauthenticated local
development selects `demo`. Use `demo` only for disposable development and acceptance databases.

Terminate TLS for the management HTTP endpoint at the ingress or reverse proxy. EdgeLink performs
its own TLS 1.3 mutual authentication. Restrict ports `8080` and `18080` with network policy; only
administrators and enrolled runtimes should reach them.

## Health Probes

- `GET /health/live` proves the process and HTTP event loop are alive.
- `GET /health/ready` checks in-memory state and executes `SELECT 1` against SQLite.

Both endpoints are intentionally public so orchestrators can probe a required-auth deployment.
They expose no fleet or credential data. Use a 5-second timeout and remove an instance from service
after three consecutive readiness failures. Do not restart solely because readiness is temporarily
unavailable; liveness is the restart signal.

## Graceful Shutdown

`cloud-api` handles SIGTERM and Ctrl+C. It stops accepting HTTP requests, drains in-flight requests,
then closes the EdgeLink listener. Configure a termination grace period of at least 30 seconds.
Runtime config and MQTT outbox state remain in RocksDB and are resumed on the next start.

## Backup And Restore

Create an online, integrity-checked SQLite backup while the service is running:

```bash
scripts/cloud-state.sh backup \
  /var/lib/edgeops/cloud-agent.sqlite \
  /var/backups/edgeops/cloud-agent-$(date +%Y%m%d-%H%M%S).sqlite
```

Verify a retained backup regularly:

```bash
scripts/cloud-state.sh verify /var/backups/edgeops/cloud-agent-20260718-120000.sqlite
```

To restore, stop `cloud-api`, preserve the damaged database for investigation, and run:

```bash
scripts/cloud-state.sh restore \
  /var/backups/edgeops/cloud-agent-20260718-120000.sqlite \
  /var/lib/edgeops/cloud-agent.sqlite
```

Start the service and require `/health/ready` to return HTTP 200 before admitting traffic. Then
confirm project, product version, edge binding, release, conversation, knowledge, and audit records.
Back up TLS keys and CA material through the organization's secret manager, not with this script.

## Certificate Rotation

Issue server and runtime certificates from a dedicated EdgeOps CA. Use short-lived runtime
certificates and a separate enrollment token for first registration. To rotate without downtime:

1. Add the new CA to the trust bundle while retaining the old CA.
2. Deploy new server and runtime certificates.
3. Verify a new mTLS session and config acknowledgement.
4. Remove the old CA only after every runtime has reconnected with the new certificate.

Manage gateway certificate releases with the bundled tool. It validates the trust chain, confirms
that the private key matches the leaf certificate, enforces a remaining-validity threshold, stores
versioned releases with strict key permissions, and atomically switches the `current` symlink:

```bash
scripts/edgelink-certificates.sh install \
  ./server.pem ./server-key.pem ./runtime-ca.pem /etc/edgeops/tls 30
scripts/edgelink-certificates.sh status /etc/edgeops/tls 30
scripts/edgelink-certificates.sh list /etc/edgeops/tls
```

Configure the service paths as `/etc/edgeops/tls/current/server.pem`,
`/etc/edgeops/tls/current/server-key.pem`, and `/etc/edgeops/tls/current/runtime-ca.pem`. Roll back
by activating a retained release ID. The process reads certificate files at startup, so restart it
gracefully after activation. Partial TLS configuration fails closed. Run
`scripts/run-certificate-lifecycle-acceptance.sh` and `scripts/run-edgelink-mtls-acceptance.sh`
before production rollout to validate both rotation safety and the full identity/config path.

## Release Gate

Run the complete local gate with one command. Supplying `VELAMQ_REPO` also executes the real
broker TLS/QoS 1 acceptance. The combined report records the exact Git commit, dirty-worktree flag,
per-gate duration, logs, and nested evidence locations:

```bash
VELAMQ_REPO=/path/to/velamq-rs scripts/run-release-gates.sh
```

The equivalent individual gates are:

```bash
cargo test --workspace
npm --prefix web/console test -- --run
npm --prefix web/console run build
scripts/run-edgelink-mtls-acceptance.sh
scripts/run-certificate-lifecycle-acceptance.sh
scripts/run-performance-gates.sh
```

Run `scripts/run-real-velamq-acceptance.sh` against the target VelaMQ build when MQTT transport,
broker certificates, or topic policy changes.

Before approving a site rollout, run `scripts/run-field-hardware-acceptance.sh` on the target edge
host with the released package, production certificate chain, physical serial port, and target MQTT
broker. Its preflight mode is suitable for deployment preparation but is not field evidence. The
full procedure and pass criteria are in [`field-acceptance.md`](field-acceptance.md).

For the final site sign-off, export the field variables described there and use the strict profile:

```bash
export EDGEOPS_RELEASE_PROFILE=site
export VELAMQ_REPO=/path/to/velamq-rs
# export EDGEOPS_FIELD_* and referenced MQTT password variables
scripts/run-release-gates.sh
```

The `site` profile fails closed when VelaMQ source acceptance or physical field inputs are missing;
it never turns a skipped external check into a passing production report.

The repeatable recovery harness starts the real cloud binary twice, performs an online backup,
changes the live database, restores the backup atomically, verifies a marker and catalog data, and
asserts graceful SIGTERM handling:

```bash
scripts/run-cloud-recovery-acceptance.sh
```

## Performance Gate

`scripts/run-performance-gates.sh` builds release binaries and measures two production paths. The
Cloud API gate runs authenticated concurrent requests against the real Axum/SQLite process and
fails on request errors, insufficient throughput, or excessive P95 latency. The Runtime gate
executes a configurable multi-point, multi-node DSL graph and fails on insufficient sample
throughput or excessive batch P95 latency. Every run retains raw ApacheBench output, process logs,
Runtime JSON, and a combined `report.json` under `target/performance-gates-*`.

Thresholds are deployment inputs rather than hard-coded claims. Override
`EDGEOPS_PERF_MIN_HTTP_RPS`, `EDGEOPS_PERF_MAX_HTTP_P95_MS`, `EDGEOPS_PERF_MIN_RUNTIME_SPS`, and
`EDGEOPS_PERF_MAX_RUNTIME_P95_US` with the approved hardware baseline in CI or on the target edge
host.
