#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CLOUD_BIN="${EDGEOPS_DEPLOY_SMOKE_CLOUD_BIN:-${ROOT_DIR}/target/debug/cloud-api}"
HTTP_PORT="${EDGEOPS_DEPLOY_SMOKE_HTTP_PORT:-18253}"
GATEWAY_PORT="${EDGEOPS_DEPLOY_SMOKE_GATEWAY_PORT:-19253}"
WORK_DIR="${EDGEOPS_DEPLOY_SMOKE_WORK_DIR:-${ROOT_DIR}/target/deployment-smoke-$$}"
REPORT_PATH="${EDGEOPS_DEPLOY_SMOKE_REPORT:-${WORK_DIR}/report.json}"
FIXTURE_DIR="${EDGEOPS_DEPLOY_SMOKE_FIXTURE_DIR:-${ROOT_DIR}/crates/cloud-api/tests/fixtures/edgelink}"
VIEWER_TOKEN="deployment-smoke-viewer-token-32"
OPERATOR_TOKEN="deployment-smoke-operator-token-32"
ADMIN_TOKEN="deployment-smoke-admin-token-32"

for command in curl jq nc rg; do
  command -v "$command" >/dev/null || {
    echo "missing required deployment smoke command: $command" >&2
    exit 2
  }
done

for path in \
  deploy/systemd/edgeops-cloud.service \
  deploy/systemd/edgeops-runtime@.service \
  deploy/env/cloud.env.example \
  deploy/env/runtime.env.example; do
  [[ -s "${ROOT_DIR}/${path}" ]] || {
    echo "missing deployment artifact: ${path}" >&2
    exit 2
  }
done

for path in ca.pem server.pem server-key.pem; do
  [[ -s "${FIXTURE_DIR}/${path}" ]] || {
    echo "missing deployment smoke TLS fixture: ${path}" >&2
    exit 2
  }
done

rg -q '^EnvironmentFile=/etc/edgeops/cloud.env$' \
  "${ROOT_DIR}/deploy/systemd/edgeops-cloud.service"
rg -q -- '--access-token-env EDGEOPS_EDGE_TOKEN' \
  "${ROOT_DIR}/deploy/systemd/edgeops-runtime@.service"
rg -q -- '--edgelink-daemon' "${ROOT_DIR}/deploy/systemd/edgeops-runtime@.service"
rg -q -- '--mqtt-uplink' "${ROOT_DIR}/deploy/systemd/edgeops-runtime@.service"
rg -q '^EDGEOPS_API_AUTH_MODE=required$' "${ROOT_DIR}/deploy/env/cloud.env.example"
rg -q '^EDGEOPS_BOOTSTRAP_MODE=empty$' "${ROOT_DIR}/deploy/env/cloud.env.example"
rg -q '^EDGEOPS_CONSOLE_DIST=/opt/edgeops/console$' "${ROOT_DIR}/deploy/env/cloud.env.example"

for port in "$HTTP_PORT" "$GATEWAY_PORT"; do
  if nc -z 127.0.0.1 "$port" >/dev/null 2>&1; then
    echo "deployment smoke port is already in use: ${port}" >&2
    exit 2
  fi
done

mkdir -p "$WORK_DIR" "${WORK_DIR}/console" "$(dirname "$REPORT_PATH")"
cargo build -p cloud-api >/dev/null
cp -R "${ROOT_DIR}/web/console/dist/." "${WORK_DIR}/console/"

EDGEOPS_CLOUD_DB="sqlite://${WORK_DIR}/cloud.sqlite?mode=rwc" \
EDGEOPS_HTTP_ADDR="127.0.0.1:${HTTP_PORT}" \
EDGEOPS_GATEWAY_ADDR="127.0.0.1:${GATEWAY_PORT}" \
EDGEOPS_CONSOLE_DIST="${WORK_DIR}/console" \
EDGEOPS_API_AUTH_MODE=required \
EDGEOPS_BOOTSTRAP_MODE=empty \
EDGEOPS_VIEWER_TOKEN="$VIEWER_TOKEN" \
EDGEOPS_OPERATOR_TOKEN="$OPERATOR_TOKEN" \
EDGEOPS_ADMIN_TOKEN="$ADMIN_TOKEN" \
EDGEOPS_GATEWAY_TLS_CERT="${FIXTURE_DIR}/server.pem" \
EDGEOPS_GATEWAY_TLS_KEY="${FIXTURE_DIR}/server-key.pem" \
EDGEOPS_GATEWAY_TLS_CLIENT_CA="${FIXTURE_DIR}/ca.pem" \
RUST_LOG=cloud_api=info \
"$CLOUD_BIN" >"${WORK_DIR}/cloud.log" 2>&1 &
CLOUD_PID=$!

