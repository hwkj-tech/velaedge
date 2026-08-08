#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${EDGEOPS_PROTOCOL_MATRIX_WORK_DIR:-${ROOT_DIR}/target/protocol-matrix-acceptance-$$}"
REPORT_PATH="${EDGEOPS_PROTOCOL_MATRIX_REPORT:-${WORK_DIR}/report.json}"
FILTER="${EDGEOPS_PROTOCOL_MATRIX_FILTER:-}"
STARTED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
STARTED_SECONDS="$(date +%s)"
RESULTS='[]'
DECLARED_PROTOCOL_IDS='[]'
OVERALL_STATUS=passed

for command in cargo git jq rg shasum; do
  command -v "$command" >/dev/null || {
    echo "protocol matrix acceptance: missing required command: $command" >&2
    exit 2
  }
done

mkdir -p "$WORK_DIR" "$(dirname "$REPORT_PATH")"

GIT_COMMIT="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || printf unknown)"
GIT_DIRTY=false
if [[ -n "$(git -C "$ROOT_DIR" status --porcelain 2>/dev/null)" ]]; then
  GIT_DIRTY=true
fi
SCRIPT_SHA256="$(shasum -a 256 "${BASH_SOURCE[0]}" | awk '{print $1}')"
COMMON_SOURCE_FILES="crates/edge-core/src/config.rs crates/edge-runtime/src/configured_runtime.rs crates/edge-runtime/src/protocol_catalog.rs"

is_selected() {
  local protocol_id="$1"
  [[ -z "$FILTER" ]] && return 0
  local item
  local -a items
  IFS=',' read -r -a items <<<"$FILTER"
  for item in "${items[@]}"; do
    [[ "$item" == "$protocol_id" ]] && return 0
  done
  return 1
}

append_result() {
  local protocol_id="$1"
  local display_name="$2"
  local status="$3"
  local transport="$4"
  local capabilities="$5"
  local test_count="$6"
  local duration_seconds="$7"
  local log="$8"
  local source_sha256="$9"
  local source_files="${10}"
  local test_targets="${11}"
  local test_invocations="${12}"
  RESULTS="$(printf '%s' "$RESULTS" | jq \
    --arg protocolId "$protocol_id" \
    --arg displayName "$display_name" \
    --arg status "$status" \
    --arg transport "$transport" \
    --argjson capabilities "$capabilities" \
    --argjson testCount "$test_count" \
    --argjson durationSeconds "$duration_seconds" \
    --arg log "$log" \
    --arg sourceSha256 "$source_sha256" \
    --argjson sourceFiles "$source_files" \
    --argjson testTargets "$test_targets" \
    --argjson testInvocations "$test_invocations" \
    '. + [{
      protocolId:$protocolId,
      displayName:$displayName,
      status:$status,
      transportEvidence:$transport,
      capabilities:$capabilities,
      testCount:$testCount,
      durationSeconds:$durationSeconds,
      log:$log,
      sourceSha256:$sourceSha256,
      sourceFiles:$sourceFiles,
      testTargets:$testTargets,
      testInvocations:$testInvocations
    }]')"
}

