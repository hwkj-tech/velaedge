#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${EDGEOPS_MODBUS_TCP_LAB_WORK_DIR:-${ROOT_DIR}/target/modbus-tcp-lab-acceptance-$$}"
REPORT_PATH="${EDGEOPS_MODBUS_TCP_LAB_REPORT:-${WORK_DIR}/report.json}"
ENDURANCE_REPORT="${WORK_DIR}/endurance-report.json"
TEST_LOG="${WORK_DIR}/tests.log"
ENDURANCE_LOG="${WORK_DIR}/endurance.log"
CONTAINER_LOG="${WORK_DIR}/modbus-device.log"
COMPOSE_FILE="${ROOT_DIR}/deploy/modbus-device/compose.yaml"
COMPOSE_PROJECT="velaedge-modbus-lab-$$"
HOST_PORT="${EDGEOPS_MODBUS_TCP_LAB_PORT:-15020}"
ENDPOINT="${EDGEOPS_MODBUS_TCP_LAB_ENDPOINT:-127.0.0.1:${HOST_PORT}}"
DURATION_SECONDS="${EDGEOPS_MODBUS_TCP_LAB_DURATION_SECONDS:-8}"
INTERVAL_MS="${EDGEOPS_MODBUS_TCP_LAB_INTERVAL_MS:-200}"
MINIMUM_CYCLES="${EDGEOPS_MODBUS_TCP_LAB_MINIMUM_CYCLES:-8}"
MAXIMUM_FAILURE_RATIO="${EDGEOPS_MODBUS_TCP_LAB_MAXIMUM_FAILURE_RATIO:-0.50}"
MANAGE_CONTAINER="${EDGEOPS_MODBUS_TCP_LAB_MANAGE_CONTAINER:-1}"
INJECT_OUTAGE="${EDGEOPS_MODBUS_TCP_LAB_INJECT_OUTAGE:-1}"
MQTT_BROKER="${EDGEOPS_MODBUS_TCP_LAB_MQTT_BROKER:-}"
STARTED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
STARTED_SECONDS="$(date +%s)"
ACCEPTANCE_PID=""
CONTAINER_STARTED=0

for command in cargo docker git jq nc rg shasum; do
  command -v "$command" >/dev/null || {
    echo "Modbus TCP lab acceptance: missing required command: $command" >&2
    exit 2
  }
done

case "$MANAGE_CONTAINER" in 0 | 1) ;; *) echo "EDGEOPS_MODBUS_TCP_LAB_MANAGE_CONTAINER must be 0 or 1" >&2; exit 2 ;; esac
case "$INJECT_OUTAGE" in 0 | 1) ;; *) echo "EDGEOPS_MODBUS_TCP_LAB_INJECT_OUTAGE must be 0 or 1" >&2; exit 2 ;; esac
if [[ "$INJECT_OUTAGE" == 1 && "$MANAGE_CONTAINER" != 1 ]]; then
  echo "Fault injection requires EDGEOPS_MODBUS_TCP_LAB_MANAGE_CONTAINER=1" >&2
  exit 2
fi

mkdir -p "$WORK_DIR" "$(dirname "$REPORT_PATH")"

compose() {
  MODBUS_HOST_PORT="$HOST_PORT" docker compose \
    --project-name "$COMPOSE_PROJECT" \
    --file "$COMPOSE_FILE" "$@"
}

