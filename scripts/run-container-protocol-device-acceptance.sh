#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${EDGEOPS_CONTAINER_PROTOCOL_WORK_DIR:-${ROOT_DIR}/target/container-protocol-device-acceptance-$(date +%s)}"
REPORT_PATH="${EDGEOPS_CONTAINER_PROTOCOL_REPORT:-${WORK_DIR}/report.json}"
COMPOSE_FILE="${ROOT_DIR}/deploy/industrial-device-lab/compose.yaml"
COMPOSE_PROJECT="velaedge-container-protocol-$$"
S7_HOST_PORT="${EDGEOPS_CONTAINER_S7_PORT:-21102}"
FINS_HOST_PORT="${EDGEOPS_CONTAINER_FINS_PORT:-29600}"
IEC104_HOST_PORT="${EDGEOPS_CONTAINER_IEC104_PORT:-22404}"
BACNET_HOST_PORT="${EDGEOPS_CONTAINER_BACNET_PORT:-24780}"
NO_BUILD="${EDGEOPS_CONTAINER_PROTOCOL_NO_BUILD:-0}"
SIMULATOR_IMAGE="velaedge/protocol-device-sim:0.1.0"
CONTAINER_LOG="${WORK_DIR}/containers.log"
TEST_LOG="${WORK_DIR}/production-adapters.log"
PS_LOG="${WORK_DIR}/compose-ps.json"
STARTED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
STARTED_SECONDS="$(date +%s)"
CONTAINERS_STARTED=0

for command in cargo docker git jq rg shasum; do
  command -v "$command" >/dev/null || {
    echo "container protocol acceptance: missing required command: $command" >&2
    exit 2
  }
done

mkdir -p "$WORK_DIR" "$(dirname "$REPORT_PATH")"

compose() {
  S7_HOST_PORT="$S7_HOST_PORT" FINS_HOST_PORT="$FINS_HOST_PORT" \
    IEC104_HOST_PORT="$IEC104_HOST_PORT" BACNET_HOST_PORT="$BACNET_HOST_PORT" \
    docker compose --project-name "$COMPOSE_PROJECT" --file "$COMPOSE_FILE" "$@"
}

cleanup() {
  if [[ "$CONTAINERS_STARTED" == 1 ]]; then
    compose logs --no-color >"$CONTAINER_LOG" 2>&1 || true
    compose down --volumes --remove-orphans >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT INT TERM

STATUS=passed
IMAGE_ID=""
if [[ "$NO_BUILD" == 1 ]]; then
  if ! IMAGE_ID="$(docker image inspect "$SIMULATOR_IMAGE" --format '{{.Id}}' 2>/dev/null)"; then
    echo "container protocol acceptance: cached image is missing: $SIMULATOR_IMAGE" >&2
    exit 2
  fi
  COMPOSE_UP_ARGS=(up --detach --no-build --wait)
else
  COMPOSE_UP_ARGS=(up --detach --build --wait)
fi
if compose "${COMPOSE_UP_ARGS[@]}" >"$CONTAINER_LOG" 2>&1; then
  CONTAINERS_STARTED=1
  IMAGE_ID="$(docker image inspect "$SIMULATOR_IMAGE" --format '{{.Id}}')"
  compose ps --format json >"$PS_LOG"
else
  STATUS=failed
fi

if [[ "$STATUS" == passed ]]; then
  if ! VELAEDGE_S7_SIM_ENDPOINT="127.0.0.1:${S7_HOST_PORT}" \
    VELAEDGE_FINS_SIM_ENDPOINT="127.0.0.1:${FINS_HOST_PORT}" \
    VELAEDGE_IEC104_SIM_ENDPOINT="127.0.0.1:${IEC104_HOST_PORT}" \
    VELAEDGE_BACNET_SIM_ENDPOINT="127.0.0.1:${BACNET_HOST_PORT}" \
    cargo test --manifest-path "${ROOT_DIR}/Cargo.toml" \
      -p edge-runtime --test container_protocol_devices -- --ignored --nocapture \
      >"$TEST_LOG" 2>&1; then
    STATUS=failed
  fi
else
  : >"$TEST_LOG"
  : >"$PS_LOG"
fi

FINISHED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
DURATION_SECONDS=$(( $(date +%s) - STARTED_SECONDS ))
TEST_COUNT="$({ rg -o '[0-9]+ passed' "$TEST_LOG" || true; } | awk '{total += $1} END {print total + 0}')"
GIT_COMMIT="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || printf unknown)"
GIT_DIRTY=false
if [[ -n "$(git -C "$ROOT_DIR" status --porcelain 2>/dev/null)" ]]; then
  GIT_DIRTY=true
fi
SOURCE_SHA256="$(
  shasum -a 256 \
    "${ROOT_DIR}/deploy/industrial-device-lab/simulator/src/main.rs" \
    "${ROOT_DIR}/deploy/industrial-device-lab/simulator/src/bacnet.rs" \
    "${ROOT_DIR}/deploy/industrial-device-lab/compose.yaml" \
    "${ROOT_DIR}/crates/edge-runtime/tests/container_protocol_devices.rs" \
    "${ROOT_DIR}/scripts/run-container-protocol-device-acceptance.sh" \
    | shasum -a 256 | awk '{print $1}'
)"

