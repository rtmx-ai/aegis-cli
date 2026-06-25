#!/usr/bin/env bash
# setup.sh — one-command host bring-up for the full aegis stack (REL-004).
#
# Builds the whole stack from pinned source (aegis + OpenCode + llama.cpp), stages
# + verifies the model GGUF, calibrates serving to this host, and runs the
# full-stack integration smoke. Quiet by default: each step shows a live progress
# bar; all build output goes to ./setup.log. Re-runnable + idempotent.
#
#   ./setup.sh                 # quiet (default) — progress bars, output -> setup.log
#   ./setup.sh -v              # verbose — also stream build output to the terminal
#   MODEL_SRC=<dir> ./setup.sh # also stage the model -> calibrate -> integration smoke
#
# Air-gap posture: only the build phase touches the network (pinned source +
# frozen deps); nothing fetches at runtime.
set -uo pipefail
cd "$(dirname "$0")"
REPO="$PWD"
LOG="$REPO/setup.log"
VERBOSE=0
CONF="$REPO/setup.conf"             # persisted init choices (gitignored)
MODEL_GGUF="${MODEL_GGUF:-${AEGIS_MODEL_GGUF:-}}"

usage() {
	cat <<-USAGE
	aegis setup — build + bring up the full stack (aegis + OpenCode + llama.cpp + model)

	usage: ./setup.sh [-m <path.gguf>] [-v|--verbose] [-h|--help]
	  -m, --model <p>  path to the model GGUF — pinned (sha256) + staged automatically
	  -v, --verbose    also stream build output to the terminal
	  (default)        quiet: per-step progress bar; full output -> setup.log
	env:
	  MODEL_GGUF=<p>   same as --model (or MODEL_SRC=<dir> for an already-pinned model)
	  Choices are saved to setup.conf so re-runs are non-interactive.
	USAGE
}
while [ $# -gt 0 ]; do
	case "$1" in
	-m | --model) MODEL_GGUF="${2:?--model needs a path}" && shift ;;
	-v | --verbose) VERBOSE=1 ;;
	-q | --quiet) VERBOSE=0 ;;
	-h | --help) usage && exit 0 ;;
	*) echo "setup: unknown option: $1" >&2 && usage >&2 && exit 2 ;;
	esac
	shift
done

# Resolve the model GGUF: flag/env > saved setup.conf > interactive prompt. Then
# persist it so the next run is fully non-interactive.
[ -z "$MODEL_GGUF" ] && [ -f "$CONF" ] && . "$CONF" && MODEL_GGUF="${MODEL_GGUF:-${AEGIS_MODEL_GGUF:-}}"
if [ -z "$MODEL_GGUF" ] && [ -t 0 ] && ! grep -q '"sha256": "[0-9a-f]' deploy/models/MODEL_REF 2>/dev/null; then
	printf 'Path to the model GGUF (Enter to skip model setup for now): '
	read -r MODEL_GGUF || MODEL_GGUF=''
fi
if [ -n "$MODEL_GGUF" ]; then printf 'AEGIS_MODEL_GGUF=%s\n' "$MODEL_GGUF" >"$CONF"; fi

# --- palette (interactive terminal only; respects NO_COLOR) ------------------
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
	B=$'\033[1m' D=$'\033[2m' R=$'\033[31m' G=$'\033[32m' Y=$'\033[33m' C=$'\033[36m' X=$'\033[0m'
	TTY=1
else
	B='' D='' R='' G='' Y='' C='' X='' TTY=0
fi
trap '[ "$TTY" = 1 ] && printf "\033[?25h"' EXIT INT TERM # always restore the cursor

have() { command -v "$1" >/dev/null 2>&1; }
model_name() { grep -o '"name"[^,]*' deploy/models/MODEL_REF 2>/dev/null | sed 's/.*: *"//; s/".*//'; }

TOTAL=7 STEP=0 FAILED='' STAGED=0 SMOKE=0
: >"$LOG"
echo "aegis setup — $(date) — verbose=$VERBOSE" >>"$LOG"

