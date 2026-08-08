#!/usr/bin/env bash
set -euo pipefail

fail() {
  echo "field campaign runner: $*" >&2
  exit 2
}

preflight_only=0
if [[ "${1:-}" == "--preflight-only" ]]; then
  preflight_only=1
  shift
fi
[[ "$#" -eq 0 ]] || fail "unsupported argument: $1"

require_value() {
  local name="$1"
  [[ -n "${!name:-}" ]] || fail "$name is required"
}

require_unsigned() {
  local name="$1"
  local value="${!name:-}"
  [[ "$value" =~ ^[0-9]+$ ]] || fail "$name must be an unsigned integer"
}

require_absolute_path() {
  local name="$1"
  local value="${!name:-}"
  [[ "$value" == /* ]] || fail "$name must be an absolute path"
}

for name in \
  EDGEOPS_FIELD_CAMPAIGN_CONFIG \
  EDGEOPS_FIELD_CAMPAIGN_OUTPUT_DIR \
  EDGEOPS_FIELD_CAMPAIGN_NATIVE_BROKER_AUDIT \
  EDGEOPS_FIELD_CAMPAIGN_SITE_ID \
  EDGEOPS_FIELD_CAMPAIGN_OPERATOR \
  EDGEOPS_FIELD_CAMPAIGN_DEVICE_CONNECTION_ID \
  EDGEOPS_FIELD_CAMPAIGN_DEVICE_MANUFACTURER \
  EDGEOPS_FIELD_CAMPAIGN_DEVICE_MODEL \
  EDGEOPS_FIELD_CAMPAIGN_DEVICE_SERIAL; do
  require_value "$name"
done

[[ "${EDGEOPS_FIELD_CAMPAIGN_PHYSICAL_DEVICE_CONFIRMED:-0}" == "1" ]] || \
  fail "EDGEOPS_FIELD_CAMPAIGN_PHYSICAL_DEVICE_CONFIRMED=1 is required"

require_absolute_path EDGEOPS_FIELD_CAMPAIGN_CONFIG
require_absolute_path EDGEOPS_FIELD_CAMPAIGN_OUTPUT_DIR
require_absolute_path EDGEOPS_FIELD_CAMPAIGN_NATIVE_BROKER_AUDIT
[[ -f "$EDGEOPS_FIELD_CAMPAIGN_CONFIG" ]] || \
  fail "configuration package does not exist: $EDGEOPS_FIELD_CAMPAIGN_CONFIG"

EDGEOPS_FIELD_CAMPAIGN_DURATION_SECONDS="${EDGEOPS_FIELD_CAMPAIGN_DURATION_SECONDS:-86400}"
EDGEOPS_FIELD_CAMPAIGN_SCHEDULER_INTERVAL_MS="${EDGEOPS_FIELD_CAMPAIGN_SCHEDULER_INTERVAL_MS:-100}"
EDGEOPS_FIELD_CAMPAIGN_MAXIMUM_FAILURE_RATIO="${EDGEOPS_FIELD_CAMPAIGN_MAXIMUM_FAILURE_RATIO:-0.01}"
EDGEOPS_FIELD_CAMPAIGN_MAXIMUM_PROGRESS_GAP_SECONDS="${EDGEOPS_FIELD_CAMPAIGN_MAXIMUM_PROGRESS_GAP_SECONDS:-300}"
EDGEOPS_FIELD_CAMPAIGN_RECEIPT_STARTUP_TIMEOUT_SECONDS="${EDGEOPS_FIELD_CAMPAIGN_RECEIPT_STARTUP_TIMEOUT_SECONDS:-30}"
EDGEOPS_FIELD_CAMPAIGN_RECEIPT_POST_RUN_GRACE_SECONDS="${EDGEOPS_FIELD_CAMPAIGN_RECEIPT_POST_RUN_GRACE_SECONDS:-60}"
EDGEOPS_FIELD_CAMPAIGN_NATIVE_BROKER_AUDIT_WAIT_SECONDS="${EDGEOPS_FIELD_CAMPAIGN_NATIVE_BROKER_AUDIT_WAIT_SECONDS:-300}"

for name in \
  EDGEOPS_FIELD_CAMPAIGN_DURATION_SECONDS \
  EDGEOPS_FIELD_CAMPAIGN_SCHEDULER_INTERVAL_MS \
  EDGEOPS_FIELD_CAMPAIGN_MAXIMUM_PROGRESS_GAP_SECONDS \
  EDGEOPS_FIELD_CAMPAIGN_RECEIPT_STARTUP_TIMEOUT_SECONDS \
  EDGEOPS_FIELD_CAMPAIGN_RECEIPT_POST_RUN_GRACE_SECONDS \
  EDGEOPS_FIELD_CAMPAIGN_NATIVE_BROKER_AUDIT_WAIT_SECONDS; do
  require_unsigned "$name"
done
(( EDGEOPS_FIELD_CAMPAIGN_DURATION_SECONDS > 0 )) || \
  fail "EDGEOPS_FIELD_CAMPAIGN_DURATION_SECONDS must be greater than zero"
(( EDGEOPS_FIELD_CAMPAIGN_SCHEDULER_INTERVAL_MS > 0 )) || \
  fail "EDGEOPS_FIELD_CAMPAIGN_SCHEDULER_INTERVAL_MS must be greater than zero"
(( EDGEOPS_FIELD_CAMPAIGN_MAXIMUM_PROGRESS_GAP_SECONDS > 0 )) || \
  fail "EDGEOPS_FIELD_CAMPAIGN_MAXIMUM_PROGRESS_GAP_SECONDS must be greater than zero"
(( EDGEOPS_FIELD_CAMPAIGN_RECEIPT_STARTUP_TIMEOUT_SECONDS > 0 )) || \
  fail "EDGEOPS_FIELD_CAMPAIGN_RECEIPT_STARTUP_TIMEOUT_SECONDS must be greater than zero"
(( EDGEOPS_FIELD_CAMPAIGN_NATIVE_BROKER_AUDIT_WAIT_SECONDS > 0 )) || \
  fail "EDGEOPS_FIELD_CAMPAIGN_NATIVE_BROKER_AUDIT_WAIT_SECONDS must be greater than zero"
[[ "$EDGEOPS_FIELD_CAMPAIGN_MAXIMUM_FAILURE_RATIO" =~ ^(0(\.[0-9]+)?|1(\.0+)?)$ ]] || \
  fail "EDGEOPS_FIELD_CAMPAIGN_MAXIMUM_FAILURE_RATIO must be between 0 and 1"

campaign_binary="${EDGEOPS_FIELD_CAMPAIGN_BINARY:-/opt/edgeops/bin/field-campaign}"
[[ -x "$campaign_binary" ]] || fail "field-campaign binary is not executable: $campaign_binary"

umask 077
mkdir -p \
  "$(dirname "$EDGEOPS_FIELD_CAMPAIGN_OUTPUT_DIR")" \
  "$(dirname "$EDGEOPS_FIELD_CAMPAIGN_NATIVE_BROKER_AUDIT")"

command=(
  "$campaign_binary"
  --config "$EDGEOPS_FIELD_CAMPAIGN_CONFIG"
  --output-dir "$EDGEOPS_FIELD_CAMPAIGN_OUTPUT_DIR"
  --native-broker-audit "$EDGEOPS_FIELD_CAMPAIGN_NATIVE_BROKER_AUDIT"
  --native-broker-audit-wait-seconds "$EDGEOPS_FIELD_CAMPAIGN_NATIVE_BROKER_AUDIT_WAIT_SECONDS"
  --duration-seconds "$EDGEOPS_FIELD_CAMPAIGN_DURATION_SECONDS"
  --scheduler-interval-ms "$EDGEOPS_FIELD_CAMPAIGN_SCHEDULER_INTERVAL_MS"
  --maximum-failure-ratio "$EDGEOPS_FIELD_CAMPAIGN_MAXIMUM_FAILURE_RATIO"
  --maximum-progress-gap-seconds "$EDGEOPS_FIELD_CAMPAIGN_MAXIMUM_PROGRESS_GAP_SECONDS"
  --receipt-startup-timeout-seconds "$EDGEOPS_FIELD_CAMPAIGN_RECEIPT_STARTUP_TIMEOUT_SECONDS"
  --receipt-post-run-grace-seconds "$EDGEOPS_FIELD_CAMPAIGN_RECEIPT_POST_RUN_GRACE_SECONDS"
  --physical-device-exercised
  --site-id "$EDGEOPS_FIELD_CAMPAIGN_SITE_ID"
  --operator "$EDGEOPS_FIELD_CAMPAIGN_OPERATOR"
  --device-connection-id "$EDGEOPS_FIELD_CAMPAIGN_DEVICE_CONNECTION_ID"
  --device-manufacturer "$EDGEOPS_FIELD_CAMPAIGN_DEVICE_MANUFACTURER"
  --device-model "$EDGEOPS_FIELD_CAMPAIGN_DEVICE_MODEL"
  --device-serial "$EDGEOPS_FIELD_CAMPAIGN_DEVICE_SERIAL"
)

if [[ -n "${EDGEOPS_FIELD_CAMPAIGN_MINIMUM_CYCLES:-}" ]]; then
  require_unsigned EDGEOPS_FIELD_CAMPAIGN_MINIMUM_CYCLES
  (( EDGEOPS_FIELD_CAMPAIGN_MINIMUM_CYCLES > 0 )) || \
    fail "EDGEOPS_FIELD_CAMPAIGN_MINIMUM_CYCLES must be greater than zero"
  command+=(--minimum-cycles "$EDGEOPS_FIELD_CAMPAIGN_MINIMUM_CYCLES")
fi
if [[ -n "${EDGEOPS_FIELD_CAMPAIGN_ROCKSDB_PATH:-}" ]]; then
  require_absolute_path EDGEOPS_FIELD_CAMPAIGN_ROCKSDB_PATH
  command+=(--rocksdb-path "$EDGEOPS_FIELD_CAMPAIGN_ROCKSDB_PATH")
fi
if [[ "${EDGEOPS_FIELD_CAMPAIGN_REQUIRE_RECOVERY:-0}" == "1" ]]; then
  command+=(--require-recovery)
elif [[ "${EDGEOPS_FIELD_CAMPAIGN_REQUIRE_RECOVERY:-0}" != "0" ]]; then
  fail "EDGEOPS_FIELD_CAMPAIGN_REQUIRE_RECOVERY must be 0 or 1"
fi
if [[ -n "${EDGEOPS_FIELD_CAMPAIGN_CHANGING_POINTS:-}" ]]; then
  IFS=',' read -r -a changing_points <<<"$EDGEOPS_FIELD_CAMPAIGN_CHANGING_POINTS"
  for point in "${changing_points[@]}"; do
    [[ -n "$point" && "$point" == */* ]] || \
      fail "each changing point must use DEVICE_ID/POINT_ID"
    command+=(--require-changing-point "$point")
  done
fi
if [[ "$preflight_only" -eq 1 ]]; then
  command+=(--preflight-only)
fi

exec "${command[@]}"