jq -n \
  --arg status "$STATUS" \
  --arg startedAt "$STARTED_AT" \
  --arg finishedAt "$FINISHED_AT" \
  --argjson durationSeconds "$DURATION_SECONDS" \
  --arg gitCommit "$GIT_COMMIT" \
  --argjson gitDirty "$GIT_DIRTY" \
  --arg sourceSha256 "$SOURCE_SHA256" \
  --arg image "$SIMULATOR_IMAGE" \
  --arg imageId "$IMAGE_ID" \
  --arg s7Endpoint "127.0.0.1:${S7_HOST_PORT}" \
  --arg finsTcpEndpoint "127.0.0.1:${FINS_HOST_PORT}" \
  --arg finsUdpEndpoint "127.0.0.1:${FINS_HOST_PORT}" \
  --arg iec104Endpoint "127.0.0.1:${IEC104_HOST_PORT}" \
  --arg bacnetEndpoint "127.0.0.1:${BACNET_HOST_PORT}" \
  --argjson imageBuildSkipped "$([[ "$NO_BUILD" == 1 ]] && printf true || printf false)" \
  --argjson testCount "$TEST_COUNT" \
  '{
    status:$status,
    mode:"containerized-industrial-device-lab",
    physicalDeviceExercised:false,
    startedAt:$startedAt,
    finishedAt:$finishedAt,
    durationSeconds:$durationSeconds,
    source:{
      gitCommit:$gitCommit,
      gitDirty:$gitDirty,
      sha256:$sourceSha256,
      image:$image,
      imageId:$imageId,
      imageBuildSkipped:$imageBuildSkipped
    },
    devices:[
      {
        protocol:"siemens-s7",
        transport:"ISO-on-TCP/S7",
        endpoint:$s7Endpoint,
        capabilities:["dynamic-read","persistent-session","bit-write","command-feedback"]
      },
      {
        protocol:"omron-fins",
        transport:"FINS/TCP and FINS/UDP",
        tcpEndpoint:$finsTcpEndpoint,
        udpEndpoint:$finsUdpEndpoint,
        capabilities:["node-handshake","dynamic-read","persistent-session","bit-write","command-feedback"]
      },
      {
        protocol:"iec-60870-5-104",
        transport:"IEC 60870-5-104/TCP",
        endpoint:$iec104Endpoint,
        capabilities:["startdt","general-interrogation","spontaneous-telemetry","persistent-session","single-command","double-command","float-setpoint","select-before-operate","activation-confirmation"]
      },
      {
        protocol:"bacnet-ip",
        transport:"BACnet/IP over UDP",
        endpoint:$bacnetEndpoint,
        capabilities:["directed-who-is","i-am","read-property-multiple","dynamic-read","persistent-session","subscribe-cov","unconfirmed-cov-notification","write-property","command-priority","command-feedback"]
      }
    ],
    productionAdapterTests:{status:$status,count:$testCount,log:"production-adapters.log"},
    evidence:{containerLog:"containers.log",composeState:"compose-ps.json"},
    limitation:"Container devices exercise real protocol sockets and production adapters, but do not prove vendor firmware interoperability, field wiring, electrical resilience, or 24-hour physical PLC stability."
  }' >"$REPORT_PATH"

if [[ "$STATUS" != passed ]]; then
  tail -120 "$CONTAINER_LOG" >&2 || true
  tail -120 "$TEST_LOG" >&2 || true
  echo "container protocol acceptance failed; evidence retained at: $WORK_DIR" >&2
  exit 1
fi

jq '.' "$REPORT_PATH"
echo "container protocol acceptance evidence: $WORK_DIR"
