#!/usr/bin/env bash
set -euo pipefail

API_BASE="${VELAEDGE_API_BASE:-http://127.0.0.1:8082}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="${VELAEDGE_DEMO_MANIFEST:-${ROOT_DIR}/deploy/demo/industrial-line-demo.json}"

for command in curl jq; do
  command -v "${command}" >/dev/null || {
    echo "missing required command: ${command}" >&2
    exit 1
  }
done

request() {
  local method="$1"
  local path="$2"
  local payload="$3"
  local output
  output="$(mktemp)"
  local status
  status="$(curl -sS -o "${output}" -w '%{http_code}' -X "${method}" \
    -H 'content-type: application/json' \
    --data-binary "${payload}" \
    "${API_BASE}${path}")"
  if [[ "${status}" -lt 200 || "${status}" -ge 300 ]]; then
    echo "${method} ${path} failed with HTTP ${status}" >&2
    cat "${output}" >&2
    rm -f "${output}"
    exit 1
  fi
  cat "${output}"
  rm -f "${output}"
}

project="$(jq -c '.project' "${MANIFEST}")"
project_id="$(jq -r '.project.projectId' "${MANIFEST}")"
if curl -fsS "${API_BASE}/api/projects" | jq -e --arg id "${project_id}" '.[] | select(.projectId == $id)' >/dev/null; then
  request PUT "/api/projects/${project_id}" "${project}" >/dev/null
else
  request POST /api/projects "${project}" >/dev/null
fi

while IFS= read -r point_set; do
  point_set_id="$(jq -r '.pointSetId' <<<"${point_set}")"
  if curl -fsS "${API_BASE}/api/point-sets" | jq -e --arg id "${point_set_id}" '.[] | select(.pointSetId == $id)' >/dev/null; then
    request PUT "/api/point-sets/${point_set_id}" "${point_set}" >/dev/null
  else
    request POST /api/point-sets "${point_set}" >/dev/null
  fi
done < <(jq -c '.pointSets[]' "${MANIFEST}")

product="$(jq -c '.product' "${MANIFEST}")"
product_id="$(jq -r '.product.productId' "${MANIFEST}")"
if curl -fsS "${API_BASE}/api/products" | jq -e --arg id "${product_id}" '.[] | select(.productId == $id)' >/dev/null; then
  request PUT "/api/products/${product_id}" "${product}" >/dev/null
else
  request POST /api/products "${product}" >/dev/null
fi

version="$(jq -c '.version' "${MANIFEST}")"
version_id="$(jq -r '.version.version' "${MANIFEST}")"
existing_status="$(curl -fsS "${API_BASE}/api/products/${product_id}/versions" | jq -r --arg version "${version_id}" '.[] | select(.version == $version) | .status' | head -1)"
if [[ -z "${existing_status}" ]]; then
  request POST "/api/products/${product_id}/versions" "${version}" >/dev/null
  existing_status="draft"
elif [[ "${existing_status}" == "draft" ]]; then
  request PUT "/api/products/${product_id}/versions/${version_id}" "${version}" >/dev/null
fi
if [[ "${existing_status}" == "draft" ]]; then
  request POST "/api/products/${product_id}/versions/${version_id}/publish" null >/dev/null
fi

edge_id="$(jq -r '.binding.edgeId' "${MANIFEST}")"
binding="$(jq -c '.binding | {projectId, productId, desiredVersion}' "${MANIFEST}")"
request PUT "/api/edge-nodes/${edge_id}/product-binding" "${binding}" >/dev/null

desired="$(curl -fsS "${API_BASE}/api/edges/${edge_id}/desired-config")"
expected_protocols="$(jq '.version.protocolConnections | length' "${MANIFEST}")"
expected_points="$(jq '[.pointSets[].points[]] | length' "${MANIFEST}")"
expected_flows="$(jq '.version.dataConfigs | length' "${MANIFEST}")"
expected_outputs="$(jq '[.version.dataConfigs[].visual_graph.nodes[] | select(.kind == "Mqtt")] | length' "${MANIFEST}")"
expected_commands="$(jq '.version.commandFlows | length' "${MANIFEST}")"
actual_version="$(jq -r '.desiredVersion' <<<"${desired}")"
actual_protocols="$(jq '.package.protocol_connections | length' <<<"${desired}")"
actual_points="$(jq '.package.point_mappings | length' <<<"${desired}")"
actual_flows="$(jq '.package.data_configs | length' <<<"${desired}")"
actual_outputs="$(jq '[.package.data_configs[].visual_graph.nodes[] | select(.kind == "Mqtt")] | length' <<<"${desired}")"
actual_commands="$(jq '.package.command_flows | length' <<<"${desired}")"

if [[ "${actual_version}" != "${version_id}" ||
      "${actual_protocols}" != "${expected_protocols}" ||
      "${actual_points}" != "${expected_points}" ||
      "${actual_flows}" != "${expected_flows}" ||
      "${actual_outputs}" != "${expected_outputs}" ||
      "${actual_commands}" != "${expected_commands}" ]]; then
  echo "demo config verification failed" >&2
  jq -n \
    --arg expectedVersion "${version_id}" \
    --arg actualVersion "${actual_version}" \
    --argjson expectedProtocols "${expected_protocols}" \
    --argjson actualProtocols "${actual_protocols}" \
    --argjson expectedPoints "${expected_points}" \
    --argjson actualPoints "${actual_points}" \
    --argjson expectedFlows "${expected_flows}" \
    --argjson actualFlows "${actual_flows}" \
    --argjson expectedOutputs "${expected_outputs}" \
    --argjson actualOutputs "${actual_outputs}" \
    --argjson expectedCommands "${expected_commands}" \
    --argjson actualCommands "${actual_commands}" \
    '{expected:{version:$expectedVersion,protocols:$expectedProtocols,points:$expectedPoints,flows:$expectedFlows,outputs:$expectedOutputs,commands:$expectedCommands},actual:{version:$actualVersion,protocols:$actualProtocols,points:$actualPoints,flows:$actualFlows,outputs:$actualOutputs,commands:$actualCommands}}' >&2
  exit 1
fi

jq -n \
  --arg project "${project_id}" \
  --arg product "${product_id}" \
  --arg version "${version_id}" \
  --arg edge "${edge_id}" \
  --argjson protocols "${actual_protocols}" \
  --argjson points "${actual_points}" \
  --argjson flows "${actual_flows}" \
  --argjson outputs "${actual_outputs}" \
  --argjson commands "${actual_commands}" \
  '{status:"ready", project:$project, product:$product, version:$version, edge:$edge, protocolConnections:$protocols, pointMappings:$points, dataFlows:$flows, mqttOutputs:$outputs, commandFlows:$commands}'
