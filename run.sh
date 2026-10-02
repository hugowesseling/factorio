#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_DIR="$ROOT/backend"
FRONTEND_DIR="$ROOT/frontend"
RUNDIR="$ROOT/.run"

SEED=7
VIEW_RADIUS=2
TICKS=1800
PORT=5173
ADDR="127.0.0.1:9000"
SERVE=1
PROFILE_DIR="debug"
RELEASE_FLAG=""
RUN_BACKEND=1
RUN_FRONTEND=1

usage() {
  cat <<'EOF'
Usage: ./run.sh [options]

Starts the Rust backend and the Vite dev server together. Ctrl-C stops both.

Options:
  --seed N           backend world seed                (default 7)
  --view-radius N    chunks streamed around the player (default 2)
  --ticks N          backend ticks before it exits     (default 1800, batch only)
  --serve            run the WebSocket listener        (default)
  --batch            run the headless batch sim instead of the listener
  --addr HOST:PORT   listener address                  (default 127.0.0.1:9000)
  --port N           dev server port                   (default 5173)
  --release          build the backend with optimizations
  --frontend-only    start only the dev server
  --backend-only     start only the backend
  -h, --help         show this help

The backend listens for WebSocket clients at ws://ADDR/ws and the browser client
connects automatically and goes live. Use --batch for the headless batch
simulation instead: it streams chunks, prints a status line every 600 ticks and
writes save.fsv into .run/, while the browser stays in its offline demo world.
EOF
}

die() {
  printf 'run.sh: %s\n' "$*" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || die "$1 is required but was not found on PATH"
}

resolve_cargo() {
  if command -v cargo >/dev/null 2>&1; then
    CARGO="cargo"
    return 0
  fi
  local candidate
  for candidate in "${CARGO_HOME:-}/bin/cargo" "${HOME:-}/.cargo/bin/cargo" "${HOME:-}/.cargo/bin/cargo.exe"; do
    if [ -x "$candidate" ]; then
      CARGO="$candidate"
      PATH="$(dirname "$candidate"):$PATH"
      export PATH
      return 0
    fi
  done
  return 1
}

note() {
  printf '%s\n' "${DIM}run.sh:${OFF} $*"
}

warn() {
  printf '%s\n' "${YELLOW}run.sh:$OFF $*" >&2
}

fail() {
  warn "$*"
  exit 1
}

DIM=""
YELLOW=""
BACKEND_TAG=""
FRONTEND_TAG=""
OFF=""
if [ -t 1 ]; then
  DIM=$'\033[2m'
  YELLOW=$'\033[1;33m'
  BACKEND_TAG=$'\033[1;34m[backend]'
  FRONTEND_TAG=$'\033[1;32m[frontend]'
  OFF=$'\033[0m'
else
  BACKEND_TAG="[backend]"
  FRONTEND_TAG="[frontend]"
fi

while [ $# -gt 0 ]; do
  case "$1" in
    --seed)
      [ $# -ge 2 ] || die "--seed needs a value"
      SEED="$2"
      shift 2
      ;;
    --view-radius)
      [ $# -ge 2 ] || die "--view-radius needs a value"
      VIEW_RADIUS="$2"
      shift 2
      ;;
    --ticks)
      [ $# -ge 2 ] || die "--ticks needs a value"
      TICKS="$2"
      shift 2
      ;;
    --serve)
      SERVE=1
      shift
      ;;
    --batch)
      SERVE=0
      shift
      ;;
    --addr)
      [ $# -ge 2 ] || die "--addr needs a value"
      ADDR="$2"
      shift 2
      ;;
    --port)
      [ $# -ge 2 ] || die "--port needs a value"
      PORT="$2"
      shift 2
      ;;
    --release)
      PROFILE_DIR="release"
      RELEASE_FLAG="--release"
      shift
      ;;
    --frontend-only)
      RUN_BACKEND=0
      shift
      ;;
    --backend-only)
      RUN_FRONTEND=0
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      die "unknown option $1"
      ;;
  esac
done

check_number() {
  case "$1" in
    "" | *[!0-9]*) die "$2 must be a non-negative number (got '$1')" ;;
  esac
}

check_number "$SEED" "--seed"
check_number "$VIEW_RADIUS" "--view-radius"
check_number "$TICKS" "--ticks"
check_number "$PORT" "--port"
[ "$TICKS" -gt 0 ] || die "ticks must be greater than zero"
[ "$PORT" -gt 0 ] && [ "$PORT" -lt 65536 ] || die "port must be between 1 and 65535"
case "$ADDR" in
  "") die "--addr must not be empty" ;;
  *:*) ;;
  *) die "--addr must look like HOST:PORT (got '$ADDR')" ;;
esac

BACKEND_PGID=""
FRONTEND_PGID=""

