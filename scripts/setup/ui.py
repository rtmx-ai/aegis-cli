"""Terminal UI for the aegis setup orchestrator (SETUP-006).

Isolated from step logic: this module only renders. Pure helpers (bar, bounce,
fmt_dur, truncate) are unit-tested; the live Panel does the ANSI multi-line redraw
on a tty (header + progress bar + a tail of the step's last N lines). Non-tty and
--verbose paths degrade to plain lines.
"""
import os
import select
import shutil
import sys

# ---- pure render helpers (unit-tested) -------------------------------------


def bar(pct, width=24):
    """Determinate bar for pct in [0,100]."""
    pct = 0 if pct < 0 else 100 if pct > 100 else pct
    fill = int(round(pct * width / 100.0))
    return "[" + "█" * fill + "░" * (width - fill) + "] %3d%%" % int(pct)


def bounce(tick, width=24, seg=5):
    """Indeterminate bouncing segment (when no % is knowable)."""
    span = width - seg
    p = tick % (span * 2)
    if p > span:
        p = span * 2 - p
    return "[" + "".join("█" if p <= i < p + seg else "░" for i in range(width)) + "]"


def fmt_dur(secs):
    secs = int(secs)
    return "%dm%02ds" % (secs // 60, secs % 60)


def truncate(s, width):
    s = s.replace("\t", " ").rstrip("\n")
    return s if len(s) <= width else s[: width - 1] + "…"


class UI:
    def __init__(self, verbose=False, stream=None):
        self.stream = stream or sys.stderr
        self.verbose = verbose
        self.tty = self.stream.isatty() and os.environ.get("NO_COLOR") is None
        if self.tty:
            self.B, self.D, self.R, self.G, self.Y, self.C, self.X = (
                "\033[1m", "\033[2m", "\033[31m", "\033[32m", "\033[33m", "\033[36m", "\033[0m")
        else:
            self.B = self.D = self.R = self.G = self.Y = self.C = self.X = ""

    def width(self):
        return shutil.get_terminal_size((80, 24)).columns

    def write(self, s):
        self.stream.write(s)
        self.stream.flush()

    def header(self, step, total, title):
        self.write("\n%s[%d/%d]%s %s%s%s\n" % (self.C + self.B, step, total, self.X, self.B, title, self.X))

    def hide_cursor(self):
        if self.tty:
            self.write("\033[?25l")

    def show_cursor(self):
        if self.tty:
            self.write("\033[?25h")


class Panel:
    """A live, in-place multi-line region: a progress line + the step's last N
    output lines. Only active on a tty (SETUP-006)."""

    def __init__(self, ui, tail=5):
        self.ui = ui
        self.tail = tail
        self.height = 0
        self.active = ui.tty

    def render(self, prog_line, lines):
        if not self.active:
            return
        w = self.ui.width()
        block = [prog_line] + [self.ui.D + "  │ " + truncate(x, w - 4) + self.ui.X for x in lines[-self.tail:]]
        while len(block) < self.tail + 1:
            block.append("")
        out = []
        if self.height:
            out.append("\033[%dA" % self.height)
        for ln in block:
            out.append("\033[2K\r" + ln + "\n")
        self.height = len(block)
        self.ui.write("".join(out))

    def clear(self):
        if self.active and self.height:
            self.ui.write("\033[%dA" % self.height + "\033[J")
            self.height = 0


def tail_lines(path, n):
    """Last n non-empty lines of a file (best-effort, cheap)."""
    try:
        with open(path, "rb") as f:
            data = f.read()[-65536:]
    except OSError:
        return []
    rows = [r for r in data.decode("utf-8", "replace").splitlines() if r.strip()]
    return rows[-n:]


def read_timeout(timeout):
    """Read a line from stdin with a timeout (POSIX tty). None on timeout/EOF."""
    try:
        r, _, _ = select.select([sys.stdin], [], [], timeout)
    except (OSError, ValueError):
        return None
    if r:
        line = sys.stdin.readline()
        return line.strip() if line else None
    return None
