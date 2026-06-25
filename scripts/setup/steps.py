"""Setup steps (SETUP-002/003/005).

Each step is declarative: a title, an is_done() idempotency gate, a cmd() that
reuses the dedicated shell script (no build logic duplicated — DRY), and a
progress() strategy. The Toolchain step runs in-process because it mutates
os.environ['PATH'] so later subprocess steps inherit the bootstrapped bun/go.
"""
import json
import os
import platform
import re
import shutil
import subprocess

from . import catalog, profile


def _have(t):
    return shutil.which(t) is not None


def _gomod_version():
    try:
        with open("go.mod") as f:
            for line in f:
                m = re.match(r"go (\d[\d.]*)", line.strip())
                if m:
                    return m.group(1)
    except OSError:
        pass
    return None


def _pkg_hint(pkg):
    for mgr, cmd in (("apt-get", "sudo apt-get install -y %s"), ("dnf", "sudo dnf install -y %s"),
                     ("brew", "brew install %s"), ("pacman", "sudo pacman -S --noconfirm %s")):
        if _have(mgr):
            return cmd % pkg
    return "(install '%s' via your package manager)" % pkg


def _sudo_ok():
    try:
        return subprocess.run(["sudo", "-n", "true"], capture_output=True).returncode == 0
    except OSError:
        return False


def _model_ref():
    try:
        with open("deploy/models/MODEL_REF") as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def _human(path):
    try:
        n = os.path.getsize(path)
    except OSError:
        return "?"
    for u in ("B", "K", "M", "G"):
        if n < 1024 or u == "G":
            return "%d%s" % (n, u)
        n //= 1024


class Step:
    id = ""
    title = ""
    skip_reason = ""

    def is_done(self):
        return False

    def cmd(self):
        return None

    def done_summary(self):
        """A short description of the already-present result (for 'Already done.')."""
        return ""

    def progress(self, tail, elapsed):
        return profile.pct_from_time(elapsed, profile.estimate(self.id))


class Toolchain(Step):
    id, title = "toolchain", "Bootstrapping toolchain"

    def is_done(self):
        return all(_have(t) for t in ("bun", "go", "cmake", "git")) and (_have("cc") or _have("gcc"))

    def run_inproc(self, log):
        missing = []
        if not _have("bun"):
            subprocess.run("curl -fsSL https://bun.sh/install | bash", shell=True, stdout=log, stderr=log)
        bun = os.path.expanduser(os.environ.get("BUN_INSTALL", os.path.expanduser("~/.bun")))
        os.environ["PATH"] = os.path.join(bun, "bin") + os.pathsep + os.environ["PATH"]
        if not _have("bun"):
            missing.append("bun")
        if not _have("go"):
            gover = _gomod_version() or "1.25.11"
            osn = platform.system().lower()
            arch = {"x86_64": "amd64", "aarch64": "arm64", "arm64": "arm64"}.get(platform.machine(), "amd64")
            tc = os.path.expanduser("~/.aegis-toolchain")
            os.makedirs(tc, exist_ok=True)
            subprocess.run("curl -fsSL https://go.dev/dl/go%s.%s-%s.tar.gz | tar -C %s -xz"
                           % (gover, osn, arch, tc), shell=True, stdout=log, stderr=log)
            os.environ["PATH"] = os.path.join(tc, "go", "bin") + os.pathsep + os.environ["PATH"]
        if not _have("go"):
            missing.append("go")
        else:
            try:
                gp = subprocess.run(["go", "env", "GOPATH"], capture_output=True, text=True).stdout.strip()
                if gp:
                    os.environ["PATH"] = os.path.join(gp, "bin") + os.pathsep + os.environ["PATH"]
            except OSError:
                pass
        for t in ("git", "cmake"):
            if _have(t):
                continue
            hint = _pkg_hint(t)
            if _sudo_ok():
                subprocess.run(hint, shell=True, stdout=log, stderr=log)
            if not _have(t):
                missing.append("%s (%s)" % (t, hint))
        if not (_have("cc") or _have("gcc")):
            missing.append("cc (%s)" % _pkg_hint("build-essential"))
        return (not missing, missing)

    def done_summary(self):
        parts = []
        try:
            parts.append(subprocess.run(["go", "version"], capture_output=True, text=True).stdout.split()[2])
        except Exception:
            pass
        try:
            parts.append("bun " + subprocess.run(["bun", "--version"], capture_output=True, text=True).stdout.strip())
        except Exception:
            pass
        return ", ".join(parts)


class ScriptStep(Step):
    """Reuse a dedicated shell script as the step body (DRY, SETUP-005)."""

    def __init__(self, id, title, argv, done_path, progress_kind="time"):
        self.id, self.title, self.argv, self.done_path, self.progress_kind = id, title, argv, done_path, progress_kind

    def is_done(self):
        return bool(self.done_path) and os.path.exists(self.done_path)

    def cmd(self):
        return self.argv

    def done_summary(self):
        if self.done_path and os.path.exists(self.done_path):
            return "%s (%s)" % (self.done_path, _human(self.done_path))
        return ""

    def progress(self, tail, elapsed):
        if self.progress_kind == "cmake":
            m = re.findall(r"\[\s*(\d+)%\]", tail)
            if m:
                return float(m[-1])
        return super().progress(tail, elapsed)


