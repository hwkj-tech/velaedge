#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -n "${EDGEOPS_FIELD_CAMPAIGN_RUNNER_TEST_WORK_DIR:-}" ]]; then
  WORK_DIR="$EDGEOPS_FIELD_CAMPAIGN_RUNNER_TEST_WORK_DIR"
  CLEANUP=0
else
  WORK_DIR="${ROOT_DIR}/target/field-campaign-runner-test-$$"
  CLEANUP=1
fi
if [[ "$WORK_DIR" != /* ]]; then
  WORK_DIR="${ROOT_DIR}/${WORK_DIR}"
fi
REPORT_PATH="${WORK_DIR}/report.json"
ARGS_PATH="${WORK_DIR}/args.txt"
mkdir -p "$WORK_DIR"
if [[ "$CLEANUP" -eq 1 ]]; then
  trap 'rm -rf "$WORK_DIR"' EXIT
fi

cat >"${WORK_DIR}/fake-field-campaign" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$@" >"${EDGEOPS_FIELD_CAMPAIGN_TEST_ARGS}"
EOF
chmod +x "${WORK_DIR}/fake-field-campaign"
printf '{}\n' >"${WORK_DIR}/configuration-package.json"

run_runner() {
  local -a runner_args=()
  local -a command=()
  if [[ "${1:-}" == "--preflight-only" ]]; then
    runner_args+=("$1")
    shift
  fi
  command=(
    env
    EDGEOPS_FIELD_CAMPAIGN_BINARY="${WORK_DIR}/fake-field-campaign"
    EDGEOPS_FIELD_CAMPAIGN_TEST_ARGS="$ARGS_PATH"
    EDGEOPS_FIELD_CAMPAIGN_CONFIG="${WORK_DIR}/configuration-package.json"
    EDGEOPS_FIELD_CAMPAIGN_OUTPUT_DIR="${WORK_DIR}/evidence"
    EDGEOPS_FIELD_CAMPAIGN_NATIVE_BROKER_AUDIT="${WORK_DIR}/inbox/audit.json"
    EDGEOPS_FIELD_CAMPAIGN_SITE_ID="WO-42"
    EDGEOPS_FIELD_CAMPAIGN_OPERATOR="operator-a"
    EDGEOPS_FIELD_CAMPAIGN_DEVICE_CONNECTION_ID="modbus-main"
    EDGEOPS_FIELD_CAMPAIGN_DEVICE_MANUFACTURER="Vendor A"
    EDGEOPS_FIELD_CAMPAIGN_DEVICE_MODEL="PLC-100"
    EDGEOPS_FIELD_CAMPAIGN_DEVICE_SERIAL="ASSET-001"
    EDGEOPS_FIELD_CAMPAIGN_PHYSICAL_DEVICE_CONFIRMED=1
    EDGEOPS_FIELD_CAMPAIGN_CHANGING_POINTS="pump-1/pressure,pump-1/running"
  )
  command+=("$@")
  command+=("${ROOT_DIR}/scripts/run-field-campaign.sh")
  if [[ "${#runner_args[@]}" -gt 0 ]]; then
    command+=("${runner_args[0]}")
  fi
  "${command[@]}"
}

run_runner

expect_pair() {
  local option="$1"
  local expected="$2"
  awk -v option="$option" -v expected="$expected" \
    '$0 == option { getline; if ($0 == expected) found=1 } END { exit(found ? 0 : 1) }' \
    "$ARGS_PATH" || {
      echo "missing expected argument pair: $option $expected" >&2
      exit 1
    }
}

expect_pair --config "${WORK_DIR}/configuration-package.json"
expect_pair --output-dir "${WORK_DIR}/evidence"
expect_pair --duration-seconds 86400
expect_pair --maximum-failure-ratio 0.01
expect_pair --maximum-progress-gap-seconds 300
expect_pair --site-id WO-42
expect_pair --device-connection-id modbus-main
expect_pair --device-manufacturer "Vendor A"
expect_pair --device-model PLC-100
expect_pair --device-serial ASSET-001
grep -Fx -- '--physical-device-exercised' "$ARGS_PATH" >/dev/null
[[ "$(grep -Fxc -- '--require-changing-point' "$ARGS_PATH")" -eq 2 ]]

rm -f "$ARGS_PATH"
run_runner env \
  EDGEOPS_FIELD_CAMPAIGN_MINIMUM_CYCLES=85000 \
  EDGEOPS_FIELD_CAMPAIGN_ROCKSDB_PATH="${WORK_DIR}/runtime.rocksdb" \
  EDGEOPS_FIELD_CAMPAIGN_REQUIRE_RECOVERY=1
expect_pair --minimum-cycles 85000
expect_pair --rocksdb-path "${WORK_DIR}/runtime.rocksdb"
grep -Fx -- '--require-recovery' "$ARGS_PATH" >/dev/null

rm -f "$ARGS_PATH"
run_runner --preflight-only
grep -Fx -- '--preflight-only' "$ARGS_PATH" >/dev/null

rm -f "$ARGS_PATH"
if run_runner env EDGEOPS_FIELD_CAMPAIGN_PHYSICAL_DEVICE_CONFIRMED=0 >/dev/null 2>&1; then
  echo "runner accepted an unconfirmed physical-device campaign" >&2
  exit 1
fi
[[ ! -e "$ARGS_PATH" ]] || {
  echo "field-campaign binary ran despite missing physical confirmation" >&2
  exit 1
}

if run_runner env EDGEOPS_FIELD_CAMPAIGN_MAXIMUM_PROGRESS_GAP_SECONDS=0 >/dev/null 2>&1; then
  echo "runner accepted a zero collection/MQTT progress-gap limit" >&2
  exit 1
fi

grep -F 'KillSignal=SIGTERM' \
  "${ROOT_DIR}/deploy/systemd/edgeops-field-campaign@.service" >/dev/null
grep -F 'Restart=no' \
  "${ROOT_DIR}/deploy/systemd/edgeops-field-campaign@.service" >/dev/null
grep -F 'ExecStartPre=/opt/edgeops/bin/run-field-campaign --preflight-only' \
  "${ROOT_DIR}/deploy/systemd/edgeops-field-campaign@.service" >/dev/null

jq -n \
  --arg status passed \
  --arg runner scripts/run-field-campaign.sh \
  --arg service deploy/systemd/edgeops-field-campaign@.service \
  '{schemaVersion:1,status:$status,runner:$runner,service:$service,checks:{parameterMapping:true,optionalPolicyMapping:true,physicalConfirmationRequired:true,numericLimitsValidated:true,preflightMapping:true,signalPolicy:true}}' \
  >"$REPORT_PATH"
jq '.' "$REPORT_PATH"
