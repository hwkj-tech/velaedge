#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG_PATH="${EDGEOPS_FIELD_CONFIG:-}"
CLOUD_DB_SOURCE="${EDGEOPS_FIELD_CLOUD_DB_SOURCE:-}"
SERIAL_PORT="${EDGEOPS_FIELD_SERIAL_PORT:-}"
SERVER_CERT="${EDGEOPS_FIELD_SERVER_CERT:-}"
SERVER_KEY="${EDGEOPS_FIELD_SERVER_KEY:-}"
RUNTIME_CA="${EDGEOPS_FIELD_RUNTIME_CA:-}"
RUNTIME_CERT="${EDGEOPS_FIELD_RUNTIME_CERT:-}"
RUNTIME_KEY="${EDGEOPS_FIELD_RUNTIME_KEY:-}"
SERVER_CA="${EDGEOPS_FIELD_SERVER_CA:-${RUNTIME_CA}}"
SERVER_NAME="${EDGEOPS_FIELD_SERVER_NAME:-localhost}"
HTTP_PORT="${EDGEOPS_FIELD_HTTP_PORT:-18101}"
GATEWAY_PORT="${EDGEOPS_FIELD_GATEWAY_PORT:-19101}"
WORK_DIR="${EDGEOPS_FIELD_WORK_DIR:-${ROOT_DIR}/target/field-acceptance-$$}"
REPORT_PATH="${EDGEOPS_FIELD_REPORT:-${WORK_DIR}/report.json}"
PREFLIGHT_ONLY="${EDGEOPS_FIELD_PREFLIGHT_ONLY:-0}"
ALLOW_INSECURE_MQTT="${EDGEOPS_FIELD_ALLOW_INSECURE_MQTT:-0}"
ALLOW_TEST_SERIAL="${EDGEOPS_FIELD_ALLOW_TEST_SERIAL:-0}"
MIN_CERT_DAYS="${EDGEOPS_FIELD_MIN_CERT_DAYS:-30}"
SITE_ID="${EDGEOPS_FIELD_SITE_ID:-}"
OPERATOR="${EDGEOPS_FIELD_OPERATOR:-}"
CLOUD_BIN="${EDGEOPS_FIELD_CLOUD_BIN:-${ROOT_DIR}/target/release/cloud-api}"
RUNTIME_BIN="${EDGEOPS_FIELD_RUNTIME_BIN:-${ROOT_DIR}/target/release/edge-runtime}"
ADMIN_TOKEN="${EDGEOPS_FIELD_ADMIN_TOKEN:-edgeops-field-admin-token-00000001}"
TOKEN_ENV_NAME="EDGEOPS_FIELD_RUNTIME_ACCESS_TOKEN"
CLOUD_PID=""

usage() {
  cat <<'EOF'
Run a production-path field acceptance against a physical serial device and MQTT broker.

Required environment:
  EDGEOPS_FIELD_CONFIG         EdgeConfigPackage JSON file for the enrolled site edge
  EDGEOPS_FIELD_CLOUD_DB_SOURCE  SQLite database containing the enrolled edge (full run only)
  EDGEOPS_FIELD_SERIAL_PORT    Physical serial character device used by the package
  EDGEOPS_FIELD_SERVER_CERT    Cloud EdgeLink server certificate
  EDGEOPS_FIELD_SERVER_KEY     Cloud EdgeLink server private key
  EDGEOPS_FIELD_RUNTIME_CA     CA that issued the Runtime client certificate
  EDGEOPS_FIELD_RUNTIME_CERT   Runtime client certificate
  EDGEOPS_FIELD_RUNTIME_KEY    Runtime client private key
  EDGEOPS_FIELD_SITE_ID        Site/work-order identifier (full run only)
  EDGEOPS_FIELD_OPERATOR       Acceptance operator (full run only)

Optional:
  EDGEOPS_FIELD_SERVER_CA      CA that issued the server certificate (defaults to Runtime CA)
  EDGEOPS_FIELD_SERVER_NAME    TLS server name (default: localhost)
  EDGEOPS_FIELD_PREFLIGHT_ONLY Validate inputs and write evidence without starting processes
  EDGEOPS_FIELD_ALLOW_INSECURE_MQTT=1  Permit mqtt:// only for controlled lab runs
  EDGEOPS_FIELD_ALLOW_TEST_SERIAL=1    Permit /dev/null or /dev/zero in preflight only

MQTT password values remain in environment variables named by mqtt_uplinks[].password_env.
EOF
}

