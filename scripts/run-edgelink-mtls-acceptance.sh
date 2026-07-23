#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CLOUD_BIN="${EDGELINK_ACCEPTANCE_CLOUD_BIN:-${ROOT_DIR}/target/debug/cloud-api}"
RUNTIME_BIN="${EDGELINK_ACCEPTANCE_RUNTIME_BIN:-${ROOT_DIR}/target/debug/edge-runtime}"
HTTP_PORT="${EDGELINK_ACCEPTANCE_HTTP_PORT:-18081}"
GATEWAY_PORT="${EDGELINK_ACCEPTANCE_GATEWAY_PORT:-18082}"
WORK_DIR="${EDGELINK_ACCEPTANCE_WORK_DIR:-${ROOT_DIR}/target/edgelink-acceptance-$$}"
REPORT_PATH="${EDGELINK_ACCEPTANCE_REPORT:-${WORK_DIR}/report.json}"
FIXTURE_DIR="${EDGELINK_ACCEPTANCE_FIXTURE_DIR:-${ROOT_DIR}/crates/cloud-api/tests/fixtures/edgelink}"
EDGE_ID="edge-dev"
RUNTIME_ID="runtime-process-acceptance"
TOKEN_ENV_NAME="EDGEOPS_ACCEPTANCE_ACCESS_TOKEN"

for command in curl jq nc; do
  command -v "$command" >/dev/null || {
    echo "missing required command: $command" >&2
    exit 2
  }
done

for path in ca.pem server.pem server-key.pem client.pem client-key.pem; do
  [[ -f "${FIXTURE_DIR}/${path}" ]] || {
    echo "missing EdgeLink acceptance fixture: ${FIXTURE_DIR}/${path}" >&2
    exit 2
  }
done

"${ROOT_DIR}/scripts/edgelink-certificates.sh" check \
  "${FIXTURE_DIR}/server.pem" \
  "${FIXTURE_DIR}/server-key.pem" \
  "${FIXTURE_DIR}/ca.pem" \
  "${EDGELINK_ACCEPTANCE_MIN_CERT_DAYS:-30}" >/dev/null

for port in "$HTTP_PORT" "$GATEWAY_PORT"; do
  if nc -z 127.0.0.1 "$port" >/dev/null 2>&1; then
    echo "acceptance port is already in use: $port" >&2
    exit 2
  fi
done

mkdir -p "$WORK_DIR" "$(dirname "$REPORT_PATH")"
cargo build -p cloud-api -p edge-runtime >/dev/null

DATABASE_URL="sqlite://${WORK_DIR}/cloud.sqlite?mode=rwc"
EDGEOPS_CLOUD_DB="$DATABASE_URL" \
EDGEOPS_HTTP_ADDR="127.0.0.1:${HTTP_PORT}" \
EDGEOPS_GATEWAY_ADDR="127.0.0.1:${GATEWAY_PORT}" \
EDGEOPS_GATEWAY_TLS_CERT="${FIXTURE_DIR}/server.pem" \
EDGEOPS_GATEWAY_TLS_KEY="${FIXTURE_DIR}/server-key.pem" \
EDGEOPS_GATEWAY_TLS_CLIENT_CA="${FIXTURE_DIR}/ca.pem" \
EDGEOPS_BOOTSTRAP_MODE=demo \
RUST_LOG=info \
"$CLOUD_BIN" >"${WORK_DIR}/cloud.log" 2>&1 &
CLOUD_PID=$!

cleanup() {
  kill "$CLOUD_PID" 2>/dev/null || true
  wait "$CLOUD_PID" 2>/dev/null || true
  unset "$TOKEN_ENV_NAME" 2>/dev/null || true
}
on_exit() {
  status=$?
  if [[ "$status" -ne 0 ]]; then
    echo "EdgeLink mTLS acceptance failed; evidence retained at: $WORK_DIR" >&2
  fi
  cleanup
  return "$status"
}
trap on_exit EXIT
trap 'exit 130' INT TERM

API_READY=0
for _ in $(seq 1 60); do
  if curl -fsS --max-time 1 "http://127.0.0.1:${HTTP_PORT}/api/summary" >/dev/null 2>&1; then
    API_READY=1
    break
  fi
  if ! kill -0 "$CLOUD_PID" 2>/dev/null; then
    echo "cloud-api exited before its HTTP API became ready" >&2
    tail -80 "${WORK_DIR}/cloud.log" >&2
    exit 1
  fi
  sleep 1
done
if [[ "$API_READY" -ne 1 ]]; then
  echo "cloud-api HTTP API did not become ready" >&2
  tail -80 "${WORK_DIR}/cloud.log" >&2
  exit 1
fi

TOKEN_JSON="$(curl -fsS --max-time 10 \
  -X POST \
  "http://127.0.0.1:${HTTP_PORT}/api/edge-nodes/${EDGE_ID}/access-token")"
ACCESS_TOKEN="$(printf '%s' "$TOKEN_JSON" | jq -er '.accessToken')"
export "${TOKEN_ENV_NAME}=${ACCESS_TOKEN}"
unset ACCESS_TOKEN TOKEN_JSON