run_protocol() {
  local protocol_id="$1"
  local display_name="$2"
  local transport="$3"
  local capabilities="$4"
  local test_specs="$5"
  local source_files="$6"

  DECLARED_PROTOCOL_IDS="$(printf '%s' "$DECLARED_PROTOCOL_IDS" | jq \
    --arg protocolId "$protocol_id" '. + [$protocolId]')"

  is_selected "$protocol_id" || return 0

  local log_path="${WORK_DIR}/${protocol_id}.log"
  local started_seconds duration_seconds test_count status source_sha256
  local source_files_json='[]'
  local test_targets_json='[]'
  local test_invocations_json='[]'
  local source_file_sha256
  local -a command
  local test_spec test_target test_filter source_file
  local invocation_index=0 invocation_log invocation_test_count invocation_status command_status

  for test_spec in $test_specs; do
    test_target="${test_spec%%::*}"
    if [[ "$test_spec" == *::* ]]; then
      test_filter="${test_spec#*::}"
    else
      test_filter=""
    fi
    test_targets_json="$(printf '%s' "$test_targets_json" | jq \
      --arg target "$test_target" 'if index($target) then . else . + [$target] end')"
  done

  started_seconds="$(date +%s)"
  echo "[protocol-matrix] running: ${display_name}"
  status=passed
  : >"$log_path"
  for test_spec in $test_specs; do
    test_target="${test_spec%%::*}"
    if [[ "$test_spec" == *::* ]]; then
      test_filter="${test_spec#*::}"
    else
      test_filter=""
    fi
    command=(cargo test --manifest-path "${ROOT_DIR}/Cargo.toml" -p edge-runtime --test "$test_target")
    if [[ -n "$test_filter" ]]; then
      command+=("$test_filter")
    fi
    invocation_index=$((invocation_index + 1))
    invocation_log="${WORK_DIR}/${protocol_id}-${invocation_index}-${test_target}.log"
    printf '\n$' >>"$log_path"
    printf ' %q' "${command[@]}" >>"$log_path"
    printf '%s\n' ' -- --nocapture' >>"$log_path"
    command_status=0
    "${command[@]}" -- --nocapture >"$invocation_log" 2>&1 || command_status=$?
    cat "$invocation_log" >>"$log_path"
    invocation_test_count="$(
      { rg -o '[0-9]+ passed' "$invocation_log" || true; } \
        | awk '{total += $1} END {print total + 0}'
    )"
    invocation_status=passed
    if [[ "$command_status" -ne 0 || "$invocation_test_count" -eq 0 ]]; then
      invocation_status=failed
      status=failed
      OVERALL_STATUS=failed
    fi
    test_invocations_json="$(printf '%s' "$test_invocations_json" | jq \
      --arg target "$test_target" \
      --arg filter "$test_filter" \
      --arg status "$invocation_status" \
      --arg log "$(basename "$invocation_log")" \
      --argjson testCount "$invocation_test_count" \
      '. + [{
        target:$target,
        filter:(if $filter == "" then null else $filter end),
        status:$status,
        testCount:$testCount,
        log:$log
      }]')"
  done
  duration_seconds=$(( $(date +%s) - started_seconds ))
  test_count="$(printf '%s' "$test_invocations_json" | jq '[.[].testCount] | add // 0')"

  for source_file in $COMMON_SOURCE_FILES $source_files; do
    source_file_sha256="$(shasum -a 256 "${ROOT_DIR}/${source_file}" | awk '{print $1}')"
    source_files_json="$(printf '%s' "$source_files_json" | jq \
      --arg path "$source_file" \
      --arg sha256 "$source_file_sha256" \
      '. + [{path:$path,sha256:$sha256}]')"
  done
  source_sha256="$(printf '%s' "$source_files_json" | jq -cS '.' | shasum -a 256 | awk '{print $1}')"

  append_result \
    "$protocol_id" "$display_name" "$status" "$transport" "$capabilities" \
    "$test_count" "$duration_seconds" "$(basename "$log_path")" "$source_sha256" \
    "$source_files_json" "$test_targets_json" "$test_invocations_json"

  echo "[protocol-matrix] ${status}: ${display_name} (${test_count} tests, ${duration_seconds}s)"
  if [[ "$status" == failed ]]; then
    tail -120 "$log_path" >&2
  fi
}

run_protocol \
  modbus-tcp "Modbus TCP" "real loopback TCP frames" \
  '["read","write","batch-read","batch-write","runtime-graph","mqtt-qos1"]' \
  "modbus_tcp modbus_tcp_endurance" \
  "crates/edge-runtime/src/modbus_tcp.rs crates/edge-runtime/tests/modbus_tcp.rs crates/edge-runtime/tests/modbus_tcp_endurance.rs"

run_protocol \
  modbus-rtu "Modbus RTU" "production serial codec and operating-system PTY" \
  '["read","write","batch-read","batch-write","runtime-graph"]' \
  "modbus_rtu serial_pty::production_serial_factory_round_trips_modbus_over_a_pty serial_pty::modbus_pty_runtime_publishes_qos1_data_config_payload" \
  "crates/edge-runtime/src/modbus_rtu.rs crates/edge-runtime/tests/modbus_rtu.rs crates/edge-runtime/tests/serial_pty.rs"