hr() { printf '%s────────────────────────────────────────────────────────%s\n' "$D" "$X"; }
begin() { # label
	STEP=$((STEP + 1))
	printf '\n%s[%d/%d]%s %s%s%s\n' "$C$B" "$STEP" "$TOTAL" "$X" "$B" "$1" "$X"
	printf '\n========== [%d/%d] %s ==========\n' "$STEP" "$TOTAL" "$1" >>"$LOG"
}
done_ok() { printf '  %s✓%s %s %s(%ss)%s\n' "$G" "$X" "$1" "$D" "$2" "$X"; }
done_skip() {
	printf '  %s⊘%s %s%s%s\n' "$Y" "$X" "$D" "$1" "$X"
}
done_fail() {
	printf '  %s✗%s %s %s(%ss — see setup.log)%s\n' "$R" "$X" "$1" "$D" "$2" "$X"
	FAILED="$FAILED $1"
}

# bar: an indeterminate bouncing segment (we cannot know a compile's %)
bar() { # tick
	local track=28 seg=5 t=$1 span p i out=''
	span=$((track - seg))
	p=$((t % (span * 2)))
	[ "$p" -gt "$span" ] && p=$((span * 2 - p))
	i=0
	while [ "$i" -lt "$track" ]; do
		if [ "$i" -ge "$p" ] && [ "$i" -lt $((p + seg)) ]; then out="$out█"; else out="$out░"; fi
		i=$((i + 1))
	done
	printf '%s' "$out"
}

# run a long command: quiet -> LOG with a live bar; verbose -> tee to terminal.
run() { # label cmd...
	local label=$1
	shift
	local start
	start=$(date +%s)
	if [ "$VERBOSE" = 1 ] || [ "$TTY" = 0 ]; then
		if [ "$VERBOSE" = 1 ]; then "$@" 2>&1 | tee -a "$LOG"; else
			printf '  %srunning… (tail -f setup.log)%s\n' "$D" "$X"
			"$@" >>"$LOG" 2>&1
		fi
		local rc=${PIPESTATUS[0]:-$?}
	else
		"$@" >>"$LOG" 2>&1 &
		local pid=$! t=0
		printf '\033[?25l'
		while kill -0 "$pid" 2>/dev/null; do
			local dt=$(($(date +%s) - start))
			printf '\r  %s%s%s  %s%dm%02ds%s ' "$C" "$(bar $t)" "$X" "$D" "$((dt / 60))" "$((dt % 60))" "$X"
			t=$((t + 1))
			sleep 0.12
		done
		printf '\033[?25h\r\033[K'
		wait "$pid"
		local rc=$?
	fi
	local dt=$(($(date +%s) - start))
	if [ "$rc" = 0 ]; then done_ok "$label" "$dt"; else done_fail "$label" "$dt"; fi
	return "$rc"
}

# ============================================================================
# [1/7] toolchain — runs in-process so PATH exports propagate to later phases.
begin "Bootstrapping toolchain"
{
	missing=''
	os="$(uname -s | tr '[:upper:]' '[:lower:]')"
	case "$(uname -m)" in x86_64) arch=amd64 ;; aarch64 | arm64) arch=arm64 ;; *) arch=amd64 ;; esac
	pkg_hint() {
		if have apt-get; then echo "sudo apt-get install -y $1"
		elif have dnf; then echo "sudo dnf install -y $1"
		elif have brew; then echo "brew install $1"
		elif have pacman; then echo "sudo pacman -S --noconfirm $1"
		else echo "(install '$1' via your package manager)"; fi
	}
	if ! have bun; then curl -fsSL https://bun.sh/install | bash || true; fi
	export BUN_INSTALL="${BUN_INSTALL:-$HOME/.bun}"
	export PATH="$BUN_INSTALL/bin:$PATH"
	have bun || missing="$missing bun"
	if ! have go; then
		gover="$(sed -n 's/^go \([0-9][0-9.]*\).*/\1/p' go.mod 2>/dev/null | head -1)"
		gover="${gover:-1.25.11}"
		mkdir -p "$HOME/.aegis-toolchain"
		curl -fsSL "https://go.dev/dl/go${gover}.${os}-${arch}.tar.gz" | tar -C "$HOME/.aegis-toolchain" -xz || true
	fi
	export PATH="$HOME/.aegis-toolchain/go/bin:$(go env GOPATH 2>/dev/null || echo "$HOME/go")/bin:$PATH"
	have go || missing="$missing go"
	for t in git cmake; do
		have "$t" && continue
		c="$(pkg_hint "$t")"
		sudo -n true 2>/dev/null && eval "$c" >/dev/null 2>&1 || true
		have "$t" || missing="$missing $t ($c)"
	done
	have cc || have gcc || missing="$missing cc ($(pkg_hint build-essential))"
	echo "missing:${missing:-none}"
} >>"$LOG" 2>&1
if [ -n "${missing# }" ]; then
	done_fail "toolchain — missing:$missing" 0
	printf '\n%s✗ install the missing tools above, then re-run ./setup.sh%s\n' "$R" "$X"
	printf '  %s(Bun + Go auto-install user-local; cmake/cc/git need your package manager)%s\n' "$D" "$X"
	exit 1
