#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VELAMQ_REPO="${VELAMQ_REPO:?set VELAMQ_REPO to a VelaMQ source checkout}"
VELAMQ_BIN="${VELAMQ_BIN:-${VELAMQ_REPO}/target/debug/velamqd}"
EDGE_ACCEPTANCE_BIN="${EDGE_ACCEPTANCE_BIN:-${ROOT_DIR}/target/debug/mqtt-acceptance}"

API_PORT="${VELAMQ_ACCEPTANCE_API_PORT:-18091}"
MQTT_PORT="${VELAMQ_ACCEPTANCE_MQTT_PORT:-18890}"
MQTTS_PORT="${VELAMQ_ACCEPTANCE_MQTTS_PORT:-18891}"
CLUSTER_PORT="${VELAMQ_ACCEPTANCE_CLUSTER_PORT:-55061}"
WORK_DIR="${VELAMQ_ACCEPTANCE_WORK_DIR:-${ROOT_DIR}/target/velamq-acceptance-$$}"
REPORT_PATH="${VELAMQ_ACCEPTANCE_REPORT:-${WORK_DIR}/report.json}"
BOOTSTRAP_PASSWORD="${VELAMQ_ACCEPTANCE_ADMIN_PASSWORD:-admin-$RANDOM-$RANDOM}"
MQTT_USERNAME="${VELAMQ_ACCEPTANCE_MQTT_USERNAME:-edge-runtime}"
MQTT_PASSWORD="${VELAMQ_ACCEPTANCE_MQTT_PASSWORD:-mqtt-$RANDOM-$RANDOM}"
PASSWORD_ENV_NAME="EDGE_ACCEPTANCE_MQTT_PASSWORD"

for command in curl jq nc; do
  command -v "$command" >/dev/null || {
    echo "missing required command: $command" >&2
    exit 2
  }
done

[[ -x "$VELAMQ_BIN" ]] || {
  echo "VelaMQ binary is missing: $VELAMQ_BIN" >&2
  echo "build it with: cargo build -p velamqd" >&2
  exit 2
}

mkdir -p "$WORK_DIR" "$(dirname "$REPORT_PATH")"
cargo build -p edge-runtime --bin mqtt-acceptance >/dev/null

VELAMQ_NODE_ID="edge-acceptance-$$" \
VELAMQ_CLUSTER_BIND_ADDR="127.0.0.1:${CLUSTER_PORT}" \
VELAMQ_ADVERTISE_ADDR="http://127.0.0.1:${CLUSTER_PORT}" \
VELAMQ_CLUSTER_SHARDS_REPLICATION_FACTOR=1 \
VELAMQ_ROCKSDB_DATA_DIR="${WORK_DIR}/rocksdb" \
VELAMQ_ROCKSDB_BACKUP_DIR="${WORK_DIR}/backups" \
VELAMQ_API_BIND_ADDR="127.0.0.1:${API_PORT}" \
VELAMQ_API_STATIC_DIR="${VELAMQ_REPO}/static" \
VELAMQ_LOG_DIR="${WORK_DIR}/logs" \
VELAMQ_CONSOLE_BOOTSTRAP_PASSWORD="$BOOTSTRAP_PASSWORD" \
RUST_LOG=info \
"$VELAMQ_BIN" >"${WORK_DIR}/velamq.log" 2>&1 &
VELAMQ_PID=$!

