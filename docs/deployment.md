# VelaEdge Deployment And Recovery

## Production Baseline

Run `cloud-api` and each `edge-runtime` as separate operating-system services. Keep the cloud
SQLite database, runtime RocksDB directory, TLS private keys, and MQTT credentials on persistent
volumes. Do not use the bundled test certificates outside acceptance tests.

Required cloud settings:

```bash
export EDGEOPS_CLOUD_DB='sqlite:///var/lib/edgeops/cloud-agent.sqlite?mode=rwc'
export EDGEOPS_HTTP_ADDR='0.0.0.0:8080'
export EDGEOPS_GATEWAY_ADDR='0.0.0.0:18080'
export EDGEOPS_CONSOLE_DIST='/opt/edgeops/console'
export EDGEOPS_API_AUTH_MODE='required'
export EDGEOPS_BOOTSTRAP_MODE='empty'
export EDGEOPS_VIEWER_TOKEN='replace-with-a-secret-of-at-least-24-characters'
export EDGEOPS_OPERATOR_TOKEN='replace-with-an-independent-secret'
export EDGEOPS_ADMIN_TOKEN='replace-with-an-independent-secret'
export EDGEOPS_GATEWAY_TLS_CERT='/etc/edgeops/tls/current/server.pem'
export EDGEOPS_GATEWAY_TLS_KEY='/etc/edgeops/tls/current/server-key.pem'
export EDGEOPS_GATEWAY_TLS_CLIENT_CA='/etc/edgeops/tls/current/runtime-ca.pem'
```

Reference systemd units and environment templates are provided under `deploy/systemd` and
`deploy/env`. Install the Cloud and Runtime binaries under `/opt/edgeops/bin`, copy the built console
contents from `web/console/dist` to `/opt/edgeops/console`, and store populated environment files
under `/etc/edgeops`. Environment files contain secrets and must be owned by the service account
with mode `0600`; never install the example placeholder values unchanged.

For Cloud:

```bash
install -m 0755 target/release/cloud-api /opt/edgeops/bin/cloud-api
cp -R web/console/dist/. /opt/edgeops/console/
install -m 0644 deploy/systemd/edgeops-cloud.service /etc/systemd/system/
install -m 0600 deploy/env/cloud.env.example /etc/edgeops/cloud.env
systemctl daemon-reload
systemctl enable --now edgeops-cloud
```

For each Runtime, create `/etc/edgeops/runtime/EDGE_ID.env`, provision its mTLS identity and
one-time edge token, ensure the `edgeops-runtime` account can open the selected serial device, then
start `edgeops-runtime@EDGE_ID.service`. The service passes only the token variable name on the
command line, so the secret itself is not exposed by the process list.

For a physical 24-hour campaign, also install the release campaign binary and guarded runner:

```bash
install -m 0755 target/release/field-campaign /opt/edgeops/bin/field-campaign
install -m 0755 target/release/field-campaign-status /opt/edgeops/bin/field-campaign-status
install -m 0755 scripts/run-field-campaign.sh /opt/edgeops/bin/run-field-campaign
install -m 0644 deploy/systemd/edgeops-field-campaign@.service /etc/systemd/system/
install -m 0644 deploy/systemd/edgeops-field-campaign-status.service /etc/systemd/system/
install -m 0644 deploy/systemd/edgeops-field-campaign-status.timer /etc/systemd/system/
install -d -m 0755 /opt/edgeops/deploy
install -m 0644 deploy/field-acceptance-policy.json \
  /opt/edgeops/deploy/field-acceptance-policy.json
install -d -m 0700 /etc/edgeops/field-campaign
install -m 0600 deploy/env/field-campaign.env.example \
  /etc/edgeops/field-campaign/IEC104_VENDOR_A.env
install -m 0640 site-plan.json /etc/edgeops/field-campaign/site-plan.json
systemctl daemon-reload
systemctl enable --now edgeops-field-campaign-status.timer
```

Populate one environment file per physical asset, place its immutable released package under the
matching `/var/lib/edgeops-field-campaign/INSTANCE/input` directory, and set physical confirmation
to `1` only after the work order, wiring and identity have been checked. Before opening the
evidence window, systemd runs the same launcher with `--preflight-only`. This validates the complete
Runtime graph, physical connection binding, MQTT 3.1.1/5.0 route expansion, password environment
references, custom CA files, thresholds, and evidence paths without creating the evidence directory
or opening a device/Broker session. Start it with `systemctl start
edgeops-field-campaign@IEC104_VENDOR_A`. `systemctl stop` sends `SIGTERM`; the campaign retains a
failed/interrupted manifest rather than leaving ambiguous running evidence.
The unit intentionally uses `Restart=no`: a new attempt requires a new evidence directory and an
explicit operator start.