cleanup() {
  if [[ -n "$ACCEPTANCE_PID" ]]; then
    kill "$ACCEPTANCE_PID" >/dev/null 2>&1 || true
    wait "$ACCEPTANCE_PID" >/dev/null 2>&1 || true
  fi
  if [[ "$CONTAINER_STARTED" == 1 ]]; then
    compose logs --no-color >"$CONTAINER_LOG" 2>&1 || true
    compose down --volumes --remove-orphans >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT INT TERM

wait_for_modbus() {
  local attempt
  for attempt in $(seq 1 80); do
    if nc -z 127.0.0.1 "$HOST_PORT" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.1
  done
  echo "Modbus TCP device did not become ready at 127.0.0.1:${HOST_PORT}" >&2
  return 1
}

TEST_STATUS=passed
{
  cargo test --manifest-path "${ROOT_DIR}/Cargo.toml" \
    -p edge-runtime --test modbus_tcp -- --nocapture
  cargo test --manifest-path "${ROOT_DIR}/Cargo.toml" \
    -p edge-runtime --test modbus_tcp_endurance -- --nocapture
} >"$TEST_LOG" 2>&1 || TEST_STATUS=failed

cargo build --manifest-path "${ROOT_DIR}/Cargo.toml" \
  -p edge-runtime --bin modbus-tcp-endurance >>"$TEST_LOG" 2>&1 || TEST_STATUS=failed

if [[ "$MANAGE_CONTAINER" == 1 ]]; then
  compose up --detach --build >"$CONTAINER_LOG" 2>&1
  CONTAINER_STARTED=1
  wait_for_modbus
fi

COMMAND=(
  "${ROOT_DIR}/target/debug/modbus-tcp-endurance"
  --endpoint "$ENDPOINT"
  --duration-seconds "$DURATION_SECONDS"
  --interval-ms "$INTERVAL_MS"
  --minimum-cycles "$MINIMUM_CYCLES"
  --maximum-failure-ratio "$MAXIMUM_FAILURE_RATIO"
  --rocksdb-path "${WORK_DIR}/runtime.rocksdb"
  --report "$ENDURANCE_REPORT"
)
if [[ "$INJECT_OUTAGE" == 1 ]]; then
  COMMAND+=(--require-recovery)
fi
if [[ -n "$MQTT_BROKER" ]]; then
  COMMAND+=(
    --mqtt-broker "$MQTT_BROKER"
    --mqtt-sink-id "${EDGEOPS_MODBUS_TCP_LAB_MQTT_SINK_ID:-modbus-lab}"
    --mqtt-client-id "${EDGEOPS_MODBUS_TCP_LAB_MQTT_CLIENT_ID:-modbus-lab-runtime-$$}"
    --mqtt-version "${EDGEOPS_MODBUS_TCP_LAB_MQTT_VERSION:-3.1.1}"
  )
  if [[ -n "${EDGEOPS_MODBUS_TCP_LAB_MQTT_USERNAME:-}" ]]; then
    COMMAND+=(--mqtt-username "$EDGEOPS_MODBUS_TCP_LAB_MQTT_USERNAME")
  fi
  if [[ -n "${EDGEOPS_MODBUS_TCP_LAB_MQTT_PASSWORD_ENV:-}" ]]; then
    COMMAND+=(--mqtt-password-env "$EDGEOPS_MODBUS_TCP_LAB_MQTT_PASSWORD_ENV")
  fi
  if [[ -n "${EDGEOPS_MODBUS_TCP_LAB_MQTT_CA_PATH:-}" ]]; then
    COMMAND+=(--mqtt-ca-path "$EDGEOPS_MODBUS_TCP_LAB_MQTT_CA_PATH")
  fi
fi

ENDURANCE_STATUS=passed
"${COMMAND[@]}" >"$ENDURANCE_LOG" 2>&1 &
ACCEPTANCE_PID=$!

if [[ "$INJECT_OUTAGE" == 1 ]]; then
  sleep 2
  compose stop modbus-device >>"$CONTAINER_LOG" 2>&1
  sleep 1
  compose up --detach --no-build modbus-device >>"$CONTAINER_LOG" 2>&1
  wait_for_modbus
fi

if ! wait "$ACCEPTANCE_PID"; then
  ENDURANCE_STATUS=failed
fi
ACCEPTANCE_PID=""

FINISHED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
DURATION_SECONDS_OBSERVED=$(( $(date +%s) - STARTED_SECONDS ))
TEST_COUNT="$(rg -o '[0-9]+ passed' "$TEST_LOG" | awk '{total += $1} END {print total + 0}')"
GIT_COMMIT="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || printf unknown)"
GIT_DIRTY=false
if [[ -n "$(git -C "$ROOT_DIR" status --porcelain 2>/dev/null)" ]]; then
  GIT_DIRTY=true
fi

if [[ -f "$ENDURANCE_REPORT" ]]; then
  OVERALL_STATUS=passed
  if [[ "$TEST_STATUS" != passed || "$ENDURANCE_STATUS" != passed || "$(jq -r '.status' "$ENDURANCE_REPORT")" != passed ]]; then
    OVERALL_STATUS=failed
  fi
  jq \
    --arg status "$OVERALL_STATUS" \
    --arg startedAt "$STARTED_AT" \
    --arg finishedAt "$FINISHED_AT" \
    --arg gitCommit "$GIT_COMMIT" \
    --argjson gitDirty "$GIT_DIRTY" \
    --arg testStatus "$TEST_STATUS" \
    --argjson testCount "$TEST_COUNT" \
    --arg testLog "$(basename "$TEST_LOG")" \
    --arg enduranceLog "$(basename "$ENDURANCE_LOG")" \
    --arg containerLog "$(basename "$CONTAINER_LOG")" \
    --argjson outageInjected "$INJECT_OUTAGE" \
    --argjson mqttConfigured "$(if [[ -n "$MQTT_BROKER" ]]; then echo true; else echo false; fi)" \
    --argjson wrapperDurationSeconds "$DURATION_SECONDS_OBSERVED" \
    '.status = $status
      | .lab = {
          startedAt:$startedAt,
          finishedAt:$finishedAt,
          wrapperDurationSeconds:$wrapperDurationSeconds,
          independentContainer:true,
          outageInjected:($outageInjected == 1),
          mqttBrokerConfigured:$mqttConfigured,
          automatedTests:{status:$testStatus,count:$testCount,log:$testLog},
          logs:{endurance:$enduranceLog,container:$containerLog}
        }
      | .source = {gitCommit:$gitCommit,gitDirty:$gitDirty}' \
    "$ENDURANCE_REPORT" >"$REPORT_PATH"
else
  jq -n \
    --arg startedAt "$STARTED_AT" \
    --arg finishedAt "$FINISHED_AT" \
    --arg testStatus "$TEST_STATUS" \
    --arg enduranceStatus "$ENDURANCE_STATUS" \
    --arg log "$(basename "$ENDURANCE_LOG")" \
    '{
      status:"failed",
      mode:"modbus_tcp_endurance",
      physicalDeviceExercised:false,
      startedAt:$startedAt,
      finishedAt:$finishedAt,
      failure:{testStatus:$testStatus,enduranceStatus:$enduranceStatus,log:$log}
    }' >"$REPORT_PATH"
fi

if [[ "$(jq -r '.status' "$REPORT_PATH")" != passed ]]; then
  tail -120 "$TEST_LOG" >&2 || true
  tail -120 "$ENDURANCE_LOG" >&2 || true
  echo "Modbus TCP lab acceptance failed; evidence retained at: $WORK_DIR" >&2
  exit 1
fi

jq '.' "$REPORT_PATH"
echo "Modbus TCP lab acceptance evidence: $WORK_DIR"
