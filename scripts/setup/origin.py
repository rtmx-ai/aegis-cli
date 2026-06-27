"""origin.py — MODEL-008 model-origin policy prompt.

At init, ask the operator which model-origin countries to allow or deny — the countries the
catalog actually contains — and write deploy/models/origin-policy.json. Non-interactive runs
leave the shipped default in place. Pairs with the origin gate (MODEL-007,
``aegis verify-env --check-origin``). See docs/model-compliance.md.
"""
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CATALOG_PATH = REPO / "deploy" / "models" / "catalog.json"
POLICY_PATH = REPO / "deploy" / "models" / "origin-policy.json"

_NOTE = (
    "Per-country model-origin policy (MODEL-006/008). `default` covers unlisted/unknown "
    "origins; the origin gate (aegis verify-env --check-origin / make origin-gate) fails when "
    "the pinned model's (MODEL_REF) origin is not allowed. Written by the setup origin-policy "
    "prompt; edit freely. See docs/model-compliance.md."
)


def catalog_countries(catalog_path=CATALOG_PATH):
    """The distinct origin countries present in the catalog (sorted, upper-cased)."""
    d = json.loads(Path(catalog_path).read_text())
    out = []
    for m in d.get("models", []):
        o = (m.get("origin") or "").strip().upper()
        if o and o not in out:
            out.append(o)
    return sorted(out)


def build_policy(decisions, default="deny"):
    """decisions: {country: bool allow}. Returns the policy dict (default-deny for the rest)."""
    return {
        "note": _NOTE,
        "default": default,
        "countries": {c: ("allow" if a else "deny") for c, a in decisions.items()},
    }


def prompt_decisions(countries, ask):
    """ask(prompt)->str. Returns {country: bool allow}; an empty answer denies (safe default)."""
    decisions = {}
    for c in countries:
        ans = ask("  Allow models with country of origin %s? [y/N]: " % c).strip().lower()
        decisions[c] = ans in ("y", "yes")
    return decisions


def write_policy(policy, policy_path=POLICY_PATH):
    Path(policy_path).write_text(json.dumps(policy, indent=2) + "\n")
    return policy


def configure(ask=None, interactive=None, catalog_path=CATALOG_PATH, policy_path=POLICY_PATH):
    """Run the origin-policy step. Interactive (a tty, unless overridden) prompts per country
    and writes the policy; non-interactive leaves the shipped default untouched and returns
    None. ``ask`` defaults to ``input``; inject it in tests.
    """
    if interactive is None:
        interactive = sys.stdin.isatty()
    if not interactive:
        return None
    countries = catalog_countries(catalog_path)
    if not countries:
        return None
    if ask is None:
        ask = input
        print("\nModel-origin policy (provenance gate) — allow/deny per country of origin.")
        print("A controlled/ITAR deployment should deny non-allied origins. See docs/model-compliance.md.")
    policy = build_policy(prompt_decisions(countries, ask))
    return write_policy(policy, policy_path)