cleanup() {
  kill -TERM "$CLOUD_PID" 2>/dev/null || true
  wait "$CLOUD_PID" 2>/dev/null || true
}
on_exit() {
  status=$?
  if [[ "$status" -ne 0 ]]; then
    echo "deployment smoke failed; evidence retained at: ${WORK_DIR}" >&2
    tail -80 "${WORK_DIR}/cloud.log" >&2 || true
  fi
  cleanup
  return "$status"
}
trap on_exit EXIT
trap 'exit 130' INT TERM

READY=0
for _ in $(seq 1 60); do
  if curl -fsS --max-time 1 "http://127.0.0.1:${HTTP_PORT}/health/ready" \
      >"${WORK_DIR}/ready.json" 2>/dev/null; then
    READY=1
    break
  fi
  kill -0 "$CLOUD_PID" 2>/dev/null || break
  sleep 0.25
done
[[ "$READY" -eq 1 ]] || {
  echo "production-shaped cloud did not become ready" >&2
  exit 1
}

ANONYMOUS_STATUS="$(curl -sS -o "${WORK_DIR}/anonymous.json" -w '%{http_code}' \
  "http://127.0.0.1:${HTTP_PORT}/api/summary")"
[[ "$ANONYMOUS_STATUS" == "401" ]] || {
  echo "production-shaped cloud did not reject anonymous API access" >&2
  exit 1
}

curl -fsS -H "Authorization: Bearer ${VIEWER_TOKEN}" \
  "http://127.0.0.1:${HTTP_PORT}/api/summary" >"${WORK_DIR}/summary.json"
jq -e '.edge_count == 0 and .pending_release_count == 0' \
  "${WORK_DIR}/summary.json" >/dev/null

curl -fsS "http://127.0.0.1:${HTTP_PORT}/" >"${WORK_DIR}/index.html"
ASSET_PATH="$(rg -o 'assets/index-[A-Za-z0-9_-]+\.js' "${WORK_DIR}/index.html" | head -1)"
[[ -n "$ASSET_PATH" ]] || {
  echo "deployed console index does not reference a JavaScript asset" >&2
  exit 1
}
curl -fsS "http://127.0.0.1:${HTTP_PORT}/${ASSET_PATH}" \
  >"${WORK_DIR}/console.js"
[[ -s "${WORK_DIR}/console.js" ]] || {
  echo "deployed console JavaScript asset is empty" >&2
  exit 1
}

nc -z 127.0.0.1 "$GATEWAY_PORT" >/dev/null 2>&1 || {
  echo "EdgeLink mTLS listener is not reachable" >&2
  exit 1
}

kill -TERM "$CLOUD_PID"
wait "$CLOUD_PID"
trap - EXIT INT TERM

jq -n \
  --arg generatedAt "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" \
  --arg consoleAsset "$ASSET_PATH" \
  --arg database "${WORK_DIR}/cloud.sqlite" \
  '{
    status:"passed",
    mode:"production-deployment-smoke",
    generatedAt:$generatedAt,
    checks:{
      systemdArtifacts:true,
      environmentTemplates:true,
      requiredAuthentication:true,
      emptyBootstrap:true,
      externalConsoleDirectory:true,
      consoleAssetServed:$consoleAsset,
      edgeLinkMutualTlsListener:true,
      gracefulShutdown:true,
      sqliteDatabase:$database
    }
  }' >"$REPORT_PATH"

jq '.' "$REPORT_PATH"
echo "deployment smoke evidence: $WORK_DIR"
