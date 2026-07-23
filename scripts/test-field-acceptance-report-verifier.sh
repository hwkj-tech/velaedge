#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${EDGEOPS_FIELD_VERIFIER_TEST_WORK_DIR:-${ROOT_DIR}/target/field-verifier-contract-$$}"
PREFLIGHT_DIR="${WORK_DIR}/preflight"
PHYSICAL_DIR="${WORK_DIR}/synthetic-physical"
REPORT_PATH="${WORK_DIR}/report.json"

mkdir -p "$PREFLIGHT_DIR" "$PHYSICAL_DIR"
EDGEOPS_FIELD_PREFLIGHT_WORK_DIR="$PREFLIGHT_DIR" \
  "${ROOT_DIR}/scripts/run-field-preflight-acceptance.sh" >/dev/null

"${ROOT_DIR}/scripts/verify-field-acceptance-report.sh" "$PREFLIGHT_DIR/report.json" >/dev/null
if "${ROOT_DIR}/scripts/verify-field-acceptance-report.sh" --require-physical \
  "$PREFLIGHT_DIR/report.json" >/dev/null 2>&1; then
  echo "preflight report was incorrectly accepted as physical evidence" >&2
  exit 1
fi

cp "$PREFLIGHT_DIR/configuration-package.json" \
  "$PREFLIGHT_DIR/server-certificate.json" \
  "$PREFLIGHT_DIR/runtime-certificate.json" \
  "$PHYSICAL_DIR/"
printf '%s\n' \
  '{"broker":"contract-test","topic":"field/edge-preflight/meter-1/telemetry","received":true}' \
  >"${PHYSICAL_DIR}/broker-receipt.evidence"

FILES='[]'
for file in configuration-package.json server-certificate.json runtime-certificate.json broker-receipt.evidence; do
  digest="$(shasum -a 256 "${PHYSICAL_DIR}/${file}" | awk '{print $1}')"
  bytes="$(stat -f '%z' "${PHYSICAL_DIR}/${file}" 2>/dev/null || stat -c '%s' "${PHYSICAL_DIR}/${file}")"
  FILES="$(printf '%s' "$FILES" | jq \
    --arg path "$file" --arg sha256 "$digest" --argjson bytes "$bytes" \
    '. + [{path:$path,sha256:$sha256,bytes:$bytes}]')"
done
jq -n --argjson files "$FILES" \
  '{schemaVersion:1,mode:"physical-field",createdAt:"contract-test",files:$files}' \
  >"${PHYSICAL_DIR}/evidence-manifest.json"

MANIFEST_SHA256="$(shasum -a 256 "${PHYSICAL_DIR}/evidence-manifest.json" | awk '{print $1}')"
PACKAGE_SHA256="$(shasum -a 256 "${PHYSICAL_DIR}/configuration-package.json" | awk '{print $1}')"
BROKER_SHA256="$(shasum -a 256 "${PHYSICAL_DIR}/broker-receipt.evidence" | awk '{print $1}')"

jq \
  --arg manifest "$MANIFEST_SHA256" \
  --arg package "$PACKAGE_SHA256" \
  --arg broker "$BROKER_SHA256" '
    .mode="physical-field"
    | .syntheticContractTest=true
    | .physicalDeviceExercised=true
    | .siteId="synthetic-contract-test"
    | .operator="automated-test"
    | .physicalDevice={model:"synthetic-model",serialNumber:"synthetic-serial",operatorConfirmed:true}
    | .serial.path="/dev/ttyUSB-contract-test"
    | .runtimeId="runtime-contract"
    | .releaseId="release-contract"
    | .packageSha256=$package
    | .evidenceManifestSha256=$manifest
    | .brokerReceiptSha256=$broker
    | .samplesCollected=1
    | .mqttMessagesPublished=1
    | .mqttDistinctRoutes=1
    | .edgeLinkMessagesAcknowledged=1
    | .mqttAcknowledgements={receiptCount:1,acknowledgements:[{
        sinkId:"velamq-main",broker:"mqtt://127.0.0.1:1883",clientId:"edge-preflight",
        topic:"field/edge-preflight/meter-1/telemetry",qos:1,payloadBytes:42
      }]}
    | .runtimeStatus={edges:[{
        edge_id:.edgeId,runtime_id:"runtime-contract",config_version:.configVersion,
        cloud_sync:{connected:true,reported_version:.configVersion},local_store:{backend:"rocksdb"}
      }]}
    | .releases={applyResults:[{
        edgeId:.edgeId,desiredVersion:.configVersion,reportedVersion:.configVersion,result:"已应用"
      }]}
    | .evidence.brokerReceipt="broker-receipt.evidence"
  ' "$PREFLIGHT_DIR/report.json" >"${PHYSICAL_DIR}/report.json"

"${ROOT_DIR}/scripts/verify-field-acceptance-report.sh" --require-physical \
  "${PHYSICAL_DIR}/report.json" >/dev/null

printf '\ntampered\n' >>"${PHYSICAL_DIR}/broker-receipt.evidence"
if "${ROOT_DIR}/scripts/verify-field-acceptance-report.sh" --require-physical \
  "${PHYSICAL_DIR}/report.json" >/dev/null 2>"${WORK_DIR}/tamper-rejection.log"; then
  echo "tampered evidence was incorrectly accepted" >&2
  exit 1
fi
grep -q 'evidence digest mismatch: broker-receipt.evidence' "${WORK_DIR}/tamper-rejection.log"

jq -n \
  --arg finishedAt "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" \
  '{
    status:"passed",mode:"verifier-contract",finishedAt:$finishedAt,
    checks:{preflightAccepted:true,preflightRejectedAsPhysical:true,physicalContractAccepted:true,tamperRejected:true},
    note:"The synthetic physical report validates the verifier contract only and is not field evidence."
  }' | tee "$REPORT_PATH"

echo "field acceptance verifier contract evidence: $WORK_DIR"
