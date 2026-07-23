#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${EDGEOPS_FIELD_PREFLIGHT_WORK_DIR:-${ROOT_DIR}/target/field-preflight-acceptance-$$}"
CONFIG_PATH="${EDGEOPS_FIELD_PREFLIGHT_CONFIG:-${ROOT_DIR}/scripts/fixtures/field-preflight-config.json}"
FIXTURE_DIR="${ROOT_DIR}/crates/cloud-api/tests/fixtures/edgelink"
REPORT_PATH="${WORK_DIR}/report.json"

mkdir -p "$WORK_DIR"

EDGEOPS_FIELD_CONFIG="$CONFIG_PATH" \
EDGEOPS_FIELD_SERIAL_PORT=/dev/null \
EDGEOPS_FIELD_SERVER_CERT="${FIXTURE_DIR}/server.pem" \
EDGEOPS_FIELD_SERVER_KEY="${FIXTURE_DIR}/server-key.pem" \
EDGEOPS_FIELD_RUNTIME_CA="${FIXTURE_DIR}/ca.pem" \
EDGEOPS_FIELD_RUNTIME_CERT="${FIXTURE_DIR}/client.pem" \
EDGEOPS_FIELD_RUNTIME_KEY="${FIXTURE_DIR}/client-key.pem" \
EDGEOPS_FIELD_SERVER_CA="${FIXTURE_DIR}/ca.pem" \
EDGEOPS_FIELD_SERVER_NAME=localhost \
EDGEOPS_FIELD_PREFLIGHT_ONLY=1 \
EDGEOPS_FIELD_ALLOW_TEST_SERIAL=1 \
EDGEOPS_FIELD_ALLOW_INSECURE_MQTT=1 \
EDGEOPS_FIELD_WORK_DIR="$WORK_DIR" \
EDGEOPS_FIELD_REPORT="$REPORT_PATH" \
"${ROOT_DIR}/scripts/run-field-hardware-acceptance.sh" >/dev/null

jq -e '
  .status == "passed"
  and .mode == "preflight"
  and .physicalDeviceExercised == false
  and (.serial.connections | length) == 1
  and (.mqtt | length) == 1
  and (.dataConfigs | length) == 1
  and .dataConfigs[0].point_count == 1
  and .dataConfigs[0].qos == 1
' "$REPORT_PATH" >/dev/null

"${ROOT_DIR}/scripts/verify-field-acceptance-report.sh" "$REPORT_PATH" >/dev/null
if "${ROOT_DIR}/scripts/verify-field-acceptance-report.sh" --require-physical "$REPORT_PATH" >/dev/null 2>&1; then
  echo "preflight report was incorrectly accepted as physical field evidence" >&2
  exit 1
fi

jq '.' "$REPORT_PATH"
echo "controlled field preflight evidence: $WORK_DIR"
