#!/usr/bin/env bash
# install-hooks.sh — install git hooks that enforce local↔CI parity.
#
# The Makefile is the single source of truth for the CI pipeline. These hooks do
# not re-implement any step; they simply call the same make targets CI calls:
#   pre-commit  -> make ci-fast   (fast subset)
#   pre-push    -> make ci        (the full pipeline, identical to GitHub Actions)
#
# Idempotent: re-running overwrites the managed hooks with identical content and
# is always safe. Each hook guards on `make` being present and no-ops cleanly
# (exit 0) with a clear message if it is missing, so a contributor without make
# is never hard-blocked at the hook layer (CI still enforces the gate).
set -eu

# Resolve repo root from this script's location (works from any cwd).
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HOOKS_DIR="$REPO_ROOT/.git/hooks"

if [ ! -d "$REPO_ROOT/.git" ]; then
	echo "install-hooks: not a git repository ($REPO_ROOT/.git missing); nothing to install" >&2
	exit 1
fi

mkdir -p "$HOOKS_DIR"

# write_hook <hook-name> <make-target>
write_hook() {
	hook_name="$1"
	make_target="$2"
	hook_path="$HOOKS_DIR/$hook_name"
	cat >"$hook_path" <<EOF
#!/usr/bin/env bash
# Managed by scripts/install-hooks.sh — do not edit by hand.
# Enforces local↔CI parity by calling the single-source-of-truth make target.
set -eu
REPO_ROOT="\$(git rev-parse --show-toplevel)"
cd "\$REPO_ROOT"
if ! command -v make >/dev/null 2>&1; then
	echo "[$hook_name] 'make' not found; skipping '$make_target' gate (CI still enforces it)." >&2
	exit 0
fi
echo "[$hook_name] running: make $make_target"
exec make $make_target
EOF
	chmod +x "$hook_path"
	echo "installed $hook_name -> make $make_target ($hook_path)"
}

write_hook "pre-commit" "ci-fast"
write_hook "pre-push" "ci"

echo "install-hooks: done. Hooks call the same make targets as CI (single source of truth)."