fi
done_ok "toolchain ready ($(go version 2>/dev/null | awk '{print $3}'), bun $(bun --version 2>/dev/null))" 0

# [2/7] aegis
begin "Building aegis (Go, vendored/offline)"
run "aegis" make build || true

# [3/7] OpenCode
begin "Building OpenCode from pinned source"
run "opencode" scripts/build-opencode.sh || true

# [4/7] llama.cpp
begin "Building llama.cpp from pinned source"
run "llama-server" scripts/build-llama.sh || true

# [5/7] model — pin-as-output (--model) then stage; auto-verified by sha256.
begin "Staging the model"
if [ -n "$FAILED" ]; then
	done_skip "skipped — a build failed (see Next)"
elif [ -n "$MODEL_GGUF" ]; then
	# Hash the provided GGUF -> deploy/models/MODEL_REF, then stage from its dir.
	if run "pin + stage model (sha256-verified)" \
		sh -c 'scripts/pin-model.sh "$1" && MODEL_SRC="$(dirname "$1")" scripts/stage-model.sh' _ "$MODEL_GGUF"; then
		STAGED=1
	fi
elif [ -n "${MODEL_SRC:-}" ]; then
	if run "model staged + sha256-verified" scripts/stage-model.sh; then STAGED=1; fi
else
	done_skip "no model: pass --model <path.gguf> (or MODEL_SRC=<dir>) to stage -> calibrate -> smoke"
fi
model_path="deploy/models/$(model_name)"

# [6/7] calibrate
begin "Calibrating the serving to this host"
if [ "$STAGED" = 1 ] && [ -f "$model_path" ]; then
	run "calibration written" scripts/bench.sh --model "$model_path" || true
else
	done_skip "needs a staged model"
fi

# [7/7] integration smoke
begin "Full-stack integration smoke (EGRESS=0)"
if [ "$STAGED" = 1 ] && [ -f "$model_path" ]; then
	if run "stack completed the task" scripts/integration-smoke.sh; then SMOKE=1; fi
else
	done_skip "needs a staged model"
fi

# ============================================================================
art() { # path
	if [ -e "$1" ]; then printf '  %s•%s %s %s(%s)%s\n' "$C" "$X" "$1" "$D" "$(du -h "$1" 2>/dev/null | cut -f1)" "$X"
	else printf '  %s·%s %s %s(not built)%s\n' "$D" "$X" "$1" "$D" "$X"; fi
}
printf '\n'
hr
printf '%sArtifacts%s\n' "$B" "$X"
art bin/aegis
art deploy/opencode/bin/opencode
art deploy/llama-server/bin/llama-server
[ -n "${model_path:-}" ] && art "$model_path"
art deploy/llama-server/calibration.json
printf '  %slog:%s %s\n' "$D" "$X" "$LOG"

printf '\n%sNext%s\n' "$B" "$X"
if [ -n "$FAILED" ]; then
	printf '  %s✗ build failed:%s%s\n' "$R" "$X" "$FAILED"
	printf '    inspect:  %stail -n 50 setup.log%s    (or re-run with %s./setup.sh -v%s)\n' "$B" "$X" "$B" "$X"
	printf '    then fix the missing dep / error and re-run %s./setup.sh%s\n' "$B" "$X"
	exit 1
elif [ "$SMOKE" = 1 ]; then
	printf '  %s✓ full stack validated — a real task completed (EGRESS=0).%s\n' "$G" "$X"
	printf '    install + run in the enclave:  %sdocs/operator-guide.md%s\n' "$B" "$X"
elif [ "$STAGED" = 1 ]; then
	printf '  %s✓ stack built + model staged; smoke incomplete — see setup.log.%s\n' "$Y" "$X"
else
	printf '  %s✓ stack built.%s one step to finish the bring-up:\n' "$G" "$X"
	printf '    run %s./setup.sh --model <path/to/model.gguf>%s\n' "$B" "$X"
	printf '    %s(pins it by sha256, stages, calibrates, and runs the integration smoke)%s\n' "$D" "$X"
fi