fail() {
  echo "field acceptance: $*" >&2
  exit 2
}

require_file() {
  local label="$1"
  local path="$2"
  [[ -n "$path" ]] || fail "$label is required"
  [[ -f "$path" ]] || fail "$label does not exist: $path"
}

cleanup() {
  if [[ -n "$CLOUD_PID" ]]; then
    kill -TERM "$CLOUD_PID" 2>/dev/null || true
    wait "$CLOUD_PID" 2>/dev/null || true
  fi
  unset "$TOKEN_ENV_NAME" 2>/dev/null || true
}

on_exit() {
  local status=$?
  if [[ "$status" -ne 0 ]]; then
    echo "field acceptance failed; evidence retained at: $WORK_DIR" >&2
  fi
  cleanup
  return "$status"
}

trap on_exit EXIT
trap 'exit 130' INT TERM

for command in cargo curl jq nc openssl shasum sqlite3 stat; do
  command -v "$command" >/dev/null || fail "missing required command: $command"
done

[[ -n "$CONFIG_PATH" ]] || { usage >&2; fail "EDGEOPS_FIELD_CONFIG is required"; }
[[ -n "$SERIAL_PORT" ]] || fail "EDGEOPS_FIELD_SERIAL_PORT is required"
require_file "configuration package" "$CONFIG_PATH"
require_file "EdgeLink server certificate" "$SERVER_CERT"
require_file "EdgeLink server key" "$SERVER_KEY"
require_file "Runtime client CA" "$RUNTIME_CA"
require_file "Runtime client certificate" "$RUNTIME_CERT"
require_file "Runtime client key" "$RUNTIME_KEY"
require_file "EdgeLink server CA" "$SERVER_CA"

jq -e 'type == "object"' "$CONFIG_PATH" >/dev/null || fail "configuration is not a JSON object"
EDGE_ID="$(jq -er '.edge_id | select(type == "string" and length > 0)' "$CONFIG_PATH")"
CONFIG_VERSION="$(jq -er '.version | select(type == "string" and length > 0)' "$CONFIG_PATH")"

[[ -e "$SERIAL_PORT" ]] || fail "serial device does not exist: $SERIAL_PORT"
[[ -c "$SERIAL_PORT" ]] || fail "serial path is not a character device: $SERIAL_PORT"
[[ -r "$SERIAL_PORT" && -w "$SERIAL_PORT" ]] || fail "serial device must be readable and writable: $SERIAL_PORT"
if [[ "$SERIAL_PORT" == "/dev/null" || "$SERIAL_PORT" == "/dev/zero" ]]; then
  [[ "$PREFLIGHT_ONLY" == "1" && "$ALLOW_TEST_SERIAL" == "1" ]] || \
    fail "test character devices are allowed only for explicit preflight validation"
fi

SERIAL_CONNECTIONS="$(jq -c --arg port "$SERIAL_PORT" '[
  .protocol_connections[]
  | select(.protocol == "ModbusRtu" or .protocol == "Dlt645" or .protocol == "Iec101" or .protocol == "CustomSerial")
  | select(.endpoint == $port or .serial.port == $port)
]' "$CONFIG_PATH")"
[[ "$(printf '%s' "$SERIAL_CONNECTIONS" | jq 'length')" -gt 0 ]] || \
  fail "configuration has no supported serial connection for $SERIAL_PORT"

