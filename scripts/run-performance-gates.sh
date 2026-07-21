#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HTTP_PORT="${EDGEOPS_PERF_HTTP_PORT:-18091}"
GATEWAY_PORT="${EDGEOPS_PERF_GATEWAY_PORT:-19091}"
REQUESTS="${EDGEOPS_PERF_REQUESTS:-1000}"
CONCURRENCY="${EDGEOPS_PERF_CONCURRENCY:-20}"
MIN_HTTP_RPS="${EDGEOPS_PERF_MIN_HTTP_RPS:-250}"
MAX_HTTP_P95_MS="${EDGEOPS_PERF_MAX_HTTP_P95_MS:-100}"
RUNTIME_ITERATIONS="${EDGEOPS_PERF_RUNTIME_ITERATIONS:-2000}"
RUNTIME_POINT_COUNT="${EDGEOPS_PERF_RUNTIME_POINT_COUNT:-32}"
MIN_RUNTIME_SPS="${EDGEOPS_PERF_MIN_RUNTIME_SPS:-10000}"
MAX_RUNTIME_P95_US="${EDGEOPS_PERF_MAX_RUNTIME_P95_US:-10000}"
WORK_DIR="${EDGEOPS_PERF_WORK_DIR:-${ROOT_DIR}/target/performance-gates-$$}"
REPORT_PATH="${EDGEOPS_PERF_REPORT:-${WORK_DIR}/report.json}"
CLOUD_BIN="${ROOT_DIR}/target/release/cloud-api"
RUNTIME_PERF_BIN="${ROOT_DIR}/target/release/runtime-performance"
ADMIN_TOKEN="edgeops-performance-admin-token-0001"
CLOUD_PID=""

for command in ab awk curl jq nc; do
  command -v "$command" >/dev/null || {
    echo "missing required command: $command" >&2
    exit 2
  }
done

for value in "$HTTP_PORT" "$GATEWAY_PORT" "$REQUESTS" "$CONCURRENCY"; do
  [[ "$value" =~ ^[1-9][0-9]*$ ]] || {
    echo "performance gate integer must be greater than zero: $value" >&2
    exit 2
  }
done

for port in "$HTTP_PORT" "$GATEWAY_PORT"; do
  if nc -z 127.0.0.1 "$port" >/dev/null 2>&1; then
    echo "performance gate port is already in use: $port" >&2
    exit 2
  fi
done

mkdir -p "$WORK_DIR" "$(dirname "$REPORT_PATH")"
cargo build --release -p cloud-api >/dev/null
cargo build --release -p edge-runtime --bin runtime-performance >/dev/null

cleanup() {
  if [[ -n "$CLOUD_PID" ]]; then
    kill -TERM "$CLOUD_PID" 2>/dev/null || true
    wait "$CLOUD_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT
trap 'exit 130' INT TERM

EDGEOPS_CLOUD_DB="sqlite://${WORK_DIR}/cloud.sqlite?mode=rwc" \
EDGEOPS_HTTP_ADDR="127.0.0.1:${HTTP_PORT}" \
EDGEOPS_GATEWAY_ADDR="127.0.0.1:${GATEWAY_PORT}" \
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
  if ! kill -0 "$CLOUD_PID" 2>/dev/null; then
    echo "cloud-api exited before the performance gate became ready" >&2
    tail -80 "${WORK_DIR}/cloud.log" >&2
    exit 1
  fi
  sleep 0.1
done
[[ "$READY" -eq 1 ]] || {
  echo "cloud-api readiness timed out" >&2
  exit 1
}

ab -n "$REQUESTS" -c "$CONCURRENCY" \
  -H "Authorization: Bearer ${ADMIN_TOKEN}" \
  "http://127.0.0.1:${HTTP_PORT}/api/summary" >"${WORK_DIR}/http-ab.txt"

HTTP_RPS="$(awk '/Requests per second:/ {print $4}' "${WORK_DIR}/http-ab.txt")"
HTTP_FAILED="$(awk '/Failed requests:/ {print $3}' "${WORK_DIR}/http-ab.txt")"
HTTP_P95_MS="$(awk '$1 == "95%" {print $2}' "${WORK_DIR}/http-ab.txt")"
[[ -n "$HTTP_RPS" && -n "$HTTP_FAILED" && -n "$HTTP_P95_MS" ]] || {
  echo "failed to parse ApacheBench output" >&2
  exit 1
}
[[ "$HTTP_FAILED" -eq 0 ]] || {
  echo "HTTP performance gate recorded failed requests: $HTTP_FAILED" >&2
  exit 1
}
awk -v actual="$HTTP_RPS" -v minimum="$MIN_HTTP_RPS" \
  'BEGIN { exit !(actual >= minimum) }' || {
  echo "HTTP requests/sec ${HTTP_RPS} is below ${MIN_HTTP_RPS}" >&2
  exit 1
}
awk -v actual="$HTTP_P95_MS" -v maximum="$MAX_HTTP_P95_MS" \
  'BEGIN { exit !(actual <= maximum) }' || {
  echo "HTTP P95 ${HTTP_P95_MS}ms exceeds ${MAX_HTTP_P95_MS}ms" >&2
  exit 1
}

"$RUNTIME_PERF_BIN" \
  --iterations "$RUNTIME_ITERATIONS" \
  --point-count "$RUNTIME_POINT_COUNT" \
  --min-samples-per-second "$MIN_RUNTIME_SPS" \
  --max-batch-p95-us "$MAX_RUNTIME_P95_US" \
  >"${WORK_DIR}/runtime.json"

kill -TERM "$CLOUD_PID"
wait "$CLOUD_PID"
CLOUD_PID=""
grep -q 'shutdown signal received' "${WORK_DIR}/cloud.log"
grep -q 'cloud agent stopped' "${WORK_DIR}/cloud.log"

jq -n \
  --arg status passed \
  --argjson requests "$REQUESTS" \
  --argjson concurrency "$CONCURRENCY" \
  --argjson requestsPerSecond "$HTTP_RPS" \
  --argjson failedRequests "$HTTP_FAILED" \
  --argjson p95Ms "$HTTP_P95_MS" \
  --argjson minimumRequestsPerSecond "$MIN_HTTP_RPS" \
  --argjson maximumP95Ms "$MAX_HTTP_P95_MS" \
  --argjson runtime "$(cat "${WORK_DIR}/runtime.json")" \
  '{
    status:$status,
    cloudApi:{
      requests:$requests,
      concurrency:$concurrency,
      requestsPerSecond:$requestsPerSecond,
      failedRequests:$failedRequests,
      p95Ms:$p95Ms,
      minimumRequestsPerSecond:$minimumRequestsPerSecond,
      maximumP95Ms:$maximumP95Ms,
      authentication:"required",
      sqlite:"enabled",
      gracefulShutdown:true
    },
    runtimeDsl:$runtime
  }' | tee "$REPORT_PATH"

echo "performance gate evidence: $WORK_DIR"
