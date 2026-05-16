#!/usr/bin/env bash
# adapter-debug-dashboard lifecycle helper.
#
# Usage:
#   scripts/serve.sh start    [--build|--no-build] [--open]
#   scripts/serve.sh stop
#   scripts/serve.sh restart  [--build|--no-build] [--open]
#   scripts/serve.sh status
#   scripts/serve.sh logs
#
# `start` auto-builds the Vite frontend, then launches
# `cargo run -p adapter-debug-dashboard` in the background and waits until
# `/api/health` answers. Logs land in `target/dashboard.log`; the
# foreground process id lands in `target/dashboard.pid`.
#
# `stop` finds whatever is bound to the port (default 3000, overridable
# via `WEB_SERVER_PORT`) and TERMs it.
#
# `--build` forces a rebuild even if `dist/` already exists; `--no-build`
# skips the rebuild step entirely; default (`auto`) only builds when the
# frontend's `dist/` is missing.

set -euo pipefail

cd "$(dirname "$0")/.."

PORT="${WEB_SERVER_PORT:-8080}"
LOG_FILE="target/dashboard.log"
PID_FILE="target/dashboard.pid"
URL="http://localhost:${PORT}"

mkdir -p target

cmd="${1:-}"
shift || true

build="auto"
open_after=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --build)    build="yes" ;;
    --no-build) build="no" ;;
    --open)     open_after=1 ;;
    *)
      echo "unknown arg: $1" >&2
      exit 1
      ;;
  esac
  shift
done

is_alive() {
  curl -sSf -o /dev/null --max-time 1 "${URL}/api/health" 2>/dev/null
}

stop_server() {
  local pids
  pids="$(lsof -ti ":${PORT}" 2>/dev/null || true)"
  if [[ -z "${pids}" ]]; then
    echo "no server on :${PORT}"
    rm -f "${PID_FILE}"
    return
  fi

  echo "stopping server (pids: ${pids})"
  # shellcheck disable=SC2086
  kill ${pids} 2>/dev/null || true
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    sleep 0.3
    pids="$(lsof -ti ":${PORT}" 2>/dev/null || true)"
    [[ -z "${pids}" ]] && break
  done
  if [[ -n "${pids}" ]]; then
    echo "force kill"
    # shellcheck disable=SC2086
    kill -9 ${pids} 2>/dev/null || true
  fi
  rm -f "${PID_FILE}"
  echo "stopped"
}

should_build() {
  case "${build}" in
    yes) return 0 ;;
    no)  return 1 ;;
    auto)
      [[ ! -d adapter-debug-dashboard/frontend/dist ]] && return 0
      return 1
      ;;
  esac
}

build_assets() {
  echo "→ building main frontend"
  (cd adapter-debug-dashboard/frontend && npm install --silent --no-fund --no-audit && npx vite build)
}

start_server() {
  if is_alive; then
    echo "already running on ${URL}"
    [[ "${open_after}" -eq 1 ]] && open "${URL}/"
    return 0
  fi

  if should_build; then
    build_assets
  fi

  echo "→ starting adapter-debug-dashboard on :${PORT}"
  : > "${LOG_FILE}"
  WEB_SERVER_ADDR="0.0.0.0:${PORT}" \
  RUST_LOG="${RUST_LOG:-adapter_debug_dashboard=info,tower_http=info}" \
    nohup cargo run -p adapter-debug-dashboard --quiet >>"${LOG_FILE}" 2>&1 &
  echo $! > "${PID_FILE}"

  echo -n "waiting for /api/health"
  for _ in $(seq 1 120); do
    if is_alive; then
      echo " ✓"
      echo
      echo "main: ${URL}/"
      echo "logs: tail -f ${LOG_FILE}"
      [[ "${open_after}" -eq 1 ]] && open "${URL}/"
      return 0
    fi
    echo -n "."
    sleep 0.5
  done
  echo " ✗ timeout"
  echo "--- last 30 log lines ---"
  tail -30 "${LOG_FILE}"
  return 1
}

case "${cmd}" in
  start)   start_server ;;
  stop)    stop_server ;;
  restart)
    stop_server
    sleep 0.5
    start_server
    ;;
  status)
    if is_alive; then
      echo "up (${URL})"
      [[ -f "${PID_FILE}" ]] && echo "pid: $(cat "${PID_FILE}")"
    else
      echo "down"
    fi
    ;;
  logs)
    if [[ ! -f "${LOG_FILE}" ]]; then
      echo "no log file (${LOG_FILE}). is the server running?"
      exit 1
    fi
    tail -f "${LOG_FILE}"
    ;;
  *)
    cat <<USAGE >&2
usage: $0 <command> [flags]

commands:
  start [--build|--no-build] [--open]   build assets and run adapter-debug-dashboard
  stop                                  kill server on :${PORT}
  restart [--build|--no-build] [--open] stop + start
  status                                check /api/health
  logs                                  tail target/dashboard.log

env:
  WEB_SERVER_PORT   override port (default 3000)
  RUST_LOG          override log filter (default adapter_debug_dashboard=info,tower_http=info)
USAGE
    exit 1
    ;;
esac