class ModelStep(Step):
    id, title = "model", "Staging the model"

    def __init__(self, kind, value):
        self.kind, self.value, self.total, self.file = kind, value, 0, None
        if kind == "choice":
            for m in catalog.load():
                if m["id"] == value:
                    self.total, self.file = m.get("size", 0), m.get("file")
                    break
        if kind == "skip":
            self.skip_reason = "no model selected — re-run and choose one to add a model"

    def is_done(self):
        ref = _model_ref()
        if not ref or str(ref.get("sha256", "")).startswith("PENDING"):
            return False
        return os.path.isfile(os.path.join("deploy/models", ref.get("name", "")))

    def done_summary(self):
        ref = _model_ref()
        if ref and ref.get("name"):
            p = os.path.join("deploy/models", ref["name"])
            return "%s (%s)" % (ref["name"], _human(p)) if os.path.exists(p) else ref["name"]
        return ""

    def cmd(self):
        if self.kind == "choice":
            return ["sh", "-c",
                    'p="$(scripts/fetch-model.sh "$1")" && scripts/pin-model.sh "$p" && '
                    'MODEL_SRC="$(dirname "$p")" scripts/stage-model.sh', "_", self.value]
        if self.kind == "local":
            return ["sh", "-c",
                    'scripts/pin-model.sh "$1" && MODEL_SRC="$(dirname "$1")" scripts/stage-model.sh', "_", self.value]
        return None

    def progress(self, tail, elapsed):
        if self.kind == "choice" and self.total and self.file:
            dl = os.path.expanduser(os.environ.get("MODEL_DOWNLOAD_DIR", os.path.expanduser("~/models")))
            for cand in (os.path.join(dl, self.file + ".part"), os.path.join(dl, self.file)):
                try:
                    return min(99.0, os.path.getsize(cand) * 100.0 / self.total)
                except OSError:
                    pass
        return super().progress(tail, elapsed)


def _shell_profile():
    home = os.path.expanduser("~")
    return os.path.join(home, ".zshrc") if "zsh" in os.environ.get("SHELL", "") else os.path.join(home, ".bashrc")


class InstallStep(Step):
    """Install the built aegis binary to a user-local bin dir and ensure it's on PATH."""

    id, title = "install", "Installing aegis to PATH"

    def __init__(self):
        self.dest, self.on_path, self.profile = "", False, ""

    def is_done(self):
        return False  # always (re)install on --install; the copy is idempotent

    def run_inproc(self, log):
        if not os.path.isfile("bin/aegis"):
            return (False, ["bin/aegis not built"])
        dest_dir = os.environ.get("AEGIS_BIN_DIR") or os.path.expanduser("~/.local/bin")
        os.makedirs(dest_dir, exist_ok=True)
        self.dest = os.path.join(dest_dir, "aegis")
        shutil.copy2("bin/aegis", self.dest)
        os.chmod(self.dest, 0o755)
        log.write("installed bin/aegis -> %s\n" % self.dest)
        self.on_path = dest_dir in os.environ.get("PATH", "").split(os.pathsep)
        if not self.on_path:
            self.profile = _shell_profile()
            marker = 'export PATH="%s:$PATH"' % dest_dir
            try:
                existing = open(self.profile).read() if os.path.exists(self.profile) else ""
                if marker not in existing:  # idempotent: don't append twice
                    with open(self.profile, "a") as pf:
                        pf.write("\n# added by aegis setup --install\n%s\n" % marker)
                    log.write("added %s to PATH via %s\n" % (dest_dir, self.profile))
            except OSError as e:
                log.write("could not update %s: %s\n" % (self.profile, e))
        return (True, [])

    def done_summary(self):
        return self.dest


def build_steps(model_decision, install=False):
    """The ordered step list. model_decision is (kind, value) from catalog.resolve."""
    kind, value = model_decision
    model_path = ""
    ref = _model_ref()
    if ref and not str(ref.get("sha256", "")).startswith("PENDING"):
        model_path = os.path.join("deploy/models", ref.get("name", ""))
    steps = [
        Toolchain(),
        ScriptStep("aegis", "Building aegis (Go, vendored/offline)", ["make", "build"], "bin/aegis"),
        ScriptStep("opencode", "Building OpenCode from pinned source",
                   ["scripts/build-opencode.sh"], "deploy/opencode/bin/opencode"),
        ScriptStep("llama", "Building llama.cpp from pinned source",
                   ["scripts/build-llama.sh"], "deploy/llama-server/bin/llama-server", progress_kind="cmake"),
        ModelStep(kind, value),
        ScriptStep("calibrate", "Calibrating the serving to this host",
                   ["scripts/bench.sh", "--model", model_path] if model_path else None,
                   "deploy/llama-server/calibration.json"),
        ScriptStep("smoke", "Full-stack integration smoke (EGRESS=0)",
                   ["scripts/integration-smoke.sh"] if model_path else None, ""),
    ]
    if install:
        steps.append(InstallStep())
    return steps