For a multi-device site, run `field-campaign-plan` before installing the instances. Its report
proves that the complete asset inventory satisfies `deploy/field-acceptance-policy.json`, uses
unique physical identities, edge/client IDs and evidence paths, and exposes the exact non-secret
environment map plus required secret variable names for each systemd instance. The schema and
command are documented in `docs/field-acceptance.md`. During execution, run
`field-campaign-status` against that unchanged plan to produce a site-level pending/running/passed/
failed snapshot. The final deployment sign-off must use `--require-complete`; completed rows are
accepted only after their four hash-bound artifacts also match the planned physical identity and
released package. The supplied timer refreshes
`/var/lib/edgeops-field-campaign/site-status.json` every minute using an atomic file replacement;
pending/running observations remain healthy, while failed or invalid campaigns make the oneshot
service fail visibly in systemd. The timer continues to retry on the next interval.

The `site` release profile consumes this same inventory instead of a manually assembled directory
list:

```bash
EDGEOPS_RELEASE_PROFILE=site \
EDGEOPS_FIELD_CAMPAIGN_PLAN=/etc/edgeops/field-campaign/site-plan.json \
EDGEOPS_FIELD_POLICY=/opt/edgeops/deploy/field-acceptance-policy.json \
VELAMQ_REPO=/opt/src/velamq \
scripts/run-release-gates.sh
```

The release gate invokes `field-campaign-status --require-complete`; missing, running, failed,
identity-mismatched, or policy-incomplete campaigns stop the site release.

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
- Cloud startup enables SQLite WAL mode, normal synchronous durability, a five-second busy timeout,
  and foreign-key enforcement for file databases, then runs `PRAGMA quick_check` before serving.

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

Issue server and runtime certificates from a dedicated VelaEdge CA. Use short-lived runtime
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

统一门禁的本地配置会在 Docker daemon 可用时自动运行 S7、FINS 与 IEC 104 独立设备容器验收；
`site` 配置默认把它作为必选项。可用 `EDGEOPS_CONTAINER_PROTOCOL_GATE=required|auto|skip`
显式控制策略。隔离网络已有缓存镜像时同时设置
`EDGEOPS_CONTAINER_PROTOCOL_NO_BUILD=1`，报告仍会记录实际镜像 ID。

The equivalent individual gates are:

```bash
cargo test --workspace
npm --prefix web/console test -- --run
npm --prefix web/console run build
npm --prefix web/console run test:e2e
scripts/run-edgelink-mtls-acceptance.sh
scripts/run-certificate-lifecycle-acceptance.sh
scripts/run-performance-gates.sh
scripts/run-field-preflight-acceptance.sh
scripts/run-deployment-smoke-acceptance.sh
```

Run `scripts/run-real-velamq-acceptance.sh` against the target VelaMQ build when MQTT transport,
broker certificates, or topic policy changes.

Before approving a site rollout, run `field-campaign` once for every required physical vendor
device. Pass the target broker's schema v1 structured audit path with `--native-broker-audit`.
The file may be exported after the Runtime window closes; the campaign publishes its current phase
to `manifest.json` and waits up to `--native-broker-audit-wait-seconds` for the completed audit.
Each campaign uses the released package and target MQTT broker and retains a hash-bound
evidence directory. `scripts/run-field-hardware-acceptance.sh` remains available for serial-only
preflight and legacy site diagnostics, but one such run cannot satisfy the multi-vendor protocol
matrix. The full procedure and pass criteria are in
[`field-acceptance.md`](field-acceptance.md).

The local release profile runs `run-field-preflight-acceptance.sh` automatically when no site
package is supplied. This controlled fixture checks the field harness, package constraints, serial
binding, QoS 1 route, and EdgeLink certificate chains. Its report intentionally records
`physicalDeviceExercised: false`; it is deployment-preparation evidence, not site sign-off.

The console E2E gate starts a real `cloud-api` process in empty-bootstrap mode with an isolated
SQLite database, then drives Chromium through project creation, reusable point-set creation,
product binding and publication, edge enrollment, one-time token display, runtime-monitor dialogs,
and Escape-key modal dismissal.
Artifacts and a machine-readable Playwright report are retained under the release work directory.
The legacy serial diagnostic mode additionally requires device model/serial identity, explicit
operator confirmation, a broker-side receipt, and a verifiable `evidence-manifest.json`. Its
`verify-field-acceptance-report.sh --require-physical` check only validates that legacy bundle; it
does not satisfy the schema v4 interoperability policy used by the `site` release profile.

For the final site sign-off, export the field variables described there and use the strict profile:

```bash
export EDGEOPS_RELEASE_PROFILE=site
export VELAMQ_REPO=/path/to/velamq-rs
# Validated site plan with hash-bound campaign directories for every policy requirement.
export EDGEOPS_FIELD_CAMPAIGN_PLAN=/etc/edgeops/field-campaign/site-plan.json
# Optional override; site defaults to deploy/field-acceptance-policy.json.
export EDGEOPS_FIELD_POLICY=/approved/field-acceptance-policy.json
scripts/run-release-gates.sh
```

The `site` profile fails closed when VelaMQ source acceptance or the campaign plan is missing.
It verifies every schema v3 campaign manifest and all four artifact digests, including the native
broker audit, then runs the versioned per-protocol manufacturer/model interoperability policy.
The repository policy covers all ten non-simulated southbound protocols; an approved policy can be
supplied with `EDGEOPS_FIELD_POLICY`. A missing or malformed policy fails the release. The gate never
turns a skipped external check into a passing production report.

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