run_protocol \
  opc-ua-client "OPC UA Client" "real embedded OPC UA TCP server" \
  '["read","write","readback-verification","subscription","browse","browse-path-translate","session-recovery","runtime-graph"]' \
  "opcua" \
  "crates/edge-runtime/src/opcua.rs crates/edge-runtime/tests/opcua.rs"

run_protocol \
  dlt645-2007 "DL/T 645-2007" "production serial codec and operating-system PTY" \
  '["read","multi-meter","deduplicate","partial-failure-isolation","vendor-di-contract","runtime-graph"]' \
  "dlt645 serial_pty::dlt645_pty_runtime_publishes_qos1_data_config_payload" \
  "crates/edge-runtime/src/dlt645.rs crates/edge-runtime/tests/dlt645.rs crates/edge-runtime/tests/serial_pty.rs"

run_protocol \
  iec60870-5-101-unbalanced "IEC 60870-5-101" "production serial codec and operating-system PTY" \
  '["read","write","single-command","double-command","short-float-command","select-before-operate","activation-confirmation","link-reset","class-1-poll","class-2-poll","cp24-time","cp56-time","runtime-graph"]' \
  "iec101 serial_pty::iec101_pty" \
  "crates/edge-runtime/src/iec101.rs crates/edge-runtime/tests/iec101.rs crates/edge-runtime/tests/serial_pty.rs"

run_protocol \
  iec60870-5-104-client "IEC 60870-5-104" "real loopback TCP frames" \
  '["read","write","single-command","double-command","short-float-command","select-before-operate","activation-confirmation","startdt","general-interrogation","session-reuse","quality","cp56-time"]' \
  "iec104" \
  "crates/edge-runtime/src/iec104.rs crates/edge-runtime/tests/iec104.rs"

run_protocol \
  bacnet-ip "BACnet/IP" "real loopback UDP frames" \
  '["read","write-property","write-priority","read-property-multiple","single-point-fallback","session-reuse","bbmd-foreign-device","cov-subscription","cov-renewal","polling-fallback"]' \
  "bacnet_ip" \
  "crates/edge-runtime/src/bacnet_ip.rs crates/edge-runtime/tests/bacnet_ip.rs"

run_protocol \
  siemens-s7 "Siemens S7" "real loopback ISO-on-TCP/S7 frames" \
  '["read","write","batch-read","session-reuse","permission-enforcement"]' \
  "siemens_s7" \
  "crates/edge-runtime/src/siemens_s7.rs crates/edge-runtime/tests/siemens_s7.rs"

run_protocol \
  omron-fins "Omron FINS" "real loopback UDP and TCP frames" \
  '["udp","tcp","node-handshake","read","write","batch-read","session-reuse","reconnect","permission-enforcement"]' \
  "omron_fins" \
  "crates/edge-runtime/src/omron_fins.rs crates/edge-runtime/tests/omron_fins.rs"

run_protocol \
  custom-serial-frame-dsl-v2 "Custom Serial Frame DSL" "production serial codec and operating-system PTY" \
  '["read","frame-template","dsl-v1-compatibility","dsl-v2","raw","slip","cobs","sum8","xor8","modbus-crc16","crc16-ccitt-false","field-decode","runtime-graph","mqtt-qos1"]' \
  "custom_serial serial_pty::custom_serial_v2_pty" \
  "crates/edge-runtime/src/custom_serial.rs crates/edge-runtime/tests/custom_serial.rs crates/edge-runtime/tests/serial_pty.rs"

CATALOG_JSON="$(cargo run --quiet --manifest-path "${ROOT_DIR}/Cargo.toml" \
  -p edge-runtime --bin protocol-catalog)"
CATALOG_PROTOCOL_IDS="$(printf '%s' "$CATALOG_JSON" | jq -c \
  '[.[] | select(.capabilityId != "simulated" and .maturity != "planned") | .capabilityId] | sort')"
