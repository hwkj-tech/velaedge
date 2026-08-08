#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CLOUD_BIN="${CLOUD_RECOVERY_CLOUD_BIN:-${ROOT_DIR}/target/debug/cloud-api}"
HTTP_PORT="${CLOUD_RECOVERY_HTTP_PORT:-18085}"
GATEWAY_PORT="${CLOUD_RECOVERY_GATEWAY_PORT:-19085}"
WORK_DIR="${CLOUD_RECOVERY_WORK_DIR:-${ROOT_DIR}/target/cloud-recovery-acceptance-$$}"
REPORT_PATH="${CLOUD_RECOVERY_REPORT:-${WORK_DIR}/report.json}"
CLOUD_PID=""

for command in curl jq nc sqlite3; do
  command -v "$command" >/dev/null || {
    echo "missing required command: $command" >&2
    exit 2
  }
done

for port in "$HTTP_PORT" "$GATEWAY_PORT"; do
  if nc -z 127.0.0.1 "$port" >/dev/null 2>&1; then
    echo "acceptance port is already in use: $port" >&2
    exit 2
  fi
done

mkdir -p "$WORK_DIR" "$(dirname "$REPORT_PATH")"
RUN_DIR="$(mktemp -d "${WORK_DIR}/run.XXXXXX")"
DATABASE_PATH="${RUN_DIR}/cloud.sqlite"
BACKUP_PATH="${RUN_DIR}/cloud.backup.sqlite"
cargo build -p cloud-api >/dev/null

cleanup() {
  if [[ -n "$CLOUD_PID" ]]; then
    kill "$CLOUD_PID" 2>/dev/null || true
    wait "$CLOUD_PID" 2>/dev/null || true
  fi
}
on_exit() {
  status=$?
  if [[ "$status" -ne 0 ]]; then
    echo "cloud recovery acceptance failed; evidence retained at: $WORK_DIR" >&2
  fi
  cleanup
  return "$status"
}
trap on_exit EXIT
trap 'exit 130' INT TERM

start_cloud() {
  local log_path="$1"
  EDGEOPS_CLOUD_DB="sqlite://${DATABASE_PATH}?mode=rwc" \
  EDGEOPS_HTTP_ADDR="127.0.0.1:${HTTP_PORT}" \
  EDGEOPS_GATEWAY_ADDR="127.0.0.1:${GATEWAY_PORT}" \
  EDGEOPS_BOOTSTRAP_MODE=demo \
  RUST_LOG=info \
  "$CLOUD_BIN" >"$log_path" 2>&1 &
  CLOUD_PID=$!
}

wait_until_ready() {
  local log_path="$1"
  for _ in $(seq 1 60); do
    if curl -fsS --max-time 1 "http://127.0.0.1:${HTTP_PORT}/health/ready"; then
      return 0
    fi
    if ! kill -0 "$CLOUD_PID" 2>/dev/null; then
      echo "cloud-api exited before readiness" >&2
      tail -80 "$log_path" >&2
      return 1
    fi
    sleep 1
  done
  echo "cloud-api readiness timed out" >&2
  tail -80 "$log_path" >&2
  return 1
}

stop_cloud() {
  local log_path="$1"
  kill -TERM "$CLOUD_PID"
  wait "$CLOUD_PID"
  CLOUD_PID=""
  grep -q "shutdown signal received" "$log_path"
  grep -q "cloud agent stopped" "$log_path"
}

FIRST_LOG="${RUN_DIR}/cloud-before-restore.log"
start_cloud "$FIRST_LOG"
READY_BEFORE="$(wait_until_ready "$FIRST_LOG")"
sqlite3 "$DATABASE_PATH" \
  "CREATE TABLE recovery_acceptance_marker(value TEXT NOT NULL); INSERT INTO recovery_acceptance_marker(value) VALUES ('before-backup');"
"${ROOT_DIR}/scripts/cloud-state.sh" backup "$DATABASE_PATH" "$BACKUP_PATH"
stop_cloud "$FIRST_LOG"

sqlite3 "$DATABASE_PATH" "UPDATE recovery_acceptance_marker SET value = 'corrupted-after-backup';"
"${ROOT_DIR}/scripts/cloud-state.sh" restore "$BACKUP_PATH" "$DATABASE_PATH"
RESTORED_MARKER="$(sqlite3 "$DATABASE_PATH" 'SELECT value FROM recovery_acceptance_marker LIMIT 1;')"
[[ "$RESTORED_MARKER" == "before-backup" ]] || {
  echo "restored marker mismatch: $RESTORED_MARKER" >&2
  exit 1
}

SECOND_LOG="${RUN_DIR}/cloud-after-restore.log"
start_cloud "$SECOND_LOG"
READY_AFTER="$(wait_until_ready "$SECOND_LOG")"
PROJECTS="$(curl -fsS --max-time 5 "http://127.0.0.1:${HTTP_PORT}/api/projects")"
printf '%s' "$PROJECTS" | jq -e 'any(.[]; .projectId == "demo-plant")' >/dev/null
stop_cloud "$SECOND_LOG"

jq -n \
  --arg runDirectory "$RUN_DIR" \
  --arg database "$DATABASE_PATH" \
  --arg backup "$BACKUP_PATH" \
  --arg restoredMarker "$RESTORED_MARKER" \
  --argjson readinessBefore "$READY_BEFORE" \
  --argjson readinessAfter "$READY_AFTER" \
  --argjson projects "$PROJECTS" \
  '{
    runDirectory: $runDirectory,
    database: $database,
    backup: $backup,
    restoredMarker: $restoredMarker,
    gracefulShutdown: true,
    readinessBefore: $readinessBefore,
    readinessAfter: $readinessAfter,
    projects: $projects
  }' | tee "$REPORT_PATH"

echo "cloud recovery acceptance evidence: $WORK_DIR"
