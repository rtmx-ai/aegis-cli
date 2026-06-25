"""Setup orchestrator (SETUP-002/003/004).

Sequences steps with the live UI: idempotent skips (is_done), rugged per-step
isolation (a failure is logged + reported, never crashes the run or corrupts
state), and a status-aware summary. Records successful step durations for the
time-estimate progress bar.
"""
import os
import subprocess
import time

from . import profile
from .ui import Panel, bar, bounce, fmt_dur, tail_lines

LOG = "setup.log"


class Orchestrator:
    def __init__(self, ui):
        self.ui = ui
        self.failed = []
        self.staged = False
        self.smoke = False

    def run(self, steps):
        open(LOG, "w").close()
        self.total = len(steps)
        self.ui.hide_cursor()
        try:
            for i, step in enumerate(steps, 1):
                try:
                    self.run_step(i, step)
                except Exception as e:  # ruggedness: a step never crashes the run
                    self.ui.write("  %s✗%s %s %s(%s)%s\n" % (self.ui.R, self.ui.X, step.title, self.ui.D, e, self.ui.X))
                    self.failed.append("%s: %s" % (step.title, e))
        finally:
            self.ui.show_cursor()
        return self.summary()

    def run_step(self, i, step):
        self.ui.header(i, self.total, step.title)
        with open(LOG, "a") as lf:
            lf.write("\n===== [%d/%d] %s =====\n" % (i, self.total, step.title))
        if self.failed and step.id in ("model", "calibrate", "smoke"):
            return self._skip("skipped — an earlier step failed (see Next)")
        if step.is_done():
            return self._ok(step, 0, note=step.title + " (already done)")
        if hasattr(step, "run_inproc"):
            return self._run_inproc(step)
        cmd = step.cmd()
        if cmd is None:
            return self._skip(getattr(step, "skip_reason", "nothing to do"))
        return self._run_cmd(step, cmd)

    def _run_inproc(self, step):
        start = time.time()
        with open(LOG, "a") as lf:
            ok, missing = step.run_inproc(lf)
        dt = time.time() - start
        if ok:
            profile.record(step.id, dt)
            return self._ok(step, dt)
        self._fail(step, dt, "missing: " + " ".join(missing))
        return 1

    def _run_cmd(self, step, cmd):
        start = time.time()
        if self.ui.verbose or not self.ui.tty:
            with open(LOG, "a") as lf:
                out = None if self.ui.verbose else lf
                rc = subprocess.Popen(cmd, stdout=out, stderr=out).wait()
        else:
            panel = Panel(self.ui)
            with open(LOG, "a") as lf:
                proc = subprocess.Popen(cmd, stdout=lf, stderr=lf)
                tick = 0
                while proc.poll() is None:
                    elapsed = time.time() - start
                    tail = tail_lines(LOG, panel.tail)
                    try:
                        pct = step.progress("\n".join(tail), elapsed)
                    except Exception:
                        pct = None
                    g = bar(pct) if pct is not None else bounce(tick)
                    prog = "  %s%s%s  %s%s%s" % (self.ui.C, g, self.ui.X, self.ui.D, fmt_dur(elapsed), self.ui.X)
                    panel.render(prog, tail)
                    tick += 1
                    time.sleep(0.15)
                rc = proc.returncode
            panel.clear()
        dt = time.time() - start
        if rc == 0:
            profile.record(step.id, dt)
            if step.id == "model":
                self.staged = True
            if step.id == "smoke":
                self.smoke = True
            return self._ok(step, dt)
        self._fail(step, dt)
        return rc

    def _ok(self, step, dt, note=None):
        self.ui.write("  %s✓%s %s %s(%s)%s\n" % (self.ui.G, self.ui.X, note or step.title, self.ui.D, fmt_dur(dt), self.ui.X))
        return 0

    def _skip(self, reason):
        self.ui.write("  %s⊘%s %s%s%s\n" % (self.ui.Y, self.ui.X, self.ui.D, reason, self.ui.X))
        return 0

    def _fail(self, step, dt, extra=None):
        self.ui.write("  %s✗%s %s %s(%s — see setup.log)%s\n"
                      % (self.ui.R, self.ui.X, step.title, self.ui.D, fmt_dur(dt), self.ui.X))
        self.failed.append(step.title + (": " + extra if extra else ""))

    def summary(self):
        u = self.ui

        def art(p):
            if os.path.exists(p):
                u.write("  %s•%s %s %s(%s)%s\n" % (u.C, u.X, p, u.D, _du(p), u.X))
            else:
                u.write("  %s·%s %s %s(not built)%s\n" % (u.D, u.X, p, u.D, u.X))

        u.write("\n%sArtifacts%s\n" % (u.B, u.X))
        for p in ("bin/aegis", "deploy/opencode/bin/opencode", "deploy/llama-server/bin/llama-server",
                  "deploy/llama-server/calibration.json"):
            art(p)
        u.write("  %slog:%s ./%s\n" % (u.D, u.X, LOG))

        u.write("\n%sNext%s\n" % (u.B, u.X))
        if self.failed:
            u.write("  %s✗ failed:%s %s\n" % (u.R, u.X, "; ".join(self.failed)))
            u.write("    inspect %stail -n 50 setup.log%s (or ./setup.sh -v), fix, and re-run.\n" % (u.B, u.X))
            return 1
        if self.smoke:
            u.write("  %s✓ full stack validated — a real task completed (EGRESS=0).%s\n" % (u.G, u.X))
            u.write("    install + run in the enclave: %sdocs/operator-guide.md%s\n" % (u.B, u.X))
        elif self.staged:
            u.write("  %s✓ stack built + model staged; smoke incomplete — see setup.log.%s\n" % (u.Y, u.X))
        else:
            u.write("  %s✓ stack built.%s re-run with %s--model <path>%s or pick a catalog model to finish.\n"
                    % (u.G, u.X, u.B, u.X))
        return 0


def _du(p):
    try:
        n = os.path.getsize(p)
    except OSError:
        return "?"
    for unit in ("B", "K", "M", "G"):
        if n < 1024 or unit == "G":
            return "%d%s" % (n, unit)
        n //= 1024
