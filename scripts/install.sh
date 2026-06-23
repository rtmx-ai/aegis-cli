#!/usr/bin/env bash
# install.sh — connected-staging-then-disconnect bootstrap for aegis-cli.
#
# This is a THIN wrapper. It does not reimplement any step — it orchestrates the
# existing pieces in the right order and documents the one thing the toolchain
# cannot do for you: fetch the external prerequisites BEFORE you disconnect.
#
# ┌─ The flow this script assumes ────────────────────────────────────────────┐
# │ 1. CONNECTED STAGING (on a host that still has a network):                 │
# │      - clone this repo                                                     │
# │      - fetch the external prerequisites listed under PREREQS below         │
# │        (llama.cpp source, a model GGUF, the harness, the rtmx binary)      │
# │      - run this script with --connected to verify they are all staged      │
# │ 2. DISCONNECT the host from the network (pull the cable / down the iface). │
# │ 3. OFFLINE BOOTSTRAP (this script, default mode):                          │
# │      build → aegis init → hooks-install → calibrate → verify air-gap.      │
# │      Every step below is offline-safe; the build is vendored (no fetch).   │
# └────────────────────────────────────────────────────────────────────────────┘
#
# aegis-cli itself NEVER fetches anything (skills/airgap-hygiene). This script
# inherits that discipline: it STAGES/NOTES prerequisites and refuses to download
# them. Anything that must come over the network is your job on the connected
# host, before you disconnect.
#
# Usage:
#   scripts/install.sh                 # offline bootstrap (the default)
#   scripts/install.sh --connected     # connected-staging check (verify prereqs)
#   scripts/install.sh --model PATH    # path to the GGUF used for calibration
#   scripts/install.sh --config PATH   # config file to write (default: aegis.json)
#   scripts/install.sh --force         # overwrite an existing config
#
# Idempotent where possible: re-running rebuilds, re-writes hooks (managed,
# identical), and re-calibrates. The config write refuses to clobber unless
# --force is passed (so a hand-tuned config survives a re-run).
set -eu

PROG="$(basename "$0")"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

MODE="offline"
MODEL="${MODEL:-}"
CONFIG="aegis.json"
FORCE=0

while [ $# -gt 0 ]; do
	case "$1" in
		--connected) MODE="connected"; shift ;;
		--model)     MODEL="${2:-}"; shift 2 ;;
		--config)    CONFIG="${2:-}"; shift 2 ;;
		--force)     FORCE=1; shift ;;
		-h|--help)
			sed -n '2,40p' "$0"; exit 0 ;;
		*) echo "$PROG: unknown arg: $1" >&2; exit 2 ;;
	esac
done

cd "$REPO_ROOT"

log() { echo "[$PROG] $*"; }
note() { echo "[$PROG] NOTE: $*" >&2; }

# --- external prerequisites (STAGED on the connected host; NOT fetched here) ---
#
# These live OUTSIDE this repo and outside aegis-cli's control. Fetch them while
# connected; this script only checks/points at them, it never downloads.
#
#   1. llama.cpp (built from source, no telemetry) -> provides `llama-server`.
#      See deploy/llama-server/README.md. Build with the right backend per target
#      (CPU on the Ryzen, Metal on the Mac). Put `llama-server` on PATH.
#   2. A model GGUF (e.g. our MoE at Q4_K_M) -> pass via --model. This is the
#      large artifact; stage it to /models/ before disconnecting.
#   3. The harness (opencode default, or Goose) -> hardened config in deploy/.
#   4. The rtmx binary (static Go) -> the requirements engine the loop drives.
#
# check_prereq <name> <command-or-path-test...> : warns (does not fail) so the
# offline bootstrap can proceed and the operator sees exactly what is missing.
report_prereqs() {
	log "external prerequisites (stage these while connected; this script does not fetch):"
	if command -v llama-server >/dev/null 2>&1; then
		log "  [ok]   llama-server on PATH"
	else
		note "  llama-server NOT found on PATH — build llama.cpp from source (deploy/llama-server/README.md)"
	fi
	if [ -n "$MODEL" ] && [ -f "$MODEL" ]; then
		log "  [ok]   model GGUF: $MODEL"
	else
		note "  model GGUF not provided/found (--model PATH) — stage it to /models/ before disconnecting"
	fi
	if command -v opencode >/dev/null 2>&1 || command -v goose >/dev/null 2>&1; then
		log "  [ok]   harness on PATH (opencode/goose)"
	else
		note "  harness NOT found — install opencode (default) or Goose; hardened configs in deploy/"
	fi
	if command -v rtmx >/dev/null 2>&1; then
		log "  [ok]   rtmx on PATH"
	else
		note "  rtmx NOT found — stage the static rtmx binary (the requirements engine)"
	fi
}

if [ "$MODE" = "connected" ]; then
	log "connected-staging check: verifying external prerequisites are present"
	report_prereqs
	log "staging check done. Resolve any NOTE above, then DISCONNECT and re-run without --connected."
	exit 0
fi

# --- offline bootstrap --------------------------------------------------------
log "offline bootstrap (this and every step below is offline-safe; build is vendored)"
report_prereqs

# 1. Build the static binary from vendored deps (no network fetch).
log "step 1/4: build (make build — vendored, offline)"
make build

# Use the freshly built binary if present, else fall back to PATH.
AEGIS="$REPO_ROOT/aegis"
[ -x "$AEGIS" ] || AEGIS="aegis"

# 2. Detect host, plan target/tier/calibration, write the offline-safe config.
log "step 2/4: aegis init (detect host -> plan -> write config: $CONFIG)"
INIT_ARGS="init --config $CONFIG"
[ "$FORCE" -eq 1 ] && INIT_ARGS="$INIT_ARGS --force"
# shellcheck disable=SC2086
"$AEGIS" $INIT_ARGS

# 3. Install git hooks (local↔CI parity; single source of truth = the Makefile).
log "step 3/4: make hooks-install (git hooks call the same make targets as CI)"
make hooks-install

# 4. Calibrate serving to THIS host (writes deploy/llama-server/calibration.json).
#    bench.sh auto-detects the target and emits a valid calibration even if
#    llama-server/the model are absent (re-run on the real host to measure).
log "step 4/4a: scripts/bench.sh (calibrate serving to this host)"
if [ -n "$MODEL" ]; then
	scripts/bench.sh --model "$MODEL"
else
	note "no --model given; bench.sh will emit a default calibration with a model placeholder"
	scripts/bench.sh --model ""
fi

# 5. Prove the enclave is closed (EGRESS=0). This is the hard gate.
log "step 4/4b: scripts/verify-airgap.sh (prove zero egress)"
scripts/verify-airgap.sh -- "$AEGIS" run --once

log "bootstrap complete. Config: $CONFIG  Calibration: deploy/llama-server/calibration.json"
log "Review the calibration, point it at your real GGUF, and you are ready to 'aegis run'."