cleanup() {
  kill "$VELAMQ_PID" 2>/dev/null || true
  wait "$VELAMQ_PID" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

API_READY=0
for _ in $(seq 1 60); do
  if curl -fsS --max-time 1 "http://127.0.0.1:${API_PORT}/api/console/config" >/dev/null 2>&1; then
    API_READY=1
    break
  fi
  if ! kill -0 "$VELAMQ_PID" 2>/dev/null; then
    echo "VelaMQ exited before its management API became ready" >&2
    tail -80 "${WORK_DIR}/velamq.log" >&2
    exit 1
  fi
  sleep 1
done
if [[ "$API_READY" -ne 1 ]]; then
  echo "VelaMQ management API did not become ready" >&2
  tail -80 "${WORK_DIR}/velamq.log" >&2
  exit 1
fi

LOGIN_JSON="$(curl -fsS --max-time 10 \
  -X POST \
  -H 'Content-Type: application/json' \
  --data "{\"username\":\"admin\",\"password\":\"${BOOTSTRAP_PASSWORD}\"}" \
  "http://127.0.0.1:${API_PORT}/api/console/login")"
TOKEN="$(printf '%s' "$LOGIN_JSON" | jq -er '.token')"
AUTH_HEADER="Authorization: Bearer ${TOKEN}"

CERT_JSON="$(curl -fsS --max-time 30 \
  -X POST \
  -H "$AUTH_HEADER" \
  -H 'Content-Type: application/json' \
  --data '{"name":"edge-acceptance-cert","common_name":"localhost","subject_alt_names":["localhost","127.0.0.1"],"valid_days":30}' \
  "http://127.0.0.1:${API_PORT}/api/certificates/servers/self-signed")"
CERT_ID="$(printf '%s' "$CERT_JSON" | jq -er '.id')"

curl -fsS --max-time 10 \
  -H "$AUTH_HEADER" \
  "http://127.0.0.1:${API_PORT}/api/certificates/servers/${CERT_ID}/download" \
  | jq -er '.ca_certificate_pem' >"${WORK_DIR}/velamq-ca.pem"

ENDPOINTS_JSON="$(jq -cn \
  --argjson mqtt "$MQTT_PORT" \
  --argjson mqtts "$MQTTS_PORT" \
  --arg certificate "$CERT_ID" \
  '[
    {name:"edge-acceptance-mqtt",host:"127.0.0.1",port:$mqtt,transport:"tcp",websocket:false,proxy_protocol:false,tls:false,enabled:true,tls_require_client_cert:false,websocket_path:"/mqtt"},
    {name:"edge-acceptance-mqtts",host:"127.0.0.1",port:$mqtts,transport:"tls",websocket:false,proxy_protocol:false,tls:true,enabled:true,tls_certificate_id:$certificate,tls_require_client_cert:false,websocket_path:"/mqtt"}
  ]')"
curl -fsS --max-time 10 \
  -X PUT \
  -H "$AUTH_HEADER" \
  -H 'Content-Type: application/json' \
  --data-binary "$ENDPOINTS_JSON" \
  "http://127.0.0.1:${API_PORT}/api/endpoints" >/dev/null

AUTH_JSON="$(jq -cn \
  --arg username "$MQTT_USERNAME" \
  --arg password "$MQTT_PASSWORD" \
  '{name:"edge-runtime-auth",enabled:true,source:"Config",users:[{username:$username,password:$password,client_id_prefixes:["edgeops-acceptance-"],tags:["edge-runtime"]}]}')"
curl -fsS --max-time 10 \
  -X POST \
  -H "$AUTH_HEADER" \
  -H 'Content-Type: application/json' \
  --data-binary "$AUTH_JSON" \
  "http://127.0.0.1:${API_PORT}/api/auth" >/dev/null

for port in "$MQTT_PORT" "$MQTTS_PORT"; do
  for _ in $(seq 1 30); do
    nc -z 127.0.0.1 "$port" >/dev/null 2>&1 && break
    sleep 1
  done
  nc -z 127.0.0.1 "$port" >/dev/null 2>&1 || {
    echo "VelaMQ listener did not start on port $port" >&2
    exit 1
  }
done

if "$EDGE_ACCEPTANCE_BIN" \
  --broker "mqtts://127.0.0.1:${MQTTS_PORT}" \
  --tls-ca-path "${WORK_DIR}/velamq-ca.pem" \
  --topic edgeops/acceptance/unauthenticated \
  --qos 1 \
  --timeout-ms 3000 >"${WORK_DIR}/unauthenticated.log" 2>&1; then
  echo "VelaMQ accepted an unauthenticated MQTT connection" >&2
  exit 1
fi

export "$PASSWORD_ENV_NAME=$MQTT_PASSWORD"
"$EDGE_ACCEPTANCE_BIN" \
  --broker "mqtts://127.0.0.1:${MQTTS_PORT}" \
  --username "$MQTT_USERNAME" \
  --password-env "$PASSWORD_ENV_NAME" \
  --tls-ca-path "${WORK_DIR}/velamq-ca.pem" \
  --topic edgeops/acceptance/velamq-secure \
  --qos 1 \
  --timeout-ms 15000 | tee "$REPORT_PATH"

echo "VelaMQ acceptance evidence: $WORK_DIR"