PUBLISH_JSON="$(curl -fsS --max-time 10 \
  -X POST \
  "http://127.0.0.1:${HTTP_PORT}/api/edges/${EDGE_ID}/releases/publish")"
DESIRED_VERSION="$(printf '%s' "$PUBLISH_JSON" | jq -er \
  --arg edge "$EDGE_ID" '.applyResults[] | select(.edgeId == $edge and .result == "等待下发") | .desiredVersion' | head -1)"
[[ -n "$DESIRED_VERSION" ]] || {
  echo "cloud-api did not create a pending release for $EDGE_ID" >&2
  exit 1
}

COMMON_RUNTIME_ARGS=(
  --edge-id "$EDGE_ID"
  --cloud-gateway-addr "127.0.0.1:${GATEWAY_PORT}"
  --edgelink-tls-ca "${FIXTURE_DIR}/ca.pem"
  --edgelink-tls-cert "${FIXTURE_DIR}/client.pem"
  --edgelink-tls-key "${FIXTURE_DIR}/client-key.pem"
  --edgelink-tls-server-name localhost
)

if "$RUNTIME_BIN" \
  "${COMMON_RUNTIME_ARGS[@]}" \
  --runtime-id runtime-rejected-acceptance \
  --runtime-db "${WORK_DIR}/rejected.rocksdb" \
  >"${WORK_DIR}/rejected.log" 2>&1; then
  echo "EdgeLink accepted an mTLS runtime without an access token" >&2
  exit 1
fi
grep -q "invalid or missing edge access token" "${WORK_DIR}/rejected.log" || {
  echo "runtime was rejected for an unexpected reason" >&2
  tail -80 "${WORK_DIR}/rejected.log" >&2
  exit 1
}

"$RUNTIME_BIN" \
  "${COMMON_RUNTIME_ARGS[@]}" \
  --runtime-id "$RUNTIME_ID" \
  --runtime-db "${WORK_DIR}/runtime.rocksdb" \
  --access-token-env "$TOKEN_ENV_NAME" \
  >"${WORK_DIR}/runtime.log" 2>&1

RUNTIME_STATUS="$(curl -fsS --max-time 10 "http://127.0.0.1:${HTTP_PORT}/api/runtime-status")"
RELEASES="$(curl -fsS --max-time 10 "http://127.0.0.1:${HTTP_PORT}/api/releases")"
EDGE_NODES="$(curl -fsS --max-time 10 "http://127.0.0.1:${HTTP_PORT}/api/edge-nodes?page=1&pageSize=100")"

printf '%s' "$RUNTIME_STATUS" | jq -e \
  --arg edge "$EDGE_ID" \
  --arg runtime "$RUNTIME_ID" \
  --arg version "$DESIRED_VERSION" \
  '((.healthyEdgeCount + .degradedEdgeCount + .criticalEdgeCount) >= 1) and any(.edges[]; .edge_id == $edge and .runtime_id == $runtime and .config_version == $version and .cloud_sync.connected == true and .cloud_sync.reported_version == $version and (.local_store.backend | startswith("rocksdb")) and (.health == "Healthy" or .health == "Degraded" or .health == "Critical"))' \
  >/dev/null
printf '%s' "$RELEASES" | jq -e \
  --arg edge "$EDGE_ID" \
  --arg version "$DESIRED_VERSION" \
  'any(.applyResults[]; .edgeId == $edge and .desiredVersion == $version and .reportedVersion == $version and .result == "已应用")' \
  >/dev/null
printf '%s' "$EDGE_NODES" | jq -e \
  --arg edge "$EDGE_ID" \
  --arg runtime "$RUNTIME_ID" \
  --arg version "$DESIRED_VERSION" \
  'any(.items[]; .edgeId == $edge and .runtimeId == $runtime and .reportedProductVersion == $version and (.capabilities | index("registration:runtime-discovered")) != null)' \
  >/dev/null

jq -n \
  --arg transport "EdgeLink/TLS1.3/mTLS" \
  --arg edgeId "$EDGE_ID" \
  --arg runtimeId "$RUNTIME_ID" \
  --arg desiredVersion "$DESIRED_VERSION" \
  --argjson unauthenticatedRuntimeRejected true \
  --argjson runtimeStatus "$RUNTIME_STATUS" \
  --argjson releases "$RELEASES" \
  --argjson edgeNodes "$EDGE_NODES" \
  '{
    transport: $transport,
    edgeId: $edgeId,
    runtimeId: $runtimeId,
    desiredVersion: $desiredVersion,
    unauthenticatedRuntimeRejected: $unauthenticatedRuntimeRejected,
    acceptanceScope: "EdgeLink mTLS identity, token authorization, config deployment, RocksDB apply, acknowledgement, capability registration, and runtime metric delivery. Southbound device health is verified by protocol-specific gates.",
    runtimeStatus: $runtimeStatus,
    releases: $releases,
    edgeNodes: $edgeNodes
  }' | tee "$REPORT_PATH"

echo "EdgeLink mTLS acceptance evidence: $WORK_DIR"
