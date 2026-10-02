#!/usr/bin/env bash
# Bring up an isolated T3 Code server backed by the fake Codex, seed it, and
# (optionally) capture reference screenshots. Everything lives under one run
# directory in /tmp; nothing touches ~/.t3, ~/.codex or the daily-driver
# server on :3333.
#
# Usage:
#   e2e/run-local.sh up    [--server fork|nightly] [--web] [--capture] [--detach]
#   e2e/run-local.sh down  [--server fork|nightly]
#   e2e/run-local.sh smoke [--server fork|nightly]      # up + seed + verify + down
#
#   --server fork     run the fork server from source (~/L-Projects/t3code-again)
#   --server nightly  run `npx t3@$T3_NIGHTLY_VERSION serve` (default; what CI uses)
#   --web             also start the fork's Vite web UI (fork server only)
#   --capture         run e2e/capture-web-reference.mjs after seeding (implies --web)
#   --detach          leave processes running and exit; stop later with `down`
#
# Environment overrides:
#   T3UI_RUN_DIR        run directory        (default /tmp/t3ui-e2e/run-<server>)
#   T3UI_PORT           server port          (default 4710 nightly, 4720 fork)
#   T3UI_WEB_PORT       Vite port            (default server port + 1)
#   T3UI_FORK_DIR       fork checkout        (default ~/L-Projects/t3code-again)
#   T3_NIGHTLY_VERSION  npm version/tag      (default pinned below)
#   T3UI_NPM_CACHE      npm cache for npx    (default /tmp/t3ui-e2e/npm-cache)
#
# Outputs (in the run dir): state.json (base URL, bearer token, ids),
# logs/*.log, repos/ (fixture git repos), home/ (T3CODE_HOME).
set -euo pipefail

E2E_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_NIGHTLY_VERSION="0.0.45-nightly.20261002.2561"

cmd="${1:-up}"
shift || true
server="nightly"
with_web=0
capture=0
detach=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --server) server="$2"; shift 2 ;;
    --web) with_web=1; shift ;;
    --capture) capture=1; with_web=1; shift ;;
    --detach) detach=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[[ "$server" == fork || "$server" == nightly ]] || { echo "--server must be fork or nightly" >&2; exit 2; }
if [[ $with_web == 1 && "$server" != fork ]]; then
  echo "--web/--capture need --server fork (the reference UI is the fork's web app)" >&2
  exit 2
fi

FORK_DIR="${T3UI_FORK_DIR:-$HOME/L-Projects/t3code-again}"
NIGHTLY_VERSION="${T3_NIGHTLY_VERSION:-$DEFAULT_NIGHTLY_VERSION}"
NPM_CACHE="${T3UI_NPM_CACHE:-/tmp/t3ui-e2e/npm-cache}"
if [[ "$server" == fork ]]; then default_port=4720; else default_port=4710; fi
PORT="${T3UI_PORT:-$default_port}"
WEB_PORT="${T3UI_WEB_PORT:-$((PORT + 1))}"
RUN_DIR="${T3UI_RUN_DIR:-/tmp/t3ui-e2e/run-$server}"
BASE_URL="http://127.0.0.1:$PORT"
WEB_URL="http://127.0.0.1:$WEB_PORT"

