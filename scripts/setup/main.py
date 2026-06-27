"""Entrypoint for the aegis setup orchestrator (invoked by the thin setup.sh).

Resolves the model (flags > env > setup.conf > menu > recommended), persists the
choice, builds the step list, and runs the orchestrator. Run via the shim:
`python3 -m scripts.setup.main "$@"`.
"""
import argparse
import os
import sys

from . import catalog
from . import origin
from . import ui as ui_mod
from .orchestrator import Orchestrator
from .steps import build_steps

CONF = "setup.conf"


def load_conf():
    d = {}
    try:
        with open(CONF) as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith("#") and "=" in line:
                    k, v = line.split("=", 1)
                    d[k] = v
    except OSError:
        pass
    return d


def save_conf(decision):
    kind, value = decision
    lines = []
    if kind == "local" and os.path.isfile(value):
        lines.append("AEGIS_MODEL_GGUF=%s" % value)
    if kind == "choice":
        lines.append("AEGIS_MODEL_CHOICE=%s" % value)
    if lines:
        try:
            with open(CONF, "w") as f:
                f.write("\n".join(lines) + "\n")
        except OSError:
            pass


def main(argv=None):
    # Run from the repo root regardless of how we were invoked.
    os.chdir(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
    p = argparse.ArgumentParser(prog="setup.sh",
                                description="build + bring up the full aegis stack (aegis + OpenCode + llama.cpp + model)")
    p.add_argument("-m", "--model", help="path to a local model GGUF (pinned by sha256 + staged)")
    p.add_argument("--model-choice", help="download a CATALOG model by id (deploy/models/catalog.json)")
    p.add_argument("-v", "--verbose", action="store_true", help="stream build output to the terminal too")
    p.add_argument("-i", "--install", action="store_true",
                   help="install the built aegis binary to ~/.local/bin and put it on PATH")
    p.add_argument("--timeout", type=int, default=int(os.environ.get("MODEL_TIMEOUT", "30")),
                   help="menu auto-select countdown (default 30s)")
    args = p.parse_args(argv)
    args.model = args.model or os.environ.get("MODEL_GGUF") or os.environ.get("AEGIS_MODEL_GGUF")
    args.model_choice = args.model_choice or os.environ.get("AEGIS_MODEL_CHOICE")

    u = ui_mod.UI(verbose=args.verbose)
    decision = catalog.resolve(args, load_conf(), u)
    save_conf(decision)
    origin.configure()  # MODEL-008: prompt for the model-origin policy (tty only; else default)
    return Orchestrator(u).run(build_steps(decision, install=args.install))


if __name__ == "__main__":
    sys.exit(main())
