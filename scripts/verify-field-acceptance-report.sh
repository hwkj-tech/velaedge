#!/usr/bin/env bash
set -euo pipefail

REQUIRE_PHYSICAL=0
if [[ "${1:-}" == "--require-physical" ]]; then
  REQUIRE_PHYSICAL=1
  shift
fi

REPORT_PATH="${1:-}"
[[ -n "$REPORT_PATH" && $# -eq 1 ]] || {
  echo "usage: $0 [--require-physical] REPORT_JSON" >&2
  exit 2
}
[[ -f "$REPORT_PATH" ]] || { echo "acceptance report does not exist: $REPORT_PATH" >&2; exit 2; }

for command in jq shasum stat; do
  command -v "$command" >/dev/null || { echo "missing required command: $command" >&2; exit 2; }
done

fail() {
  echo "field acceptance report verification: $*" >&2
  exit 2
}

file_size() {
  stat -f '%z' "$1" 2>/dev/null || stat -c '%s' "$1"
}

REPORT_DIR="$(cd "$(dirname "$REPORT_PATH")" && pwd)"
REPORT_PATH="${REPORT_DIR}/$(basename "$REPORT_PATH")"
jq -e 'type == "object"' "$REPORT_PATH" >/dev/null || fail "report is not a JSON object"

MODE="$(jq -er '.mode | select(. == "preflight" or . == "physical-field")' "$REPORT_PATH")" || \
  fail "mode must be preflight or physical-field"
[[ "$REQUIRE_PHYSICAL" -eq 0 || "$MODE" == "physical-field" ]] || fail "physical field evidence is required"

jq -e '
  .status == "passed"
  and (.edgeId | type == "string" and length > 0)
  and (.configVersion | type == "string" and length > 0)
  and (.packageSha256 | test("^[0-9a-f]{64}$"))
  and (.evidenceManifestSha256 | test("^[0-9a-f]{64}$"))
  and .evidence.manifest == "evidence-manifest.json"
  and .evidence.configurationPackage == "configuration-package.json"
' "$REPORT_PATH" >/dev/null || fail "common report contract is invalid"

MANIFEST_PATH="${REPORT_DIR}/$(jq -r '.evidence.manifest' "$REPORT_PATH")"
[[ -f "$MANIFEST_PATH" ]] || fail "evidence manifest is missing"
MANIFEST_SHA256="$(shasum -a 256 "$MANIFEST_PATH" | awk '{print $1}')"
[[ "$MANIFEST_SHA256" == "$(jq -r '.evidenceManifestSha256' "$REPORT_PATH")" ]] || \
  fail "evidence manifest digest does not match the report"
jq -e --arg mode "$MODE" '
  .schemaVersion == 1 and .mode == $mode
  and (.createdAt | type == "string" and length > 0)
  and (.files | type == "array" and length > 0)
  and all(.files[];
    (.path | type == "string" and length > 0)
    and (.sha256 | test("^[0-9a-f]{64}$"))
    and (.bytes | type == "number" and . >= 0))
' "$MANIFEST_PATH" >/dev/null || fail "evidence manifest contract is invalid"

while IFS=$'\t' read -r relative expected_sha expected_bytes; do
  [[ "$relative" != /* && "$relative" != ".." && "$relative" != ../* && "$relative" != */../* ]] || \
    fail "unsafe evidence path: $relative"
  evidence_file="${REPORT_DIR}/${relative}"
  [[ -f "$evidence_file" ]] || fail "evidence file is missing: $relative"
  actual_sha="$(shasum -a 256 "$evidence_file" | awk '{print $1}')"
  [[ "$actual_sha" == "$expected_sha" ]] || fail "evidence digest mismatch: $relative"
  actual_bytes="$(file_size "$evidence_file")"
  [[ "$actual_bytes" == "$expected_bytes" ]] || fail "evidence size mismatch: $relative"
done < <(jq -r '.files[] | [.path,.sha256,(.bytes | tostring)] | @tsv' "$MANIFEST_PATH")

PACKAGE_PATH="${REPORT_DIR}/$(jq -r '.evidence.configurationPackage' "$REPORT_PATH")"
[[ "$(shasum -a 256 "$PACKAGE_PATH" | awk '{print $1}')" == "$(jq -r '.packageSha256' "$REPORT_PATH")" ]] || \
  fail "configuration package digest does not match the report"

if [[ "$MODE" == "preflight" ]]; then
  jq -e '
    .physicalDeviceExercised == false
    and (.serial.connections | type == "array" and length > 0)
    and (.mqtt | type == "array" and length > 0)
    and (.dataConfigs | type == "array" and length > 0)
  ' "$REPORT_PATH" >/dev/null || fail "preflight report contract is invalid"
else
  jq -e '
    . as $report
    | .physicalDeviceExercised == true
    and (.siteId | type == "string" and length > 0)
    and (.operator | type == "string" and length > 0)
    and (.physicalDevice.model | type == "string" and length > 0)
    and (.physicalDevice.serialNumber | type == "string" and length > 0)
    and .physicalDevice.operatorConfirmed == true
    and (.serial.path | type == "string" and length > 0)
    and (.serial.path | startswith("/dev/"))
    and (.serial.path | startswith("/dev/pts/") | not)
    and (.serial.path | startswith("/dev/ttys") | not)
    and (.serial.path != "/dev/null" and .serial.path != "/dev/zero")
    and (.samplesCollected | type == "number" and . > 0)
    and (.mqttMessagesPublished | type == "number" and . > 0)
    and (.edgeLinkMessagesAcknowledged | type == "number" and . > 0)
    and ($report.mqttDistinctRoutes | type == "number")
    and ($report.mqttDistinctRoutes >= $report.acceptancePolicy.minimumDistinctMqttRoutes)
    and (.mqttAcknowledgements.receiptCount == .mqttMessagesPublished)
    and all(.mqttAcknowledgements.acknowledgements[];
      (.topic | type == "string" and length > 0)
      and (.payloadBytes | type == "number" and . > 0)
      and (. as $ack | any($report.mqtt[];
        .sink_id == $ack.sinkId
        and .broker == $ack.broker
        and .client_id == $ack.clientId
        and .qos == $ack.qos)))
    and (.brokerReceiptSha256 | test("^[0-9a-f]{64}$"))
    and (.runtimeId | type == "string" and length > 0)
    and (.releaseId | type == "string" and length > 0)
    and (.evidence.brokerReceipt | type == "string" and length > 0)
    and any(.runtimeStatus.edges[];
      .edge_id == $report.edgeId
      and .runtime_id == $report.runtimeId
      and .config_version == $report.configVersion
      and .cloud_sync.connected == true
      and .cloud_sync.reported_version == $report.configVersion
      and (.local_store.backend | startswith("rocksdb")))
    and any(.releases.applyResults[];
      .edgeId == $report.edgeId
      and .desiredVersion == $report.configVersion
      and .reportedVersion == $report.configVersion
      and .result == "已应用")
  ' "$REPORT_PATH" >/dev/null || fail "physical field report contract is invalid"
  BROKER_RECEIPT_PATH="${REPORT_DIR}/$(jq -r '.evidence.brokerReceipt' "$REPORT_PATH")"
  [[ -s "$BROKER_RECEIPT_PATH" ]] || fail "broker-side receipt is missing or empty"
  [[ "$(shasum -a 256 "$BROKER_RECEIPT_PATH" | awk '{print $1}')" == "$(jq -r '.brokerReceiptSha256' "$REPORT_PATH")" ]] || \
    fail "broker-side receipt digest does not match the report"
fi

echo "field acceptance report verified: mode=${MODE} report=${REPORT_PATH}"
