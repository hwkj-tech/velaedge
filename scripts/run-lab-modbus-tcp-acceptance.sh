#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${EDGEOPS_MODBUS_TCP_LAB_WORK_DIR:-${ROOT_DIR}/target/modbus-tcp-lab-acceptance-$$}"
REPORT_PATH="${EDGEOPS_MODBUS_TCP_LAB_REPORT:-${WORK_DIR}/report.json}"
LOG_PATH="${WORK_DIR}/modbus-tcp.log"
STARTED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
STARTED_SECONDS="$(date +%s)"

for command in cargo git jq shasum; do
  command -v "$command" >/dev/null || {
    echo "Modbus TCP lab acceptance: missing required command: $command" >&2
    exit 2
  }
done

mkdir -p "$WORK_DIR" "$(dirname "$REPORT_PATH")"
TEST_SHA256="$(shasum -a 256 "${ROOT_DIR}/crates/edge-runtime/tests/modbus_tcp.rs" | awk '{print $1}')"
ADAPTER_SHA256="$(shasum -a 256 "${ROOT_DIR}/crates/edge-runtime/src/modbus_tcp.rs" | awk '{print $1}')"
GIT_COMMIT="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || printf unknown)"

STATUS=passed
if ! cargo test --manifest-path "${ROOT_DIR}/Cargo.toml" \
  -p edge-runtime --test modbus_tcp -- --nocapture >"$LOG_PATH" 2>&1; then
  STATUS=failed
fi

FINISHED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
DURATION_SECONDS=$(( $(date +%s) - STARTED_SECONDS ))
jq -n \
  --arg status "$STATUS" \
  --arg startedAt "$STARTED_AT" \
  --arg finishedAt "$FINISHED_AT" \
  --arg gitCommit "$GIT_COMMIT" \
  --arg testSha256 "$TEST_SHA256" \
  --arg adapterSha256 "$ADAPTER_SHA256" \
  --arg log "$(basename "$LOG_PATH")" \
  --argjson durationSeconds "$DURATION_SECONDS" \
  '{
    status:$status,
    mode:"automated-modbus-tcp-lab",
    physicalDeviceExercised:false,
    startedAt:$startedAt,
    finishedAt:$finishedAt,
    durationSeconds:$durationSeconds,
    source:{gitCommit:$gitCommit,testSourceSha256:$testSha256,adapterSourceSha256:$adapterSha256},
    productionPath:{
      deviceTransport:"real TCP socket",
      protocolAdapter:"ModbusTcpAdapter",
      runtime:"ConfiguredEdgeRuntime",
      localStore:"RocksEdgeRuntimeStore",
      mqttPublisher:"RumqttcMqttPublisher",
      mqttQos:1,
      brokerAcknowledgement:"PUBACK"
    },
    protocolFunctions:["01 Read Coils","03 Read Holding Registers","04 Read Input Registers"],
    assertions:[
      "Runtime opens a real TCP socket and validates MBAP transaction metadata",
      "register, coil, exception, and changing-value responses are decoded",
      "enabled data configuration builds a typed JSON payload",
      "RocksDB outbox is drained only after MQTT QoS 1 PUBACK",
      "acknowledgement receipt retains route metadata without payload bytes"
    ],
    testCount:5,
    log:$log,
    limitation:"The Modbus server and MQTT peer are local protocol simulators; this evidence does not replace a physical device or production broker acceptance."
  }' >"$REPORT_PATH"

if [[ "$STATUS" != "passed" ]]; then
  tail -120 "$LOG_PATH" >&2
  echo "Modbus TCP lab acceptance failed; evidence retained at: $WORK_DIR" >&2
  exit 1
fi

jq '.' "$REPORT_PATH"
echo "Modbus TCP lab acceptance evidence: $WORK_DIR"