ENABLED_CONFIGS="$(jq -c '[.data_configs[] | select(.enabled == true)]' "$CONFIG_PATH")"
[[ "$(printf '%s' "$ENABLED_CONFIGS" | jq 'length')" -gt 0 ]] || fail "configuration has no enabled data configuration"
jq -e --argjson serial "$SERIAL_CONNECTIONS" '
  ($serial | map(.connection_id)) as $ids
  | any(.data_configs[]; .enabled == true and (.protocol_connection_id as $id | $ids | index($id)) != null and (.points | length) > 0)
' "$CONFIG_PATH" >/dev/null || fail "no enabled data configuration collects points through the selected serial connection"

jq -e '
  (.mqtt_uplinks | length) > 0
  and all(.mqtt_uplinks[]; (.sink_id | type == "string" and length > 0) and .qos == 1)
  and all(.data_configs[] | select(.enabled == true); .publish.qos == 1)
  and ([.mqtt_uplinks[].sink_id] as $sinks | all(.data_configs[] | select(.enabled == true); (.publish.sink_id as $sink | $sinks | index($sink)) != null))
' "$CONFIG_PATH" >/dev/null || fail "every enabled data configuration must target an existing QoS 1 MQTT sink"

if [[ "$ALLOW_INSECURE_MQTT" != "1" ]]; then
  jq -e 'all(.mqtt_uplinks[]; (.broker | startswith("mqtts://")) and (.tls_ca_path | type == "string" and length > 0))' \
    "$CONFIG_PATH" >/dev/null || fail "field acceptance requires mqtts:// and tls_ca_path for every MQTT sink"
fi

while IFS= read -r ca_path; do
  [[ -z "$ca_path" ]] || require_file "MQTT CA" "$ca_path"
done < <(jq -r '.mqtt_uplinks[].tls_ca_path // empty' "$CONFIG_PATH")

while IFS= read -r password_env; do
  [[ -z "$password_env" ]] && continue
  [[ "$password_env" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || fail "invalid MQTT password environment name: $password_env"
  [[ -n "${!password_env:-}" ]] || fail "MQTT password environment variable is missing or empty: $password_env"
done < <(jq -r '.mqtt_uplinks[].password_env // empty' "$CONFIG_PATH")

mkdir -p "$WORK_DIR" "$(dirname "$REPORT_PATH")"
"${ROOT_DIR}/scripts/edgelink-certificates.sh" check \
  "$SERVER_CERT" "$SERVER_KEY" "$SERVER_CA" "$MIN_CERT_DAYS" >"${WORK_DIR}/server-certificate.json"
"${ROOT_DIR}/scripts/edgelink-certificates.sh" check \
  "$RUNTIME_CERT" "$RUNTIME_KEY" "$RUNTIME_CA" "$MIN_CERT_DAYS" >"${WORK_DIR}/runtime-certificate.json"

PACKAGE_SHA256="$(shasum -a 256 "$CONFIG_PATH" | awk '{print $1}')"
SERIAL_EVIDENCE="$(jq -n \
  --arg path "$SERIAL_PORT" \
  --arg stat "$(stat -f '%Sp %Su:%Sg %z bytes device=%Hr,%Lr' "$SERIAL_PORT" 2>/dev/null || stat -c '%A %U:%G %s bytes device=%t,%T' "$SERIAL_PORT")" \
  --argjson connections "$SERIAL_CONNECTIONS" \
  '{path:$path,stat:$stat,connections:$connections}')"
MQTT_EVIDENCE="$(jq -c '[.mqtt_uplinks[] | {
  sink_id, broker, client_id, username:(.username // null), password_env:(.password_env // null),
  tls_ca_path:(.tls_ca_path // null), topic_template, qos
}]' "$CONFIG_PATH")"
TOPIC_EVIDENCE="$(jq -c '[.data_configs[] | select(.enabled == true) | {
  config_id, device_id, connection_id:.protocol_connection_id, point_count:(.points | length),
  sink_id:.publish.sink_id, topic_template:.publish.topic_template, qos:.publish.qos
}]' "$CONFIG_PATH")"

