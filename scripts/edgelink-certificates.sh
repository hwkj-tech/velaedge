#!/usr/bin/env bash
set -euo pipefail

STAGE_TO_CLEAN=""
cleanup() {
  [[ -z "$STAGE_TO_CLEAN" ]] || rm -rf "$STAGE_TO_CLEAN"
}
trap cleanup EXIT

usage() {
  cat <<'EOF'
Usage:
  edgelink-certificates.sh check CERT KEY CA [MIN_VALID_DAYS]
  edgelink-certificates.sh install CERT KEY CA TARGET_DIR [MIN_VALID_DAYS]
  edgelink-certificates.sh activate TARGET_DIR RELEASE_ID [MIN_VALID_DAYS]
  edgelink-certificates.sh status TARGET_DIR [MIN_VALID_DAYS]
  edgelink-certificates.sh list TARGET_DIR

The install command validates the certificate chain, private-key match and remaining
validity before copying material into a versioned release and atomically switching
TARGET_DIR/current. Use activate to roll back to any retained release.
EOF
}

require_commands() {
  for command in openssl jq; do
    command -v "$command" >/dev/null || {
      echo "missing required command: $command" >&2
      exit 2
    }
  done
}

positive_days() {
  local value="${1:-30}"
  [[ "$value" =~ ^[0-9]+$ ]] || {
    echo "minimum validity must be a non-negative number of days: $value" >&2
    exit 2
  }
  printf '%s' "$value"
}

absolute_dir() {
  local path="$1"
  mkdir -p "$path"
  (cd "$path" && pwd -P)
}

certificate_fingerprint() {
  openssl x509 -in "$1" -noout -fingerprint -sha256 \
    | sed 's/^sha256 Fingerprint=//;s/://g' \
    | tr '[:upper:]' '[:lower:]'
}

public_key_digest_from_certificate() {
  openssl x509 -in "$1" -pubkey -noout \
    | openssl pkey -pubin -outform DER 2>/dev/null \
    | openssl dgst -sha256 -r \
    | awk '{print $1}'
}

public_key_digest_from_key() {
  openssl pkey -in "$1" -pubout -outform DER 2>/dev/null \
    | openssl dgst -sha256 -r \
    | awk '{print $1}'
}

check_expiry() {
  local cert="$1"
  local label="$2"
  local min_days="$3"
  local min_seconds=$((min_days * 86400))
  local checkend_output
  checkend_output="$(openssl x509 -in "$cert" -checkend "$min_seconds" -noout 2>&1)" || true
  [[ "$checkend_output" == *"will not expire"* ]] || {
    echo "$label expires within ${min_days} days" >&2
    return 1
  }
}

check_material() {
  local cert="$1"
  local key="$2"
  local ca="$3"
  local min_days
  min_days="$(positive_days "${4:-30}")"

  for path in "$cert" "$key" "$ca"; do
    [[ -f "$path" ]] || {
      echo "certificate material does not exist: $path" >&2
      return 1
    }
  done

  openssl x509 -in "$cert" -noout >/dev/null
  openssl pkey -in "$key" -noout >/dev/null 2>&1
  openssl x509 -in "$ca" -noout >/dev/null
  local ca_text
  ca_text="$(openssl x509 -in "$ca" -noout -text)"
  [[ "$ca_text" == *"CA:TRUE"* ]] || {
    echo "client trust certificate is not a CA" >&2
    return 1
  }
  openssl verify -CAfile "$ca" "$cert" >/dev/null

  local cert_key_digest key_digest
  cert_key_digest="$(public_key_digest_from_certificate "$cert")"
  key_digest="$(public_key_digest_from_key "$key")"
  [[ "$cert_key_digest" == "$key_digest" ]] || {
    echo "certificate and private key do not match" >&2
    return 1
  }

  check_expiry "$cert" "certificate" "$min_days"
  check_expiry "$ca" "client CA certificate" "$min_days"

  local fingerprint subject issuer not_before not_after serial
  fingerprint="$(certificate_fingerprint "$cert")"
  subject="$(openssl x509 -in "$cert" -noout -subject | sed 's/^subject=//')"
  issuer="$(openssl x509 -in "$cert" -noout -issuer | sed 's/^issuer=//')"
  not_before="$(openssl x509 -in "$cert" -noout -startdate | sed 's/^notBefore=//')"
  not_after="$(openssl x509 -in "$cert" -noout -enddate | sed 's/^notAfter=//')"
  serial="$(openssl x509 -in "$cert" -noout -serial | sed 's/^serial=//')"

  jq -n \
    --arg status valid \
    --arg fingerprint "$fingerprint" \
    --arg subject "$subject" \
    --arg issuer "$issuer" \
    --arg notBefore "$not_before" \
    --arg notAfter "$not_after" \
    --arg serial "$serial" \
    --argjson minimumValidDays "$min_days" \
    '{status:$status,fingerprint:$fingerprint,subject:$subject,issuer:$issuer,notBefore:$notBefore,notAfter:$notAfter,serial:$serial,minimumValidDays:$minimumValidDays}'
}

