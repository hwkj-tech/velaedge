#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CERT_TOOL="${ROOT_DIR}/scripts/edgelink-certificates.sh"
WORK_DIR="${CERTIFICATE_ACCEPTANCE_WORK_DIR:-${ROOT_DIR}/target/certificate-acceptance-$$}"
REPORT_PATH="${CERTIFICATE_ACCEPTANCE_REPORT:-${WORK_DIR}/report.json}"
TARGET_DIR="${WORK_DIR}/installed"

for command in openssl jq; do
  command -v "$command" >/dev/null || {
    echo "missing required command: $command" >&2
    exit 2
  }
done

mkdir -p "$WORK_DIR"
chmod 0700 "$WORK_DIR"

openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "${WORK_DIR}/ca-key.pem" >/dev/null 2>&1
openssl req -x509 -new -key "${WORK_DIR}/ca-key.pem" -sha256 -days 365 \
  -subj '/CN=EdgeOps Lifecycle Acceptance CA' -out "${WORK_DIR}/ca.pem"
printf 'subjectAltName=DNS:localhost,IP:127.0.0.1\nextendedKeyUsage=serverAuth\n' >"${WORK_DIR}/server.ext"

issue_server() {
  local name="$1"
  local days="$2"
  openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 \
    -out "${WORK_DIR}/${name}-key.pem" >/dev/null 2>&1
  openssl req -new -key "${WORK_DIR}/${name}-key.pem" -subj '/CN=localhost' \
    -out "${WORK_DIR}/${name}.csr"
  openssl x509 -req -in "${WORK_DIR}/${name}.csr" \
    -CA "${WORK_DIR}/ca.pem" -CAkey "${WORK_DIR}/ca-key.pem" -CAcreateserial \
    -days "$days" -sha256 -extfile "${WORK_DIR}/server.ext" \
    -out "${WORK_DIR}/${name}.pem" >/dev/null 2>&1
}

issue_server server-v1 120
sleep 1
issue_server server-v2 120
issue_server server-short 1

FIRST_INSTALL="$($CERT_TOOL install \
  "${WORK_DIR}/server-v1.pem" "${WORK_DIR}/server-v1-key.pem" \
  "${WORK_DIR}/ca.pem" "$TARGET_DIR" 30)"
FIRST_RELEASE="$(printf '%s' "$FIRST_INSTALL" | jq -er '.releaseId')"

SECOND_INSTALL="$($CERT_TOOL install \
  "${WORK_DIR}/server-v2.pem" "${WORK_DIR}/server-v2-key.pem" \
  "${WORK_DIR}/ca.pem" "$TARGET_DIR" 30)"
SECOND_RELEASE="$(printf '%s' "$SECOND_INSTALL" | jq -er '.releaseId')"
[[ "$FIRST_RELEASE" != "$SECOND_RELEASE" ]]

ACTIVE_AFTER_ROTATION="$($CERT_TOOL status "$TARGET_DIR" 30)"
printf '%s' "$ACTIVE_AFTER_ROTATION" | jq -e \
  --arg release "$SECOND_RELEASE" '.status == "active" and .releaseId == $release' >/dev/null

ROLLBACK="$($CERT_TOOL activate "$TARGET_DIR" "$FIRST_RELEASE" 30)"
printf '%s' "$ROLLBACK" | jq -e \
  --arg release "$FIRST_RELEASE" '.status == "active" and .releaseId == $release' >/dev/null

if $CERT_TOOL check \
  "${WORK_DIR}/server-v1.pem" "${WORK_DIR}/server-v2-key.pem" \
  "${WORK_DIR}/ca.pem" 30 >"${WORK_DIR}/mismatch.out" 2>&1; then
  echo "certificate tool accepted a mismatched private key" >&2
  exit 1
fi
grep -q 'do not match' "${WORK_DIR}/mismatch.out"

if $CERT_TOOL check \
  "${WORK_DIR}/server-short.pem" "${WORK_DIR}/server-short-key.pem" \
  "${WORK_DIR}/ca.pem" 30 >"${WORK_DIR}/expiry.out" 2>&1; then
  echo "certificate tool accepted a certificate below the validity threshold" >&2
  exit 1
fi
grep -q 'expires within 30 days' "${WORK_DIR}/expiry.out"

RELEASES="$($CERT_TOOL list "$TARGET_DIR")"
printf '%s' "$RELEASES" | jq -e \
  --arg first "$FIRST_RELEASE" --arg second "$SECOND_RELEASE" \
  'length == 2 and any(.[]; .releaseId == $first and .active == true) and any(.[]; .releaseId == $second and .active == false)' >/dev/null

jq -n \
  --arg target "$TARGET_DIR" \
  --arg firstRelease "$FIRST_RELEASE" \
  --arg secondRelease "$SECOND_RELEASE" \
  --argjson active "$($CERT_TOOL status "$TARGET_DIR" 30)" \
  --argjson releases "$RELEASES" \
  '{
    status:"passed",
    target:$target,
    firstRelease:$firstRelease,
    secondRelease:$secondRelease,
    activeAfterRollback:$active,
    releases:$releases,
    mismatchedKeyRejected:true,
    expiringCertificateRejected:true,
    atomicCurrentLink:true
  }' | tee "$REPORT_PATH"

echo "certificate lifecycle acceptance evidence: $WORK_DIR"
