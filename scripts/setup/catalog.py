"""Model catalog + selection (MODEL-003, migrated into the orchestrator).

Loads deploy/models/catalog.json and resolves which model to use:
--model > --model-choice > saved setup.conf > interactive menu (tty, timeout
auto-selects the recommended entry) > recommended default (non-tty). A discovered
local GGUF is offered as a menu option. Returns a (kind, value) decision the model
step acts on: ("choice", id) to download, ("local", path) to use, or ("skip", "").
"""
import glob
import json
import os
import subprocess

from . import ui as _ui

CATALOG = "deploy/models/catalog.json"
_GB = 1073741824
_OVERHEAD = 2 * _GB  # KV cache + context + runtime/OS headroom


def load():
    try:
        with open(CATALOG) as f:
            return json.load(f).get("models", [])
    except (OSError, ValueError):
        return []


def recommended_id(models=None):
    for m in models if models is not None else load():
        if m.get("recommended"):
            return m["id"]
    return None


def gb(size):
    return round((size or 0) / 1073741824.0)


def host_ram_bytes():
    """Total host RAM, or None if it can't be determined (then nothing is filtered)."""
    try:
        with open("/proc/meminfo") as f:
            for line in f:
                if line.startswith("MemTotal:"):
                    return int(line.split()[1]) * 1024
    except OSError:
        pass
    try:
        out = subprocess.run(["sysctl", "-n", "hw.memsize"], capture_output=True, text=True).stdout.strip()
        return int(out) if out else None
    except (OSError, ValueError):
        return None


def required_ram(size):
    """Rough RAM a GGUF needs to run acceptably: weights + ~10% + overhead."""
    return int((size or 0) * 1.1) + _OVERHEAD


def fits(size, ram):
    """Whether a model of this size fits the host. Unknown RAM → assume yes."""
    return ram is None or required_ram(size) <= ram


def default_choice(models=None, ram=None):
    """The recommended model if it fits the host, else the largest one that does."""
    models = load() if models is None else models
    ram = host_ram_bytes() if ram is None else ram
    fitting = [m for m in models if fits(m.get("size", 0), ram)]
    for m in fitting:
        if m.get("recommended"):
            return m["id"]
    return max(fitting, key=lambda m: m.get("size", 0))["id"] if fitting else None


def discover_local():
    """Largest .gguf under common host locations (a no-download menu option)."""
    home = os.path.expanduser("~")
    dirs = [os.environ.get("MODEL_SRC", ""), "models", os.path.join(home, "models"),
            os.path.join(home, "Downloads"), os.path.join(home, ".cache", "huggingface"),
            os.path.join(home, ".cache", "lm-studio", "models"), "/models", "/opt/models", "/srv/models"]
    best, best_sz = None, -1
    for d in dirs:
        if not d or not os.path.isdir(d):
            continue
        for p in glob.glob(os.path.join(d, "**", "*.gguf"), recursive=True):
            try:
                sz = os.path.getsize(p)
            except OSError:
                continue
            if sz > best_sz:
                best, best_sz = p, sz
        if best:
            return best
    return best


def menu(ui, timeout):
    """Interactive menu; returns (kind, value). Models that exceed host RAM are shown
    struck-through + non-selectable (MODEL-004); the timeout auto-selects the
    recommended model if it fits, else the largest that does."""
    models = load()
    ram = host_ram_bytes()
    default_id = default_choice(models, ram)
    options, struck, rec_idx = [], [], 1
    for m in models:
        if fits(m.get("size", 0), ram):
            star = " ★" if m["id"] == default_id else "  "
            options.append(("choice", m["id"], "%s %s  ~%dGB  download" % (star, m["name"], gb(m.get("size")))))
            if m["id"] == default_id:
                rec_idx = len(options)
        else:
            struck.append("✗ %s  ~%dGB  (needs ~%dGB RAM)" % (m["name"], gb(m.get("size")), gb(required_ram(m.get("size", 0)))))
    loc = discover_local()
    if loc:
        options.append(("local", loc, "   %s  (local, no download)" % loc))
    options.append(("path", "", "   enter a path…"))
    options.append(("skip", "", "   skip — build the stack only"))

    ui.write("\nNo model selected — choose one (auto-selects #%d in %ds; Ctrl-C aborts):\n" % (rec_idx, timeout))
    for i, (_, _, label) in enumerate(options, 1):
        ui.write("  %d)%s\n" % (i, label))
    for s in struck:
        ui.write("     %s\n" % ui.strike(s))
    ui.write("> ")
    raw = _ui.read_timeout(timeout)
    choice = rec_idx if not raw or not raw.isdigit() else int(raw)
    if not (1 <= choice <= len(options)):
        choice = rec_idx
    kind, value, _ = options[choice - 1]
    if kind == "path":
        while True:
            ui.write("Path to the model GGUF: ")
            p = (_ui.read_timeout(86400) or "").strip()
            if not p:
                return ("skip", "")
            if os.path.isfile(p):
                return ("local", p)
            ui.write("  not found: %s — try again.\n" % p)
    return (kind, value)


def resolve(args, conf, ui):
    """Decide (kind, value): flags > conf > menu(tty) > recommended (non-tty)."""
    if args.model and os.path.isfile(args.model):
        return ("local", args.model)
    if args.model:
        ui.write("setup: model not found: %s\n" % args.model)
    if args.model_choice:
        return ("choice", args.model_choice)
    if conf.get("AEGIS_MODEL_GGUF") and os.path.isfile(conf["AEGIS_MODEL_GGUF"]):
        return ("local", conf["AEGIS_MODEL_GGUF"])
    if conf.get("AEGIS_MODEL_CHOICE"):
        return ("choice", conf["AEGIS_MODEL_CHOICE"])
    import sys
    if sys.stdin.isatty():
        return menu(ui, args.timeout)
    rid = default_choice()  # fit-aware: recommended if it fits, else largest fitting
    if rid:
        ui.write("setup: no tty — defaulting to the recommended model that fits: %s\n" % rid)
        return ("choice", rid)
    return ("skip", "")