stop_group() {
  local pgid="$1" signal="$2"
  [ -n "$pgid" ] || return 0
  kill "-$signal" -- "-$pgid" 2>/dev/null || true
}

cleanup() {
  trap - EXIT INT TERM
  stop_group "$FRONTEND_PGID" TERM
  stop_group "$BACKEND_PGID" TERM
  sleep 0.3
  stop_group "$FRONTEND_PGID" KILL
  stop_group "$BACKEND_PGID" KILL
}

group_running() {
  [ -n "$1" ] && kill -0 -- "-$1" 2>/dev/null
}

group_of() {
  ps -o pgid= -p "$1" | tr -d ' '
}

trap cleanup EXIT
trap 'cleanup; exit 130' INT
trap 'cleanup; exit 143' TERM

if [ "$RUN_BACKEND" = 1 ]; then
  if resolve_cargo; then
    note "building the backend ($PROFILE_DIR) with $CARGO…"
    "$CARGO" build --manifest-path "$BACKEND_DIR/Cargo.toml" $RELEASE_FLAG -p factorio-server
  else
    die "cargo was not found. Install Rust from https://rustup.rs, or put cargo on PATH, or start with 'source \"$HOME/.cargo/env\"'"
  fi
fi

if [ "$RUN_FRONTEND" = 1 ]; then
  need node
  need npm
  if [ ! -d "$FRONTEND_DIR/node_modules" ]; then
    note "installing frontend dependencies…"
    (cd "$FRONTEND_DIR" && npm install)
  fi
fi

if [ "$RUN_BACKEND" = 1 ]; then
  mkdir -p "$RUNDIR"
  if [ "$SERVE" = 1 ]; then
    BACKEND_ARGS=(--serve "$ADDR" "$SEED" "$VIEW_RADIUS")
  else
    BACKEND_ARGS=("$SEED" "$VIEW_RADIUS" "$TICKS")
  fi
  set -m
  (cd "$RUNDIR" && exec stdbuf -oL -eL "$BACKEND_DIR/target/$PROFILE_DIR/factorio-server" "${BACKEND_ARGS[@]}") 2>&1 |
    sed -u "s|^|${BACKEND_TAG}${OFF} |" &
  BACKEND_PGID="$(group_of "$!")"
  set +m
fi

if [ "$RUN_FRONTEND" = 1 ]; then
  set -m
  (cd "$FRONTEND_DIR" && exec stdbuf -oL -eL npm run dev -- --port "$PORT" --strictPort --host 127.0.0.1) 2>&1 |
    sed -u "s|^|${FRONTEND_TAG}${OFF} |" &
  FRONTEND_PGID="$(group_of "$!")"
  set +m
fi

if [ "$RUN_BACKEND" = 1 ] && [ "$SERVE" = 1 ]; then
  note "backend  ws://$ADDR/ws seed=$SEED view_radius=$VIEW_RADIUS profile=$PROFILE_DIR"
elif [ "$RUN_BACKEND" = 1 ]; then
  note "backend  seed=$SEED view_radius=$VIEW_RADIUS ticks=$TICKS profile=$PROFILE_DIR"
fi
if [ "$RUN_FRONTEND" = 1 ]; then
  note "frontend http://127.0.0.1:$PORT  (Ctrl-C stops both)"
fi

if [ "$RUN_BACKEND" = 1 ] && [ "$RUN_FRONTEND" = 1 ]; then
  while group_running "$BACKEND_PGID" && group_running "$FRONTEND_PGID"; do
    sleep 0.5
  done
  if group_running "$FRONTEND_PGID"; then
    set +e
    wait "$BACKEND_PGID"
    backend_status=$?
    set -e
    if [ "$backend_status" -ne 0 ]; then
      fail "the backend exited with status $backend_status"
    fi
    warn "the backend finished its $TICKS ticks and left a save in .run/; the dev server keeps running"
    set +e
    wait "$FRONTEND_PGID"
    frontend_status=$?
    set -e
    [ "$frontend_status" -eq 0 ] || fail "the dev server exited with status $frontend_status"
  else
    set +e
    wait "$FRONTEND_PGID"
    frontend_status=$?
    set -e
    [ "$frontend_status" -eq 0 ] || fail "the dev server exited with status $frontend_status"
  fi
elif [ "$RUN_FRONTEND" = 1 ]; then
  set +e
  wait "$FRONTEND_PGID"
  frontend_status=$?
  set -e
  [ "$frontend_status" -eq 0 ] || fail "the dev server exited with status $frontend_status"
else
  set +e
  wait "$BACKEND_PGID"
  backend_status=$?
  set -e
  [ "$backend_status" -eq 0 ] || fail "the backend exited with status $backend_status"
fi

note "stopped"