write_preflight_report() {
  jq -n \
    --arg status passed \
    --arg mode preflight \
    --arg edgeId "$EDGE_ID" \
    --arg configVersion "$CONFIG_VERSION" \
    --arg packageSha256 "$PACKAGE_SHA256" \
    --arg serverName "$SERVER_NAME" \
    --argjson serial "$SERIAL_EVIDENCE" \
    --argjson mqtt "$MQTT_EVIDENCE" \
    --argjson dataConfigs "$TOPIC_EVIDENCE" \
    --argjson serverCertificate "$(cat "${WORK_DIR}/server-certificate.json")" \
    --argjson runtimeCertificate "$(cat "${WORK_DIR}/runtime-certificate.json")" \
    '{
      status:$status, mode:$mode, physicalDeviceExercised:false,
      edgeId:$edgeId, configVersion:$configVersion, packageSha256:$packageSha256,
      edgeLink:{transport:"TLS/mTLS",serverName:$serverName,serverCertificate:$serverCertificate,runtimeCertificate:$runtimeCertificate},
      serial:$serial, mqtt:$mqtt, dataConfigs:$dataConfigs,
      note:"Inputs validated only; no Cloud, Runtime, serial request, or MQTT publication was executed."
    }' | tee "$REPORT_PATH"
}

if [[ "$PREFLIGHT_ONLY" == "1" ]]; then
  write_preflight_report
  echo "field acceptance preflight evidence: $WORK_DIR"
  exit 0
fi

