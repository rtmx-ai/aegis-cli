#!/usr/bin/env bash
# setup.sh — THIN shim (SETUP-001). One-command host bring-up for the full aegis
# stack. All orchestration + UI lives in the std-lib Python package scripts/setup/
# (orchestrator/steps/ui/catalog/profile); this shim only ensures python3 and hands
# off. See docs/requirements/setup-orchestrator.md and docs/operator-guide.md.
#
#   ./setup.sh                      # menu of catalog models (auto-selects recommended)
#   ./setup.sh --model-choice <id>  # download a specific catalog model
#   ./setup.sh --model <path.gguf>  # use a local GGUF
#   ./setup.sh -v                   # also stream build output to the terminal
set -eu
cd "$(dirname "$0")"
if ! command -v python3 >/dev/null 2>&1; then
	echo "setup: python3 is required — install it (e.g. 'sudo apt-get install -y python3'), then re-run ./setup.sh" >&2
	exit 1
fi
exec python3 -m scripts.setup.main "$@"
