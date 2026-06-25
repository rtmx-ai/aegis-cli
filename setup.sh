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

step() { printf '\n=== setup: %s ===\n' "$1"; }
have() { command -v "$1" >/dev/null 2>&1; }
model_name() { grep -o '"name"[^,]*' deploy/models/MODEL_REF 2>/dev/null | sed 's/.*: *"//; s/".*//'; }

os="$(uname -s | tr '[:upper:]' '[:lower:]')"
case "$(uname -m)" in x86_64) arch=amd64 ;; aarch64 | arm64) arch=arm64 ;; *) arch=amd64 ;; esac

pkg_hint() { # pkg -> the right install command for this host's package manager
	if have apt-get; then echo "sudo apt-get install -y $1"
	elif have dnf; then echo "sudo dnf install -y $1"
	elif have brew; then echo "brew install $1"
	elif have pacman; then echo "sudo pacman -S --noconfirm $1"
	else echo "(install '$1' via your package manager)"; fi
}

# ---------------------------------------------------------------------------
step "bootstrapping toolchain"
missing=""

# Bun — user-local, no sudo (the official installer).
if ! have bun; then
	echo "  installing Bun (user-local, no sudo)…"
	curl -fsSL https://bun.sh/install | bash >/dev/null 2>&1 || true
fi
export BUN_INSTALL="${BUN_INSTALL:-$HOME/.bun}"
export PATH="$BUN_INSTALL/bin:$PATH"
if have bun; then echo "  bun: $(bun --version 2>/dev/null)"; else missing="$missing bun"; fi

# Go — auto-install the version pinned in go.mod (user-local tarball), if absent.
if ! have go; then
	gover="$(sed -n 's/^go \([0-9][0-9.]*\).*/\1/p' go.mod 2>/dev/null | head -1)"
	gover="${gover:-1.25.11}"
	echo "  installing Go $gover (user-local)…"
	mkdir -p "$HOME/.aegis-toolchain"
	curl -fsSL "https://go.dev/dl/go${gover}.${os}-${arch}.tar.gz" 2>/dev/null |
		tar -C "$HOME/.aegis-toolchain" -xz 2>/dev/null || true
fi
export PATH="$HOME/.aegis-toolchain/go/bin:$(go env GOPATH 2>/dev/null || echo "$HOME/go")/bin:$PATH"
if have go; then echo "  go: $(go version 2>/dev/null | awk '{print $3}')"; else missing="$missing go"; fi

# System packages (need a package manager, usually sudo): git, cmake, a compiler.
ensure_sys() { # tool pkg
	if have "$1"; then echo "  $1: present"; return; fi
	cmd="$(pkg_hint "$2")"
	if sudo -n true 2>/dev/null; then
		echo "  $1 missing — installing: $cmd"
		eval "${cmd}" >/dev/null 2>&1 || true
	fi
	if have "$1"; then echo "  $1: installed"; else echo "  $1 MISSING — run: $cmd" >&2; missing="$missing $1"; fi
}
ensure_sys git git
ensure_sys cmake cmake
if ! have cc && ! have gcc; then
	echo "  C/C++ compiler MISSING — run: $(pkg_hint build-essential)" >&2
	missing="$missing cc"
fi

if [ -n "${missing# }" ]; then
	echo "" >&2
	echo "setup: still missing:$missing — install per the hints above, then re-run ./setup.sh" >&2
	echo "setup: (Bun + Go auto-install user-local; cmake/cc/git need your package manager.)" >&2
	exit 1
fi
echo "  toolchain ready"

# ---------------------------------------------------------------------------
# Build the stack components directly (robust — does not gate on the full
# `make ci`, which needs lint/vuln/netns tooling not present on a fresh host;
# run `make ci-full` separately for full dev-parity gating).
step "building aegis (Go, vendored/offline)"
make build

step "building OpenCode from pinned source"
scripts/build-opencode.sh

step "building llama.cpp from pinned source"
scripts/build-llama.sh

# ---------------------------------------------------------------------------
step "staging the model"
if [ -n "${MODEL_SRC:-}" ]; then
	scripts/stage-model.sh
else
	echo "  skipped: set MODEL_SRC=<dir containing the pinned GGUF> to stage the model." >&2
	echo "  first finalize the SERVE-016 bake-off winner + its sha256 in deploy/models/MODEL_REF." >&2
fi
model_path="deploy/models/$(model_name)"

step "calibrating the serving"
if [ -f "$model_path" ]; then
	scripts/bench.sh --model "$model_path" || echo "  calibration skipped/failed (see scripts/bench.sh)." >&2
else
	echo "  skipped: stage a model first." >&2
fi

step "integration smoke"
if [ -f "$model_path" ]; then
	scripts/integration-smoke.sh
else
	echo "  skipped: needs a staged model." >&2
fi

# ---------------------------------------------------------------------------
step "done"
echo "setup: stack built under deploy/{opencode,llama-server,models}/ + ./bin/aegis."
if [ -d "$HOME/.aegis-toolchain/go/bin" ] || [ -d "$BUN_INSTALL/bin" ]; then
	echo "setup: tools were installed user-local — add to your shell profile to persist:"
	echo "         export PATH=\"$BUN_INSTALL/bin:\$HOME/.aegis-toolchain/go/bin:\$PATH\""
fi
echo "setup: next — install + run in the enclave per docs/operator-guide.md."