DECLARED_PROTOCOL_IDS_SORTED="$(printf '%s' "$DECLARED_PROTOCOL_IDS" | jq -c 'sort')"
DECLARED_UNIQUE_COUNT="$(printf '%s' "$DECLARED_PROTOCOL_IDS" | jq 'unique | length')"
DECLARED_COUNT="$(printf '%s' "$DECLARED_PROTOCOL_IDS" | jq 'length')"

if [[ "$DECLARED_COUNT" -ne "$DECLARED_UNIQUE_COUNT" ]]; then
  echo "protocol matrix acceptance: duplicate protocol declarations" >&2
  exit 2
fi

if [[ "$CATALOG_PROTOCOL_IDS" != "$DECLARED_PROTOCOL_IDS_SORTED" ]]; then
  echo "protocol matrix acceptance: matrix declarations do not match the runtime protocol catalog" >&2
  echo "catalog:  $CATALOG_PROTOCOL_IDS" >&2
  echo "declared: $DECLARED_PROTOCOL_IDS_SORTED" >&2
  exit 2
fi

CATALOG_SHA256="$(printf '%s' "$CATALOG_JSON" | jq -cS '.' | shasum -a 256 | awk '{print $1}')"

FINISHED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
DURATION_SECONDS=$(( $(date +%s) - STARTED_SECONDS ))
SELECTED_COUNT="$(printf '%s' "$RESULTS" | jq 'length')"
PASSED_COUNT="$(printf '%s' "$RESULTS" | jq '[.[] | select(.status == "passed")] | length')"
FAILED_COUNT="$(printf '%s' "$RESULTS" | jq '[.[] | select(.status == "failed")] | length')"

if [[ "$SELECTED_COUNT" == 0 ]]; then
  echo "protocol matrix acceptance: filter selected no protocols: $FILTER" >&2
  exit 2
fi

jq -n \
  --argjson schemaVersion 3 \
  --arg status "$OVERALL_STATUS" \
  --arg mode "automated-protocol-integration-lab" \
  --arg filter "$FILTER" \
  --arg startedAt "$STARTED_AT" \
  --arg finishedAt "$FINISHED_AT" \
  --arg gitCommit "$GIT_COMMIT" \
  --arg scriptSha256 "$SCRIPT_SHA256" \
  --arg catalogSha256 "$CATALOG_SHA256" \
  --argjson gitDirty "$GIT_DIRTY" \
  --argjson physicalDeviceExercised false \
  --argjson durationSeconds "$DURATION_SECONDS" \
  --argjson selectedCount "$SELECTED_COUNT" \
  --argjson passedCount "$PASSED_COUNT" \
  --argjson failedCount "$FAILED_COUNT" \
  --argjson runtimeProtocolCatalog "$CATALOG_JSON" \
  --argjson declaredProtocolIds "$DECLARED_PROTOCOL_IDS_SORTED" \
  --argjson protocols "$RESULTS" \
  '{
    schemaVersion:$schemaVersion,
    status:$status,
    mode:$mode,
    filter:$filter,
    physicalDeviceExercised:$physicalDeviceExercised,
    startedAt:$startedAt,
    finishedAt:$finishedAt,
    durationSeconds:$durationSeconds,
    source:{
      gitCommit:$gitCommit,
      gitDirty:$gitDirty,
      scriptSha256:$scriptSha256,
      runtimeProtocolCatalogSha256:$catalogSha256
    },
    catalogCoverage:{status:"passed",declaredProtocolIds:$declaredProtocolIds},
    summary:{selected:$selectedCount,passed:$passedCount,failed:$failedCount},
    runtimeProtocolCatalog:$runtimeProtocolCatalog,
    protocols:$protocols,
    limitation:"Loopback TCP/UDP servers and PTYs verify production protocol code paths, not vendor interoperability, field wiring, or 24-hour physical-device stability."
  }' >"$REPORT_PATH"

jq '.' "$REPORT_PATH"
echo "protocol matrix acceptance evidence: $WORK_DIR"

if [[ "$OVERALL_STATUS" != passed ]]; then
  exit 1
fi
