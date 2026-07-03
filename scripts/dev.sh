#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

MODE="${1:-all}"
export AI_PROVIDER="${AI_PROVIDER:-mock}"
export MARKET_DATA_PROVIDER="${MARKET_DATA_PROVIDER:-mock}"

BACKEND_PORT_START="${BACKEND_PORT:-8080}"
FRONTEND_PORT_START="${FRONTEND_PORT:-5173}"
BACKEND_PORT="$BACKEND_PORT_START"
FRONTEND_PORT="$FRONTEND_PORT_START"

pids=()
names=()

usage() {
  cat <<'USAGE'
Usage: scripts/dev.sh [all|backend|frontend]

Environment:
  AI_PROVIDER             Defaults to mock
  MARKET_DATA_PROVIDER    Defaults to mock
  BACKEND_PORT            Preferred backend port, defaults to 8080
  FRONTEND_PORT           Preferred frontend port, defaults to 5173
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

port_is_available() {
  local port="$1"

  if ! command -v lsof >/dev/null 2>&1; then
    return 0
  fi

  ! lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1
}

find_available_port() {
  local preferred_port="$1"
  local port="$preferred_port"

  while ! port_is_available "$port"; do
    port=$((port + 1))
  done

  printf '%s\n' "$port"
}

choose_ports() {
  BACKEND_PORT="$(find_available_port "$BACKEND_PORT_START")"
  FRONTEND_PORT="$(find_available_port "$FRONTEND_PORT_START")"

  while [[ "$FRONTEND_PORT" == "$BACKEND_PORT" ]]; do
    FRONTEND_PORT="$(find_available_port "$((FRONTEND_PORT + 1))")"
  done

  if [[ "$BACKEND_PORT" != "$BACKEND_PORT_START" ]]; then
    echo "Backend port $BACKEND_PORT_START is unavailable; using $BACKEND_PORT instead."
  fi

  if [[ "$FRONTEND_PORT" != "$FRONTEND_PORT_START" ]]; then
    echo "Frontend port $FRONTEND_PORT_START is unavailable; using $FRONTEND_PORT instead."
  fi
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
  echo "Backend:  http://127.0.0.1:$BACKEND_PORT"
  echo "Frontend: http://127.0.0.1:$FRONTEND_PORT"
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
    choose_ports
    ensure_frontend_deps
    start_service "backend" env BIND_ADDR="127.0.0.1:$BACKEND_PORT" cargo run -p prudentia-backend
    start_service "frontend" env VITE_API_BASE_URL="http://127.0.0.1:$BACKEND_PORT" npm --prefix frontend run dev -- --port "$FRONTEND_PORT" --strictPort
    wait_for_services
    ;;
  backend)
    BACKEND_PORT="$(find_available_port "$BACKEND_PORT_START")"
    if [[ "$BACKEND_PORT" != "$BACKEND_PORT_START" ]]; then
      echo "Backend port $BACKEND_PORT_START is unavailable; using $BACKEND_PORT instead."
    fi
    exec env BIND_ADDR="127.0.0.1:$BACKEND_PORT" cargo run -p prudentia-backend
    ;;
  frontend)
    FRONTEND_PORT="$(find_available_port "$FRONTEND_PORT_START")"
    if [[ "$FRONTEND_PORT" != "$FRONTEND_PORT_START" ]]; then
      echo "Frontend port $FRONTEND_PORT_START is unavailable; using $FRONTEND_PORT instead."
    fi
    ensure_frontend_deps
    exec npm --prefix frontend run dev -- --port "$FRONTEND_PORT" --strictPort
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
