#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

MODE="${1:-all}"
export AI_PROVIDER="${AI_PROVIDER:-mock}"
export MARKET_DATA_PROVIDER="${MARKET_DATA_PROVIDER:-mock}"

pids=()
names=()

usage() {
  cat <<'USAGE'
Usage: scripts/dev.sh [all|backend|frontend]

Environment:
  AI_PROVIDER             Defaults to mock
  MARKET_DATA_PROVIDER    Defaults to mock
  SKIP_NPM_INSTALL=1      Do not install frontend dependencies automatically
USAGE
}

cleanup() {
  local status=$?
  trap - EXIT INT TERM

  if ((${#pids[@]} > 0)); then
    echo
    echo "Stopping services..."
    for pid in "${pids[@]}"; do
      kill "$pid" 2>/dev/null || true
    done
    for pid in "${pids[@]}"; do
      wait "$pid" 2>/dev/null || true
    done
  fi

  exit "$status"
}

start_service() {
  local name="$1"
  shift

  echo "Starting $name..."
  "$@" &
  pids+=("$!")
  names+=("$name")
}

ensure_frontend_deps() {
  if [[ "${SKIP_NPM_INSTALL:-0}" == "1" ]]; then
    return
  fi

  if [[ ! -d frontend/node_modules ]]; then
    echo "Installing frontend dependencies..."
    npm install --prefix frontend
  fi
}

wait_for_services() {
  trap cleanup EXIT INT TERM

  echo
  echo "Backend:  http://127.0.0.1:8080"
  echo "Frontend: http://127.0.0.1:5173"
  echo "Press Ctrl+C to stop."

  while :; do
    for i in "${!pids[@]}"; do
      local pid="${pids[$i]}"
      if ! kill -0 "$pid" 2>/dev/null; then
        local status=0
        wait "$pid" || status=$?
        echo "${names[$i]} exited with status $status"
        exit "$status"
      fi
    done
    sleep 1
  done
}

case "$MODE" in
  all | dev | start)
    ensure_frontend_deps
    start_service "backend" cargo run -p prudentia-backend
    start_service "frontend" npm --prefix frontend run dev
    wait_for_services
    ;;
  backend)
    exec cargo run -p prudentia-backend
    ;;
  frontend)
    ensure_frontend_deps
    exec npm --prefix frontend run dev
    ;;
  -h | --help | help)
    usage
    ;;
  *)
    echo "Unknown mode: $MODE" >&2
    usage >&2
    exit 2
    ;;
esac
