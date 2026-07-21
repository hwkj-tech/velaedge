#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/cloud-state.sh backup  <database.sqlite> <backup.sqlite>
  scripts/cloud-state.sh verify  <database.sqlite>
  scripts/cloud-state.sh restore <backup.sqlite> <database.sqlite>

The cloud-api process must be stopped before restore. Backup uses SQLite's
online backup command and is safe while cloud-api is running.
EOF
}

command -v sqlite3 >/dev/null || {
  echo "sqlite3 is required" >&2
  exit 2
}

ACTION="${1:-}"
SOURCE="${2:-}"
TARGET="${3:-}"

verify_database() {
  local database="$1"
  [[ -f "$database" ]] || {
    echo "database does not exist: $database" >&2
    return 1
  }
  local result
  result="$(sqlite3 "$database" 'PRAGMA quick_check;')"
  [[ "$result" == "ok" ]] || {
    echo "database integrity check failed: $result" >&2
    return 1
  }
}

case "$ACTION" in
  backup)
    [[ -n "$SOURCE" && -n "$TARGET" ]] || { usage >&2; exit 2; }
    [[ -f "$SOURCE" ]] || { echo "database does not exist: $SOURCE" >&2; exit 1; }
    mkdir -p "$(dirname "$TARGET")"
    [[ ! -e "$TARGET" ]] || { echo "backup already exists: $TARGET" >&2; exit 1; }
    sqlite3 "$SOURCE" ".backup '$TARGET'"
    verify_database "$TARGET"
    echo "verified backup: $TARGET"
    ;;
  verify)
    [[ -n "$SOURCE" && -z "$TARGET" ]] || { usage >&2; exit 2; }
    verify_database "$SOURCE"
    echo "database integrity: ok"
    ;;
  restore)
    [[ -n "$SOURCE" && -n "$TARGET" ]] || { usage >&2; exit 2; }
    verify_database "$SOURCE"
    mkdir -p "$(dirname "$TARGET")"
    TEMP_TARGET="${TARGET}.restore.$$"
    cleanup() { rm -f "$TEMP_TARGET"; }
    trap cleanup EXIT
    sqlite3 "$SOURCE" ".backup '$TEMP_TARGET'"
    verify_database "$TEMP_TARGET"
    rm -f "${TARGET}-wal" "${TARGET}-shm"
    mv "$TEMP_TARGET" "$TARGET"
    trap - EXIT
    echo "verified restore: $TARGET"
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
