#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MQTT_HOST="${VELAEDGE_COMMAND_MQTT_HOST:-127.0.0.1}"
MQTT_PORT="${VELAEDGE_COMMAND_MQTT_PORT:-1883}"
MQTT_BROKER_LABEL="${VELAEDGE_COMMAND_MQTT_BROKER_LABEL:-external-mqtt-broker}"
MODBUS_ENDPOINT="${VELAEDGE_COMMAND_MODBUS_ENDPOINT:-127.0.0.1:1502}"
MODBUS_CONTAINER="${VELAEDGE_COMMAND_MODBUS_CONTAINER:-}"
WORK_DIR="${VELAEDGE_COMMAND_ACCEPTANCE_WORK_DIR:-${ROOT_DIR}/target/mqtt-modbus-command-acceptance-$$}"
REPORT_PATH="${VELAEDGE_COMMAND_ACCEPTANCE_REPORT:-${WORK_DIR}/report.json}"
TEST_LOG="${WORK_DIR}/mqtt-modbus-command.log"

for command in cargo git jq nc shasum; do
  command -v "$command" >/dev/null || {
    echo "missing required command: $command" >&2
    exit 2
  }
done

[[ "$MQTT_PORT" =~ ^[0-9]+$ ]] || {
  echo "VELAEDGE_COMMAND_MQTT_PORT must be a TCP port number" >&2
  exit 2
}

MODBUS_HOST="${MODBUS_ENDPOINT%:*}"
MODBUS_PORT="${MODBUS_ENDPOINT##*:}"
if [[ -z "$MODBUS_HOST" || "$MODBUS_HOST" == "$MODBUS_ENDPOINT" || ! "$MODBUS_PORT" =~ ^[0-9]+$ ]]; then
  echo "VELAEDGE_COMMAND_MODBUS_ENDPOINT must use host:port format" >&2
  exit 2
fi

for endpoint in "MQTT:${MQTT_HOST}:${MQTT_PORT}" "Modbus:${MODBUS_HOST}:${MODBUS_PORT}"; do
  label="${endpoint%%:*}"
  address="${endpoint#*:}"
  host="${address%:*}"
  port="${address##*:}"
  nc -z -w 2 "$host" "$port" >/dev/null 2>&1 || {
    echo "$label endpoint is not reachable: $host:$port" >&2
    exit 2
  }
done

mkdir -p "$WORK_DIR" "$(dirname "$REPORT_PATH")"
STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
STARTED_EPOCH="$(date +%s)"
GIT_COMMIT="$(git -C "$ROOT_DIR" rev-parse HEAD)"
if [[ -n "$(git -C "$ROOT_DIR" status --porcelain)" ]]; then
  GIT_DIRTY=true
else
  GIT_DIRTY=false
fi
TEST_SOURCE="${ROOT_DIR}/crates/edge-runtime/tests/mqtt_modbus_command_acceptance.rs"
TEST_SOURCE_SHA256="$(shasum -a 256 "$TEST_SOURCE" | awk '{print $1}')"

CONTAINER_IMAGE=""
CONTAINER_IMAGE_ID=""
if [[ -n "$MODBUS_CONTAINER" ]]; then
  command -v docker >/dev/null || {
    echo "docker is required when VELAEDGE_COMMAND_MODBUS_CONTAINER is set" >&2
    exit 2
  }
  CONTAINER_IMAGE="$(docker inspect --format '{{.Config.Image}}' "$MODBUS_CONTAINER")"
  CONTAINER_IMAGE_ID="$(docker image inspect --format '{{.Id}}' "$CONTAINER_IMAGE")"
fi

set +e
(
  cd "$ROOT_DIR"
  VELAEDGE_COMMAND_MQTT_HOST="$MQTT_HOST" \
  VELAEDGE_COMMAND_MQTT_PORT="$MQTT_PORT" \
  VELAEDGE_COMMAND_MODBUS_ENDPOINT="$MODBUS_ENDPOINT" \
    cargo test -p edge-runtime \
      --test mqtt_modbus_command_acceptance \
      mqtt_command_writes_docker_modbus_and_publishes_reply \
      -- --ignored --exact --nocapture
) >"$TEST_LOG" 2>&1
TEST_EXIT_CODE=$?
set -e

if [[ "$TEST_EXIT_CODE" -eq 0 ]]; then
  STATUS="passed"
  TESTS_PASSED=1
else
  STATUS="failed"
  TESTS_PASSED=0
fi

FINISHED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
FINISHED_EPOCH="$(date +%s)"
DURATION_SECONDS=$((FINISHED_EPOCH - STARTED_EPOCH))

jq -n \
  --arg status "$STATUS" \
  --arg startedAt "$STARTED_AT" \
  --arg finishedAt "$FINISHED_AT" \
  --arg gitCommit "$GIT_COMMIT" \
  --argjson gitDirty "$GIT_DIRTY" \
  --arg testSourceSha256 "$TEST_SOURCE_SHA256" \
  --arg mqttBrokerLabel "$MQTT_BROKER_LABEL" \
  --arg mqttHost "$MQTT_HOST" \
  --argjson mqttPort "$MQTT_PORT" \
  --arg modbusEndpoint "$MODBUS_ENDPOINT" \
  --arg container "$MODBUS_CONTAINER" \
  --arg containerImage "$CONTAINER_IMAGE" \
  --arg containerImageId "$CONTAINER_IMAGE_ID" \
  --arg testLog "$(basename "$TEST_LOG")" \
  --argjson durationSeconds "$DURATION_SECONDS" \
  --argjson testsPassed "$TESTS_PASSED" \
  --argjson testExitCode "$TEST_EXIT_CODE" \
  '{
    schemaVersion: 1,
    status: $status,
    mode: "mqtt-to-modbus-command-integration",
    physicalDeviceExercised: false,
    startedAt: $startedAt,
    finishedAt: $finishedAt,
    durationSeconds: $durationSeconds,
    source: {
      gitCommit: $gitCommit,
      gitDirty: $gitDirty,
      testSourceSha256: $testSourceSha256
    },
    mqtt: {
      brokerLabel: $mqttBrokerLabel,
      host: $mqttHost,
      port: $mqttPort,
      protocol: "3.1.1",
      commandQos: 1,
      replyQos: 1
    },
    modbus: {
      endpoint: $modbusEndpoint,
      function: "FC06 holding-register write with readback",
      container: (if $container == "" then null else $container end),
      image: (if $containerImage == "" then null else $containerImage end),
      imageId: (if $containerImageId == "" then null else $containerImageId end)
    },
    assertions: [
      "MQTT command subscription acknowledged",
      "nested JSON value resolved by command graph",
      "Runtime wrote the configured Modbus holding register",
      "Modbus readback matched the commanded value",
      "MQTT success reply carried command ID and verification result"
    ],
    tests: {
      passed: $testsPassed,
      failed: (if $testsPassed == 1 then 0 else 1 end),
      exitCode: $testExitCode,
      log: $testLog
    },
    limitation: "This integration uses a non-physical Modbus endpoint and does not replace field authorization, vendor interoperability, or 24-hour physical-device acceptance."
  }' >"$REPORT_PATH"

cat "$TEST_LOG"
cat "$REPORT_PATH"
echo "MQTT-to-Modbus command acceptance evidence: $WORK_DIR"

exit "$TEST_EXIT_CODE"
