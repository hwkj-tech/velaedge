#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="${EDGEOPS_RELEASE_PROFILE:-local}"
WORK_DIR="${EDGEOPS_RELEASE_WORK_DIR:-${ROOT_DIR}/target/release-gates-$$}"
if [[ "$WORK_DIR" != /* ]]; then
  WORK_DIR="${ROOT_DIR}/${WORK_DIR}"
fi
REPORT_PATH="${EDGEOPS_RELEASE_REPORT:-${WORK_DIR}/report.json}"
VELAMQ_REPO="${VELAMQ_REPO:-}"
FIELD_CAMPAIGN_DIRS="${EDGEOPS_FIELD_CAMPAIGN_DIRS:-}"
FIELD_CAMPAIGN_PLAN="${EDGEOPS_FIELD_CAMPAIGN_PLAN:-}"
FIELD_POLICY="${EDGEOPS_FIELD_POLICY:-}"
CONTAINER_PROTOCOL_GATE="${EDGEOPS_CONTAINER_PROTOCOL_GATE:-}"
STARTED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
RESULTS='[]'
OVERALL_STATUS="running"

case "$PROFILE" in
  local | site) ;;
  *) echo "EDGEOPS_RELEASE_PROFILE must be local or site" >&2; exit 2 ;;
esac

if [[ ( "$PROFILE" == site || -n "$FIELD_CAMPAIGN_PLAN" ) && -z "$FIELD_POLICY" ]]; then
  FIELD_POLICY="${ROOT_DIR}/deploy/field-acceptance-policy.json"
fi
if [[ -n "$FIELD_POLICY" && "$FIELD_POLICY" != /* ]]; then
  FIELD_POLICY="${ROOT_DIR}/${FIELD_POLICY}"
fi
if [[ -n "$FIELD_CAMPAIGN_PLAN" && "$FIELD_CAMPAIGN_PLAN" != /* ]]; then
  FIELD_CAMPAIGN_PLAN="${ROOT_DIR}/${FIELD_CAMPAIGN_PLAN}"
fi

if [[ -z "$CONTAINER_PROTOCOL_GATE" ]]; then
  if [[ "$PROFILE" == site ]]; then
    CONTAINER_PROTOCOL_GATE=required
  else
    CONTAINER_PROTOCOL_GATE=auto
  fi
fi
case "$CONTAINER_PROTOCOL_GATE" in
  auto | required | skip) ;;
  *) echo "EDGEOPS_CONTAINER_PROTOCOL_GATE must be auto, required or skip" >&2; exit 2 ;;
esac

for command in cargo curl git jq nc npm openssl rg sqlite3; do
  command -v "$command" >/dev/null || {
    echo "missing required release-gate command: $command" >&2
    exit 2
  }
done

mkdir -p "$WORK_DIR" "$(dirname "$REPORT_PATH")"

GIT_COMMIT="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || printf unknown)"
if [[ -n "$(git -C "$ROOT_DIR" status --porcelain 2>/dev/null)" ]]; then
  GIT_DIRTY=true
else
  GIT_DIRTY=false
fi

write_report() {
  local finished_at="${1:-}"
  jq -n \
    --arg status "$OVERALL_STATUS" \
    --arg profile "$PROFILE" \
    --arg startedAt "$STARTED_AT" \
    --arg finishedAt "$finished_at" \
    --arg gitCommit "$GIT_COMMIT" \
    --argjson gitDirty "$GIT_DIRTY" \
    --arg workspace "$ROOT_DIR" \
    --argjson gates "$RESULTS" \
    '{
      status:$status, profile:$profile, startedAt:$startedAt,
      finishedAt:(if $finishedAt == "" then null else $finishedAt end),
      source:{workspace:$workspace,gitCommit:$gitCommit,gitDirty:$gitDirty},
      gates:$gates
    }' >"$REPORT_PATH"
}

append_result() {
  local name="$1"
  local status="$2"
  local duration="$3"
  local required="$4"
  local log="$5"
  local evidence="$6"
  RESULTS="$(printf '%s' "$RESULTS" | jq \
    --arg name "$name" \
    --arg status "$status" \
    --argjson durationSeconds "$duration" \
    --argjson required "$required" \
    --arg log "$log" \
    --arg evidence "$evidence" \
    '. + [{
      name:$name,status:$status,durationSeconds:$durationSeconds,required:$required,
      log:(if $log == "" then null else $log end),
      evidence:(if $evidence == "" then null else $evidence end)
    }]')"
  write_report
}

run_gate() {
  local name="$1"
  local evidence="$2"
  shift 2
  local log="${WORK_DIR}/${name}.log"
  local started finished duration
  started="$(date +%s)"
  echo "[release-gate] running: $name"
  if "$@" >"$log" 2>&1; then
    finished="$(date +%s)"
    duration=$((finished - started))
    append_result "$name" passed "$duration" true "$(basename "$log")" "$evidence"
    echo "[release-gate] passed: $name (${duration}s)"
  else
    local command_status=$?
    finished="$(date +%s)"
    duration=$((finished - started))
    append_result "$name" failed "$duration" true "$(basename "$log")" "$evidence"
    OVERALL_STATUS=failed
    write_report "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    echo "[release-gate] failed: $name" >&2
    tail -120 "$log" >&2
    return "$command_status"
  fi
}

run_field_interoperability_gate() {
  local name="$1"
  local campaign_list="$2"
  local -a campaign_dirs command
  IFS=':' read -r -a campaign_dirs <<<"$campaign_list"
  command=(
    cargo run --quiet --release -p edge-runtime --bin field-interoperability-gate --
    --output "${WORK_DIR}/field-interoperability/report.json"
  )
  if [[ -n "$FIELD_POLICY" ]]; then
    [[ -f "$FIELD_POLICY" ]] || {
      echo "field interoperability policy does not exist: $FIELD_POLICY" >&2
      return 2
    }
    command+=(--policy "$FIELD_POLICY")
  fi
  local directory
  for directory in "${campaign_dirs[@]}"; do
    [[ -n "$directory" ]] || continue
    command+=(--campaign-dir "$directory")
  done
  run_gate "$name" "field-interoperability/report.json" "${command[@]}"
}

run_field_site_status_gate() {
  local name="$1"
  [[ -f "$FIELD_CAMPAIGN_PLAN" ]] || {
    echo "field campaign plan does not exist: $FIELD_CAMPAIGN_PLAN" >&2
    return 2
  }
  [[ -f "$FIELD_POLICY" ]] || {
    echo "field interoperability policy does not exist: $FIELD_POLICY" >&2
    return 2
  }
  run_gate "$name" "field-campaign-site/report.json" \
    cargo run --quiet --release -p edge-runtime --bin field-campaign-status -- \
      --plan "$FIELD_CAMPAIGN_PLAN" \
      --policy "$FIELD_POLICY" \
      --output "${WORK_DIR}/field-campaign-site/report.json" \
      --require-complete
}

skip_gate() {
  local name="$1"
  local required="$2"
  local reason="$3"
  RESULTS="$(printf '%s' "$RESULTS" | jq \
    --arg name "$name" --argjson required "$required" --arg reason "$reason" \
    '. + [{name:$name,status:"not_run",durationSeconds:0,required:$required,log:null,evidence:null,reason:$reason}]')"
  write_report
}

write_report

run_gate rust-format "cargo fmt --all --check" \
  cargo fmt --all --check
run_gate rust-clippy "cargo clippy --workspace --all-targets --all-features -- -D warnings" \
  cargo clippy --workspace --all-targets --all-features -- -D warnings
run_gate rust-workspace "cargo test --workspace" \
  cargo test --workspace
run_gate protocol-matrix "protocol-matrix/report.json" \
  env \
    EDGEOPS_PROTOCOL_MATRIX_WORK_DIR="${WORK_DIR}/protocol-matrix" \
    "${ROOT_DIR}/scripts/run-protocol-matrix-acceptance.sh"
if [[ "$CONTAINER_PROTOCOL_GATE" == skip ]]; then
  skip_gate container-protocol-devices false "disabled by EDGEOPS_CONTAINER_PROTOCOL_GATE=skip"
elif command -v docker >/dev/null && docker info >/dev/null 2>&1; then
  run_gate container-protocol-devices "container-protocol-devices/report.json" \
    env \
      EDGEOPS_CONTAINER_PROTOCOL_WORK_DIR="${WORK_DIR}/container-protocol-devices" \
      EDGEOPS_CONTAINER_PROTOCOL_NO_BUILD="${EDGEOPS_CONTAINER_PROTOCOL_NO_BUILD:-0}" \
      "${ROOT_DIR}/scripts/run-container-protocol-device-acceptance.sh"
elif [[ "$CONTAINER_PROTOCOL_GATE" == required ]]; then
  echo "Docker is required for the container protocol device gate" >&2
  OVERALL_STATUS=failed
  skip_gate container-protocol-devices true "Docker daemon is unavailable"
  write_report "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  exit 2
else
  skip_gate container-protocol-devices false "Docker daemon is unavailable"
fi
run_gate console-tests "complete component/API test suite" \
  npm --prefix web/console test -- --run
run_gate console-build "web/console/dist" \
  npm --prefix web/console run build
run_gate console-e2e "console-e2e/results.json" \
  env \
    EDGEOPS_E2E_HTTP_PORT=18261 \
    EDGEOPS_E2E_GATEWAY_PORT=19261 \
    EDGEOPS_E2E_WORK_DIR="${WORK_DIR}/console-e2e" \
    npm --prefix web/console run test:e2e
run_gate deployment-smoke "deployment-smoke/report.json" \
  env \
    EDGEOPS_DEPLOY_SMOKE_HTTP_PORT=18253 \
    EDGEOPS_DEPLOY_SMOKE_GATEWAY_PORT=19253 \
    EDGEOPS_DEPLOY_SMOKE_WORK_DIR="${WORK_DIR}/deployment-smoke" \
    "${ROOT_DIR}/scripts/run-deployment-smoke-acceptance.sh"
run_gate serial-protocol-lab "serial-lab/report.json" \
  env \
    EDGEOPS_SERIAL_LAB_WORK_DIR="${WORK_DIR}/serial-lab" \
    "${ROOT_DIR}/scripts/run-lab-serial-acceptance.sh"
run_gate modbus-tcp-lab "modbus-tcp-lab/report.json" \
  env \
    EDGEOPS_MODBUS_TCP_LAB_WORK_DIR="${WORK_DIR}/modbus-tcp-lab" \
    "${ROOT_DIR}/scripts/run-lab-modbus-tcp-acceptance.sh"
run_gate field-report-verifier "field-report-verifier/report.json" \
  env \
    EDGEOPS_FIELD_VERIFIER_TEST_WORK_DIR="${WORK_DIR}/field-report-verifier" \
    "${ROOT_DIR}/scripts/test-field-acceptance-report-verifier.sh"
run_gate field-campaign-runner "field-campaign-runner/report.json" \
  env \
    EDGEOPS_FIELD_CAMPAIGN_RUNNER_TEST_WORK_DIR="${WORK_DIR}/field-campaign-runner" \
    "${ROOT_DIR}/scripts/test-field-campaign-runner.sh"

run_gate edgelink-mtls "edgelink/report.json" \
  env \
    EDGELINK_ACCEPTANCE_HTTP_PORT=18211 \
    EDGELINK_ACCEPTANCE_GATEWAY_PORT=19211 \
    EDGELINK_ACCEPTANCE_WORK_DIR="${WORK_DIR}/edgelink" \
    "${ROOT_DIR}/scripts/run-edgelink-mtls-acceptance.sh"
run_gate certificate-lifecycle "certificates/report.json" \
  env \
    CERTIFICATE_ACCEPTANCE_WORK_DIR="${WORK_DIR}/certificates" \
    "${ROOT_DIR}/scripts/run-certificate-lifecycle-acceptance.sh"
run_gate cloud-recovery "recovery/report.json" \
  env \
    CLOUD_RECOVERY_HTTP_PORT=18215 \
    CLOUD_RECOVERY_GATEWAY_PORT=19215 \
    CLOUD_RECOVERY_WORK_DIR="${WORK_DIR}/recovery" \
    "${ROOT_DIR}/scripts/run-cloud-recovery-acceptance.sh"
run_gate performance "performance/report.json" \
  env \
    EDGEOPS_PERF_HTTP_PORT=18219 \
    EDGEOPS_PERF_GATEWAY_PORT=19219 \
    EDGEOPS_PERF_WORK_DIR="${WORK_DIR}/performance" \
    "${ROOT_DIR}/scripts/run-performance-gates.sh"

if [[ -n "$VELAMQ_REPO" ]]; then
  run_gate velamq-tls-qos1 "velamq/report.json" \
    env \
      VELAMQ_REPO="$VELAMQ_REPO" \
      VELAMQ_ACCEPTANCE_API_PORT=18231 \
      VELAMQ_ACCEPTANCE_MQTT_PORT=18940 \
      VELAMQ_ACCEPTANCE_MQTTS_PORT=18941 \
      VELAMQ_ACCEPTANCE_CLUSTER_PORT=55231 \
      VELAMQ_ACCEPTANCE_WORK_DIR="${WORK_DIR}/velamq" \
      "${ROOT_DIR}/scripts/run-real-velamq-acceptance.sh"
elif [[ "$PROFILE" == "site" ]]; then
  echo "VELAMQ_REPO is required for the site release profile" >&2
  OVERALL_STATUS=failed
  skip_gate velamq-tls-qos1 true "VELAMQ_REPO is missing"
  write_report "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  exit 2
else
  skip_gate velamq-tls-qos1 false "set VELAMQ_REPO to run broker-source acceptance"
fi

if [[ -n "$FIELD_CAMPAIGN_PLAN" ]]; then
  run_field_site_status_gate field-campaign-site
elif [[ "$PROFILE" == "site" ]]; then
  echo "EDGEOPS_FIELD_CAMPAIGN_PLAN is required for the site release profile" >&2
  OVERALL_STATUS=failed
  skip_gate field-campaign-site true "EDGEOPS_FIELD_CAMPAIGN_PLAN is missing"
  write_report "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  exit 2
elif [[ -n "$FIELD_CAMPAIGN_DIRS" ]]; then
  run_field_interoperability_gate field-interoperability "$FIELD_CAMPAIGN_DIRS"
elif [[ -n "${EDGEOPS_FIELD_CONFIG:-}" ]]; then
  run_gate field-preflight "field-preflight/report.json" \
    env \
      EDGEOPS_FIELD_PREFLIGHT_ONLY=1 \
      EDGEOPS_FIELD_WORK_DIR="${WORK_DIR}/field-preflight" \
      "${ROOT_DIR}/scripts/run-field-hardware-acceptance.sh"
else
  run_gate field-preflight "field-preflight/report.json" \
    env \
      EDGEOPS_FIELD_PREFLIGHT_WORK_DIR="${WORK_DIR}/field-preflight" \
      "${ROOT_DIR}/scripts/run-field-preflight-acceptance.sh"
fi

OVERALL_STATUS=passed
FINISHED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
write_report "$FINISHED_AT"
jq '.' "$REPORT_PATH"
echo "release-gate evidence: $WORK_DIR"
