#!/usr/bin/env bash
# setup.sh — one-command host bring-up for the full aegis stack (REL-004).
#
# Run this on the CONNECTED build host. It bootstraps the toolchain (auto-installs
# what it safely can), builds the whole stack from pinned source (aegis + OpenCode
# + llama.cpp), stages + verifies the model GGUF, calibrates the serving to this
# host, and runs the full-stack integration smoke. Then carry deploy/{opencode,
# llama-server,models}/ + the aegis binary into the enclave (docs/operator-guide.md).
#
# Robust + idempotent: re-runnable; auto-installs Bun + Go (user-local, no sudo);
# attempts/instructs system packages (cmake/cc/git); collects all gaps before
# bailing; model-dependent phases skip with guidance until a GGUF is staged.
# Air-gap posture: only the build phase touches the network (pinned source +
# frozen deps); nothing fetches at runtime.
set -eu
cd "$(dirname "$0")"

# --- progress display: numbered phases [N/total] + ASCII bar + elapsed -------
TOTAL=7
PHASE=0
PHASE_START=0
phase() { # label
	PHASE=$((PHASE + 1))
	local filled=$((PHASE * 24 / TOTAL)) bar="" i=0
	while [ "$i" -lt 24 ]; do
		if [ "$i" -lt "$filled" ]; then bar="$bar#"; else bar="$bar."; fi
		i=$((i + 1))
	done
	PHASE_START=$(date +%s)
	printf '\n\033[1m[%d/%d]\033[0m [%s] %s\n' "$PHASE" "$TOTAL" "$bar" "$1"
}
ok() { printf '\033[32m  ✓ %s (%ds)\033[0m\n' "${1:-done}" "$(($(date +%s) - PHASE_START))"; }
skip() { printf '\033[33m  – skipped: %s\033[0m\n' "$1" >&2; }

have() { command -v "$1" >/dev/null 2>&1; }
model_name() { grep -o '"name"[^,]*' deploy/models/MODEL_REF 2>/dev/null | sed 's/.*: *"//; s/".*//'; }

os="$(uname -s | tr '[:upper:]' '[:lower:]')"
case "$(uname -m)" in x86_64) arch=amd64 ;; aarch64 | arm64) arch=arm64 ;; *) arch=amd64 ;; esac
pkg_hint() {
	if have apt-get; then echo "sudo apt-get install -y $1"
	elif have dnf; then echo "sudo dnf install -y $1"
	elif have brew; then echo "brew install $1"
	elif have pacman; then echo "sudo pacman -S --noconfirm $1"
	else echo "(install '$1' via your package manager)"; fi
}

# --- [1/7] toolchain ---------------------------------------------------------
phase "Bootstrapping toolchain"
missing=""
if ! have bun; then
	echo "  installing Bun (user-local, no sudo)…"
	curl -fsSL https://bun.sh/install | bash >/dev/null 2>&1 || true
fi
export BUN_INSTALL="${BUN_INSTALL:-$HOME/.bun}"
export PATH="$BUN_INSTALL/bin:$PATH"
if have bun; then echo "  bun $(bun --version 2>/dev/null)"; else missing="$missing bun"; fi

if ! have go; then
	gover="$(sed -n 's/^go \([0-9][0-9.]*\).*/\1/p' go.mod 2>/dev/null | head -1)"
	gover="${gover:-1.25.11}"
	echo "  installing Go $gover (user-local)…"
	mkdir -p "$HOME/.aegis-toolchain"
	curl -fsSL "https://go.dev/dl/go${gover}.${os}-${arch}.tar.gz" 2>/dev/null |
		tar -C "$HOME/.aegis-toolchain" -xz 2>/dev/null || true
fi
export PATH="$HOME/.aegis-toolchain/go/bin:$(go env GOPATH 2>/dev/null || echo "$HOME/go")/bin:$PATH"
if have go; then echo "  $(go version 2>/dev/null | awk '{print $1, $3}')"; else missing="$missing go"; fi

ensure_sys() { # tool pkg
	if have "$1"; then return; fi
	cmd="$(pkg_hint "$2")"
	if sudo -n true 2>/dev/null; then echo "  installing $1: $cmd"; eval "${cmd}" >/dev/null 2>&1 || true; fi
	if ! have "$1"; then echo "  $1 MISSING — run: $cmd" >&2; missing="$missing $1"; fi
}
ensure_sys git git
ensure_sys cmake cmake
if ! have cc && ! have gcc; then echo "  C/C++ compiler MISSING — run: $(pkg_hint build-essential)" >&2; missing="$missing cc"; fi

if [ -n "${missing# }" ]; then
	echo "" >&2
	echo "setup: still missing:$missing — install per the hints above, then re-run ./setup.sh" >&2
	exit 1
fi
ok "toolchain ready"

# --- [2/7] aegis -------------------------------------------------------------
phase "Building aegis (Go, vendored/offline)"
make build
ok "aegis built"

# --- [3/7] OpenCode ----------------------------------------------------------
phase "Building OpenCode from pinned source"
scripts/build-opencode.sh
ok "opencode built"

# --- [4/7] llama.cpp ---------------------------------------------------------
phase "Building llama.cpp from pinned source"
scripts/build-llama.sh
ok "llama-server built"

# --- [5/7] model -------------------------------------------------------------
phase "Staging the model"
if [ -n "${MODEL_SRC:-}" ]; then
	scripts/stage-model.sh && ok "model staged + sha256-verified"
else
	skip "set MODEL_SRC=<dir with the pinned GGUF>; finalize the SERVE-016 winner + sha256 in deploy/models/MODEL_REF"
fi
model_path="deploy/models/$(model_name)"

# --- [6/7] calibrate ---------------------------------------------------------
phase "Calibrating the serving to this host"
if [ -f "$model_path" ]; then
	scripts/bench.sh --model "$model_path" && ok "calibration written" || skip "calibration failed (see scripts/bench.sh)"
else
	skip "stage a model first"
fi

# --- [7/7] integration smoke -------------------------------------------------
phase "Full-stack integration smoke (EGRESS=0)"
if [ -f "$model_path" ]; then
	scripts/integration-smoke.sh && ok "stack completed the task"
else
	skip "needs a staged model"
fi

# --- done --------------------------------------------------------------------
printf '\n\033[1m[%d/%d] [########################] done\033[0m\n' "$TOTAL" "$TOTAL"
echo "setup: stack built under deploy/{opencode,llama-server,models}/ + ./bin/aegis."
if [ -d "$HOME/.aegis-toolchain/go/bin" ]; then
	echo "setup: Go/Bun were installed user-local — persist them in your shell profile:"
	echo "         export PATH=\"$BUN_INSTALL/bin:\$HOME/.aegis-toolchain/go/bin:\$PATH\""
fi
echo "setup: next — install + run in the enclave per docs/operator-guide.md."
