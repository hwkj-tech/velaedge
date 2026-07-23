#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${EDGEOPS_SERIAL_LAB_WORK_DIR:-${ROOT_DIR}/target/serial-lab-acceptance-$$}"
REPORT_PATH="${EDGEOPS_SERIAL_LAB_REPORT:-${WORK_DIR}/report.json}"
LOG_PATH="${WORK_DIR}/serial-pty.log"
STARTED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
STARTED_SECONDS="$(date +%s)"

for command in cargo git jq shasum; do
  command -v "$command" >/dev/null || {
    echo "serial lab acceptance: missing required command: $command" >&2
    exit 2
  }
done

mkdir -p "$WORK_DIR" "$(dirname "$REPORT_PATH")"
SOURCE_SHA256="$(shasum -a 256 "${ROOT_DIR}/crates/edge-runtime/tests/serial_pty.rs" | awk '{print $1}')"
GIT_COMMIT="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || printf unknown)"

STATUS=passed
if ! cargo test --manifest-path "${ROOT_DIR}/Cargo.toml" \
  -p edge-runtime --test serial_pty -- --nocapture >"$LOG_PATH" 2>&1; then
  STATUS=failed
fi

FINISHED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
DURATION_SECONDS=$(( $(date +%s) - STARTED_SECONDS ))
jq -n \
  --arg status "$STATUS" \
  --arg startedAt "$STARTED_AT" \
  --arg finishedAt "$FINISHED_AT" \
  --arg gitCommit "$GIT_COMMIT" \
  --arg sourceSha256 "$SOURCE_SHA256" \
  --arg log "$(basename "$LOG_PATH")" \
  --argjson durationSeconds "$DURATION_SECONDS" \
  '{
    status:$status,
    mode:"automated-pty-lab",
    physicalDeviceExercised:false,
    startedAt:$startedAt,
    finishedAt:$finishedAt,
    durationSeconds:$durationSeconds,
    source:{gitCommit:$gitCommit,testSourceSha256:$sourceSha256},
    productionPath:{
      serialFactory:"TokioSerialBusFactory",
      runtime:"ConfiguredEdgeRuntime",
      mqttPublisher:"RumqttcMqttPublisher",
      mqttQos:1,
      brokerAcknowledgement:"PUBACK"
    },
    protocols:["Modbus RTU","DL/T 645-2007","IEC 60870-5-101"],
    assertions:[
      "request frame reaches an operating-system PTY character device",
      "device response passes production checksum and protocol decoding",
      "Runtime executes an enabled data configuration",
      "JSON payload contains decoded value and quality",
      "publish returns only after MQTT QoS 1 PUBACK"
    ],
    testCount:4,
    log:$log,
    limitation:"PTY evidence does not verify physical RS-485 wiring, adapter direction control, non-zero baud configuration, or a site device."
  }' >"$REPORT_PATH"

if [[ "$STATUS" != "passed" ]]; then
  tail -120 "$LOG_PATH" >&2
  echo "serial lab acceptance failed; evidence retained at: $WORK_DIR" >&2
  exit 1
fi

jq '.' "$REPORT_PATH"
echo "serial lab acceptance evidence: $WORK_DIR"