activate_release() {
  local target="$1"
  local release_id="$2"
  local min_days="${3:-30}"
  local release_dir="${target}/releases/${release_id}"

  [[ "$release_id" != */* && "$release_id" != .* ]] || {
    echo "invalid release id: $release_id" >&2
    return 1
  }
  [[ -d "$release_dir" ]] || {
    echo "certificate release does not exist: $release_id" >&2
    return 1
  }
  check_material \
    "${release_dir}/server.pem" \
    "${release_dir}/server-key.pem" \
    "${release_dir}/runtime-ca.pem" \
    "$min_days" >/dev/null

  local next_link="${target}/.current.$$"
  rm -f "$next_link"
  ln -s "releases/${release_id}" "$next_link"
  if mv -fh "$next_link" "${target}/current" 2>/dev/null; then
    :
  elif mv -fT "$next_link" "${target}/current" 2>/dev/null; then
    :
  else
    rm -f "$next_link"
    echo "failed to atomically activate certificate release: $release_id" >&2
    return 1
  fi
  [[ "$(readlink "${target}/current")" == "releases/${release_id}" ]] || {
    echo "certificate activation verification failed: $release_id" >&2
    return 1
  }
  jq -n --arg status active --arg releaseId "$release_id" \
    --arg current "${target}/current" \
    '{status:$status,releaseId:$releaseId,current:$current}'
}

install_release() {
  local cert="$1"
  local key="$2"
  local ca="$3"
  local target
  target="$(absolute_dir "$4")"
  local min_days="${5:-30}"
  local validation fingerprint release_id releases_dir stage release_dir

  validation="$(check_material "$cert" "$key" "$ca" "$min_days")"
  fingerprint="$(printf '%s' "$validation" | jq -er '.fingerprint')"
  releases_dir="${target}/releases"
  mkdir -p "$releases_dir"
  release_id="$(date -u '+%Y%m%dT%H%M%SZ')-${fingerprint:0:16}"
  release_dir="${releases_dir}/${release_id}"
  [[ ! -e "$release_dir" ]] || {
    echo "certificate release already exists: $release_id" >&2
    return 1
  }

  stage="$(mktemp -d "${releases_dir}/.staging.XXXXXX")"
  STAGE_TO_CLEAN="$stage"
  cp "$cert" "${stage}/server.pem"
  cp "$key" "${stage}/server-key.pem"
  cp "$ca" "${stage}/runtime-ca.pem"
  chmod 0644 "${stage}/server.pem" "${stage}/runtime-ca.pem"
  chmod 0600 "${stage}/server-key.pem"
  printf '%s\n' "$validation" >"${stage}/metadata.json"
  chmod 0644 "${stage}/metadata.json"
  mv "$stage" "$release_dir"
  stage=""
  STAGE_TO_CLEAN=""

  activate_release "$target" "$release_id" "$min_days" >/dev/null
  jq -n \
    --arg status installed \
    --arg releaseId "$release_id" \
    --arg current "${target}/current" \
    --argjson certificate "$validation" \
    '{status:$status,releaseId:$releaseId,current:$current,certificate:$certificate}'
}

status_release() {
  local target="$1"
  local min_days="${2:-30}"
  [[ -L "${target}/current" ]] || {
    echo "active certificate link does not exist: ${target}/current" >&2
    return 1
  }
  local release_path release_id validation
  release_path="$(readlink "${target}/current")"
  release_id="$(basename "$release_path")"
  validation="$(check_material \
    "${target}/current/server.pem" \
    "${target}/current/server-key.pem" \
    "${target}/current/runtime-ca.pem" \
    "$min_days")"
  jq -n --arg status active --arg releaseId "$release_id" \
    --argjson certificate "$validation" \
    '{status:$status,releaseId:$releaseId,certificate:$certificate}'
}

list_releases() {
  local target="$1"
  local active=""
  [[ -L "${target}/current" ]] && active="$(basename "$(readlink "${target}/current")")"
  local releases='[]'
  if [[ -d "${target}/releases" ]]; then
    while IFS= read -r release_id; do
      [[ -n "$release_id" ]] || continue
      releases="$(printf '%s' "$releases" | jq \
        --arg id "$release_id" --arg active "$active" \
        '. + [{releaseId:$id,active:($id == $active)}]')"
    done < <(find "${target}/releases" -mindepth 1 -maxdepth 1 -type d ! -name '.staging.*' -exec basename {} \; | sort -r)
  fi
  printf '%s\n' "$releases"
}

require_commands
command_name="${1:-}"
case "$command_name" in
  check)
    [[ $# -ge 4 && $# -le 5 ]] || { usage >&2; exit 2; }
    check_material "$2" "$3" "$4" "${5:-30}"
    ;;
  install)
    [[ $# -ge 5 && $# -le 6 ]] || { usage >&2; exit 2; }
    install_release "$2" "$3" "$4" "$5" "${6:-30}"
    ;;
  activate)
    [[ $# -ge 3 && $# -le 4 ]] || { usage >&2; exit 2; }
    activate_release "$(absolute_dir "$2")" "$3" "${4:-30}"
    ;;
  status)
    [[ $# -ge 2 && $# -le 3 ]] || { usage >&2; exit 2; }
    status_release "$(absolute_dir "$2")" "${3:-30}"
    ;;
  list)
    [[ $# -eq 2 ]] || { usage >&2; exit 2; }
    list_releases "$(absolute_dir "$2")"
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
