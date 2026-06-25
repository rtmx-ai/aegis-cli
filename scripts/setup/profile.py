"""Per-step duration cache for time-estimate progress (SETUP-006).

First run of a step is indeterminate (no data → bouncing bar); after a step
succeeds we record its wall-clock, so subsequent runs show a real-feeling
elapsed/estimate bar. Stored as JSON next to the repo; advisory only — a missing
or corrupt cache just degrades to indeterminate.
"""
import json
import os

_PATH = ".setup-profile.json"


def _load():
    try:
        with open(_PATH) as f:
            d = json.load(f)
        return d if isinstance(d, dict) else {}
    except (OSError, ValueError):
        return {}


def estimate(step_id):
    """Estimated seconds for a step, or None if never recorded."""
    v = _load().get(step_id)
    return float(v) if isinstance(v, (int, float)) and v > 0 else None


def record(step_id, seconds):
    """Record a step's successful duration (smoothed against any prior value)."""
    if seconds <= 0:
        return
    d = _load()
    prev = d.get(step_id)
    d[step_id] = round((prev + seconds) / 2.0, 1) if isinstance(prev, (int, float)) else round(seconds, 1)
    tmp = _PATH + ".tmp"
    try:
        with open(tmp, "w") as f:
            json.dump(d, f)
        os.replace(tmp, _PATH)
    except OSError:
        pass


def pct_from_time(elapsed, est):
    """Time-based progress, capped at 99% until the step actually finishes."""
    if not est or est <= 0:
        return None
    return min(99.0, elapsed * 100.0 / est)