# Stop every process group recorded in $RUN_DIR/pids (each launched via setsid).
teardown() {
  [[ -d "$RUN_DIR/pids" ]] || return 0
  for pidfile in "$RUN_DIR"/pids/*; do
    [[ -f "$pidfile" ]] || continue
    local pgid
    pgid="$(cat "$pidfile")"
    if kill -0 "$pgid" 2>/dev/null; then
      kill -TERM -- "-$pgid" 2>/dev/null || true
      for _ in $(seq 1 30); do kill -0 "$pgid" 2>/dev/null || break; sleep 0.1; done
      kill -KILL -- "-$pgid" 2>/dev/null || true
    fi
    rm -f "$pidfile"
  done
  echo "[run-local] stopped processes for $RUN_DIR"
}

# start <name> <cwd> <command...>: run in a new session so the whole tree can
# be killed by process group; stdout/stderr go to logs/<name>.log.
start() {
  local name="$1" cwd="$2"
  shift 2
  # `exec` matters: without it an intermediate bash keeps our stdout open for
  # the lifetime of the service, which hangs anyone piping this script.
  (cd "$cwd" && exec setsid bash -c 'echo $$ > "$0"; exec "$@"' "$RUN_DIR/pids/$name" "$@") \
    >"$RUN_DIR/logs/$name.log" 2>&1 </dev/null &
  for _ in $(seq 1 50); do [[ -s "$RUN_DIR/pids/$name" ]] && return 0; sleep 0.1; done
  echo "[run-local] failed to start $name" >&2
  return 1
}

port_in_use() { (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null; }

wait_for_log() {
  local file="$1" pattern="$2" seconds="$3"
  for _ in $(seq 1 $((seconds * 10))); do
    grep -q "$pattern" "$file" 2>/dev/null && return 0
    sleep 0.1
  done
  echo "[run-local] timed out waiting for '$pattern' in $file" >&2
  tail -40 "$file" >&2 || true
  return 1
}

up() {
  teardown
  for p in "$PORT" $([[ $with_web == 1 ]] && echo "$WEB_PORT"); do
    if port_in_use "$p"; then echo "[run-local] port $p is busy; set T3UI_PORT" >&2; exit 1; fi
  done
  rm -rf "$RUN_DIR"
  mkdir -p "$RUN_DIR"/{home,userhome,cwd,logs,pids,repos}

  # Server settings: Codex points at the fake, every other provider is off so
  # nothing probes real CLIs, and update checks stay offline. The fork stores
  # state under home/dev when --dev-url is set, otherwise home/userdata.
  local settings
  settings=$(cat <<JSON
{
  "enableProviderUpdateChecks": false,
  "providers": {
    "codex": { "binaryPath": "$E2E_DIR/fake-codex/codex", "homePath": "$RUN_DIR/userhome/.codex" },
    "claudeAgent": { "enabled": false },
    "cursor": { "enabled": false },
    "grok": { "enabled": false },
    "opencode": { "enabled": false },
    "antigravity": { "enabled": false }
  }
}
JSON
)
  for state in userdata dev; do
    mkdir -p "$RUN_DIR/home/$state"
    printf '%s\n' "$settings" >"$RUN_DIR/home/$state/settings.json"
  done
  mkdir -p "$RUN_DIR/userhome/.codex"
  # The terminal drawer runs a real shell; give it a prompt without user,
  # host or absolute paths so screenshots match across machines.
  printf '%s\n' "PS1='\\W \\$ '" >"$RUN_DIR/userhome/.bashrc"
  printf '%s\n' '. "$HOME/.bashrc"' >"$RUN_DIR/userhome/.bash_profile"

  # Shared env for the server and the fake Codex it spawns. HOME is isolated
  # so the server never reads the developer's ~/.codex, ~/.claude or shell rc.
  local -a env_vars=(
    "HOME=$RUN_DIR/userhome"
    "T3CODE_TELEMETRY_ENABLED=false"
    "FAKE_CODEX_LOG=$RUN_DIR/logs/fake-codex.jsonl"
    # Upstream probes `codex --version` on PATH (not binaryPath) at boot, so
    # the fake goes first and the developer's real codex is never executed.
    "PATH=$E2E_DIR/fake-codex:$PATH"
    "SHELL=/bin/bash"
    "TERM=xterm-256color"
    "LANG=C.UTF-8"
    "TZ=UTC"
  )

  local -a dev_url_args=()
  [[ $with_web == 1 ]] && dev_url_args=(--dev-url "$WEB_URL")

  if [[ "$server" == fork ]]; then
    [[ -d "$FORK_DIR/apps/server" ]] || { echo "fork not found at $FORK_DIR" >&2; exit 1; }
    start server "$RUN_DIR/cwd" env -i "${env_vars[@]}" \
      node "$FORK_DIR/apps/server/src/bin.ts" serve \
      --base-dir "$RUN_DIR/home" --host 127.0.0.1 --port "$PORT" "${dev_url_args[@]}"
  else
    mkdir -p "$NPM_CACHE"
    start server "$RUN_DIR/cwd" env -i "${env_vars[@]}" "npm_config_cache=$NPM_CACHE" \
      npx -y "t3@$NIGHTLY_VERSION" serve \
      --base-dir "$RUN_DIR/home" --host 127.0.0.1 --port "$PORT"
  fi
  wait_for_log "$RUN_DIR/logs/server.log" '^Token: ' 180
  local token version
  token="$(grep -m1 '^Token: ' "$RUN_DIR/logs/server.log" | awk '{print $2}')"
  version="$(curl -fsS "$BASE_URL/.well-known/t3/environment" | node -pe 'JSON.parse(require("fs").readFileSync(0)).serverVersion')"
  echo "[run-local] $server server $version on $BASE_URL"

  if [[ $with_web == 1 ]]; then
    # VITE_DEV_SERVER_URL makes the web app send HTTP through the Vite proxy
    # (same origin), which is what lets the browser-session cookie stick.
    start web "$FORK_DIR/apps/web" env \
      "PORT=$WEB_PORT" "HOST=127.0.0.1" \
      "VITE_HTTP_URL=$BASE_URL" "VITE_WS_URL=ws://127.0.0.1:$PORT" "VITE_DEV_SERVER_URL=$WEB_URL" \
      vp dev
    wait_for_log "$RUN_DIR/logs/web.log" 'Local:' 60
    echo "[run-local] fork web UI on $WEB_URL"
  fi

  node "$E2E_DIR/seed.mjs" --base-url "$BASE_URL" --token "$token" \
    --repos-dir "$RUN_DIR/repos" --out "$RUN_DIR/state.json"

  if [[ $capture == 1 ]]; then
    node "$E2E_DIR/capture-web-reference.mjs" --state "$RUN_DIR/state.json" --web-url "$WEB_URL" \
      --out "${T3UI_CAPTURE_OUT:-$E2E_DIR/../docs/reference}"
  fi

  cat <<INFO
[run-local] ready
  server:      $BASE_URL  ($server $version)
  state:       $RUN_DIR/state.json  (bearer token, project/thread ids)
  new pairing: node $E2E_DIR/seed.mjs --pair --state $RUN_DIR/state.json
  logs:        $RUN_DIR/logs/
INFO
}

case "$cmd" in
  up)
    if [[ $detach == 1 ]]; then
      up
      echo "[run-local] detached; stop with: $0 down --server $server"
    else
      trap teardown EXIT
      up
      [[ $capture == 1 ]] && exit 0
      echo "[run-local] Ctrl+C to stop"
      while true; do sleep 3600; done
    fi
    ;;
  smoke)
    trap teardown EXIT
    up
    node "$E2E_DIR/seed.mjs" --verify --state "$RUN_DIR/state.json"
    ;;
  down)
    teardown
    ;;
  *)
    echo "unknown command: $cmd (up|down|smoke)" >&2
    exit 2
    ;;
esac