[[ -n "$SITE_ID" ]] || fail "EDGEOPS_FIELD_SITE_ID is required for a physical acceptance run"
[[ -n "$OPERATOR" ]] || fail "EDGEOPS_FIELD_OPERATOR is required for a physical acceptance run"
require_file "Cloud SQLite source database" "$CLOUD_DB_SOURCE"
[[ "$SERIAL_PORT" != /dev/ttys* && "$SERIAL_PORT" != /dev/pts/* ]] || \
  fail "pseudo terminals cannot produce physical field acceptance evidence"

for port in "$HTTP_PORT" "$GATEWAY_PORT"; do
  [[ "$port" =~ ^[1-9][0-9]*$ ]] || fail "invalid TCP port: $port"
  nc -z 127.0.0.1 "$port" >/dev/null 2>&1 && fail "acceptance port is already in use: $port"
done

"${ROOT_DIR}/scripts/cloud-state.sh" backup \
  "$CLOUD_DB_SOURCE" "${WORK_DIR}/cloud.sqlite" >"${WORK_DIR}/cloud-database-backup.log"
CLOUD_SNAPSHOT_SHA256="$(shasum -a 256 "${WORK_DIR}/cloud.sqlite" | awk '{print $1}')"

cargo build --release -p cloud-api -p edge-runtime >/dev/null
DATABASE_URL="sqlite://${WORK_DIR}/cloud.sqlite?mode=rwc"
EDGEOPS_CLOUD_DB="$DATABASE_URL" \
EDGEOPS_HTTP_ADDR="127.0.0.1:${HTTP_PORT}" \
EDGEOPS_GATEWAY_ADDR="127.0.0.1:${GATEWAY_PORT}" \
EDGEOPS_GATEWAY_TLS_CERT="$SERVER_CERT" \
EDGEOPS_GATEWAY_TLS_KEY="$SERVER_KEY" \
EDGEOPS_GATEWAY_TLS_CLIENT_CA="$RUNTIME_CA" \
EDGEOPS_API_AUTH_MODE=required \
EDGEOPS_BOOTSTRAP_MODE=empty \
EDGEOPS_ADMIN_TOKEN="$ADMIN_TOKEN" \
RUST_LOG=info \
"$CLOUD_BIN" >"${WORK_DIR}/cloud.log" 2>&1 &
CLOUD_PID=$!

READY=0
for _ in $(seq 1 100); do
  if curl -fsS --max-time 1 "http://127.0.0.1:${HTTP_PORT}/health/ready" >/dev/null 2>&1; then
    READY=1
    break
  fi
  kill -0 "$CLOUD_PID" 2>/dev/null || {
    tail -80 "${WORK_DIR}/cloud.log" >&2
    fail "cloud-api exited before readiness"
  }
  sleep 0.1
done
[[ "$READY" -eq 1 ]] || fail "cloud-api readiness timed out"

AUTH_HEADER="Authorization: Bearer ${ADMIN_TOKEN}"
EDGE_NODES="$(curl -fsS --max-time 15 -H "$AUTH_HEADER" \
  "http://127.0.0.1:${HTTP_PORT}/api/edge-nodes")"
CATALOG_EDGE="$(printf '%s' "$EDGE_NODES" | jq -ec --arg edge "$EDGE_ID" \
  '.[] | select(.edgeId == $edge)')" || fail "Cloud database does not contain edge identity: $EDGE_ID"
printf '%s\n' "$CATALOG_EDGE" | jq '.' >"${WORK_DIR}/catalog-edge.json"

RELEASE_RESPONSE="$(curl -fsS --max-time 15 -X POST \
  -H "$AUTH_HEADER" -H 'Content-Type: application/json' \
  --data-binary "@${CONFIG_PATH}" \
  "http://127.0.0.1:${HTTP_PORT}/api/releases")"
RELEASE_ID="$(printf '%s' "$RELEASE_RESPONSE" | jq -er '.release_id')"
DESIRED_VERSION="$(printf '%s' "$RELEASE_RESPONSE" | jq -er '.desired_version')"
[[ "$DESIRED_VERSION" == "$CONFIG_VERSION" ]] || fail "Cloud release version differs from package version"

TOKEN_RESPONSE="$(curl -fsS --max-time 15 -X POST -H "$AUTH_HEADER" \
  "http://127.0.0.1:${HTTP_PORT}/api/edge-nodes/${EDGE_ID}/access-token")"
RUNTIME_ACCESS_TOKEN="$(printf '%s' "$TOKEN_RESPONSE" | jq -er '.accessToken')"
export "${TOKEN_ENV_NAME}=${RUNTIME_ACCESS_TOKEN}"
unset RUNTIME_ACCESS_TOKEN TOKEN_RESPONSE

RUNTIME_ID="field-${SITE_ID//[^A-Za-z0-9_-]/-}-$(date -u '+%Y%m%dT%H%M%SZ')"
RUNTIME_STARTED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
"$RUNTIME_BIN" \
  --edge-id "$EDGE_ID" \
  --runtime-id "$RUNTIME_ID" \
  --runtime-db "${WORK_DIR}/runtime.rocksdb" \
  --storage "${WORK_DIR}/telemetry.jsonl" \
  --cloud-gateway-addr "127.0.0.1:${GATEWAY_PORT}" \
  --edgelink-tls-ca "$SERVER_CA" \
  --edgelink-tls-cert "$RUNTIME_CERT" \
  --edgelink-tls-key "$RUNTIME_KEY" \
  --edgelink-tls-server-name "$SERVER_NAME" \
  --access-token-env "$TOKEN_ENV_NAME" \
  --mqtt-uplink \
  >"${WORK_DIR}/runtime.log" 2>&1
RUNTIME_FINISHED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

PUBLISHED_COUNT="$(sed -n 's/.*mqtt_messages_published=\([0-9][0-9]*\).*/\1/p' "${WORK_DIR}/runtime.log" | tail -1)"
SAMPLES_COUNT="$(sed -n 's/.*samples_collected=\([0-9][0-9]*\).*/\1/p' "${WORK_DIR}/runtime.log" | tail -1)"
ACKED_COUNT="$(sed -n 's/.*acked_message_count=\([0-9][0-9]*\).*/\1/p' "${WORK_DIR}/runtime.log" | tail -1)"
[[ "${SAMPLES_COUNT:-0}" -gt 0 ]] || {
  tail -120 "${WORK_DIR}/runtime.log" >&2
  fail "Runtime did not collect any sample from the physical serial device"
}
[[ "${PUBLISHED_COUNT:-0}" -gt 0 ]] || {
  tail -120 "${WORK_DIR}/runtime.log" >&2
  fail "Runtime did not publish any MQTT message"
}
[[ "${ACKED_COUNT:-0}" -gt 0 ]] || fail "Runtime did not receive an EdgeLink acknowledgement"

RUNTIME_STATUS="$(curl -fsS --max-time 15 -H "$AUTH_HEADER" "http://127.0.0.1:${HTTP_PORT}/api/runtime-status")"
RELEASES="$(curl -fsS --max-time 15 -H "$AUTH_HEADER" "http://127.0.0.1:${HTTP_PORT}/api/releases")"
printf '%s' "$RUNTIME_STATUS" | jq -e \
  --arg edge "$EDGE_ID" --arg runtime "$RUNTIME_ID" --arg version "$CONFIG_VERSION" \
  'any(.edges[]; .edge_id == $edge and .runtime_id == $runtime and .config_version == $version and .cloud_sync.connected == true and .cloud_sync.reported_version == $version and (.local_store.backend | startswith("rocksdb")))' \
  >/dev/null || fail "Runtime status does not prove the applied configuration"
printf '%s' "$RELEASES" | jq -e \
  --arg edge "$EDGE_ID" --arg version "$CONFIG_VERSION" \
  'any(.applyResults[]; .edgeId == $edge and .desiredVersion == $version and .reportedVersion == $version and .result == "已应用")' \
  >/dev/null || fail "release acknowledgement was not recorded"

kill -TERM "$CLOUD_PID"
wait "$CLOUD_PID"
CLOUD_PID=""
grep -q 'shutdown signal received' "${WORK_DIR}/cloud.log" || fail "Cloud did not log graceful shutdown"

jq -n \
  --arg status passed \
  --arg mode physical-field \
  --arg siteId "$SITE_ID" \
  --arg operator "$OPERATOR" \
  --arg edgeId "$EDGE_ID" \
  --arg runtimeId "$RUNTIME_ID" \
  --arg configVersion "$CONFIG_VERSION" \
  --arg releaseId "$RELEASE_ID" \
  --arg packageSha256 "$PACKAGE_SHA256" \
  --arg cloudSnapshotSha256 "$CLOUD_SNAPSHOT_SHA256" \
  --arg startedAt "$RUNTIME_STARTED_AT" \
  --arg finishedAt "$RUNTIME_FINISHED_AT" \
  --argjson samplesCollected "$SAMPLES_COUNT" \
  --argjson mqttMessagesPublished "$PUBLISHED_COUNT" \
  --argjson edgeLinkMessagesAcknowledged "$ACKED_COUNT" \
  --argjson serial "$SERIAL_EVIDENCE" \
  --argjson mqtt "$MQTT_EVIDENCE" \
  --argjson dataConfigs "$TOPIC_EVIDENCE" \
  --argjson catalogEdge "$CATALOG_EDGE" \
  --argjson runtimeStatus "$RUNTIME_STATUS" \
  --argjson releases "$RELEASES" \
  '{
    status:$status, mode:$mode, physicalDeviceExercised:true,
    siteId:$siteId, operator:$operator, startedAt:$startedAt, finishedAt:$finishedAt,
    edgeId:$edgeId, runtimeId:$runtimeId, configVersion:$configVersion, releaseId:$releaseId,
    packageSha256:$packageSha256, cloudSnapshotSha256:$cloudSnapshotSha256,
    catalogEdge:$catalogEdge, serial:$serial, mqtt:$mqtt, dataConfigs:$dataConfigs,
    samplesCollected:$samplesCollected,
    mqttMessagesPublished:$mqttMessagesPublished, edgeLinkMessagesAcknowledged:$edgeLinkMessagesAcknowledged,
    runtimeStatus:$runtimeStatus, releases:$releases,
    evidence:{
      cloudLog:"cloud.log",runtimeLog:"runtime.log",database:"cloud.sqlite",
      cloudDatabaseBackupLog:"cloud-database-backup.log",catalogEdge:"catalog-edge.json",
      runtimeStore:"runtime.rocksdb"
    }
  }' | tee "$REPORT_PATH"

echo "physical field acceptance evidence: $WORK_DIR"
