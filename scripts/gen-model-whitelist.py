#!/usr/bin/env python3
"""gen-model-whitelist.py — derive the OpenCode model whitelist from aegis's model policy.

Reads deploy/models/catalog.json + deploy/models/origin-policy.json and emits
deploy/opencode/models-whitelist.json (the OpenCode models.dev catalog format) containing ONLY
origin-ALLOWED, approved models — so the baked catalog (OC-012, MODELS_DEV_API_JSON) can never
include a model from a denied origin (e.g. a PRC-origin model under a controlled policy). This is
OC-013: the whitelist is governed by the same MODEL-006/007 origin policy as the model bundle.

Usage: gen-model-whitelist.py [catalog.json] [origin-policy.json] [out.json]
"""
import json
import sys

cat_path = sys.argv[1] if len(sys.argv) > 1 else "deploy/models/catalog.json"
pol_path = sys.argv[2] if len(sys.argv) > 2 else "deploy/models/origin-policy.json"
out_path = sys.argv[3] if len(sys.argv) > 3 else "deploy/opencode/models-whitelist.json"

cat = json.load(open(cat_path))
pol = json.load(open(pol_path))
models = cat["models"] if isinstance(cat.get("models"), list) else list(cat.get("models", {}).values())


def allowed(origin: str) -> bool:
    return pol.get("countries", {}).get(origin, pol.get("default", "deny")) == "allow"


approved = {}
for m in models:
    if not allowed(m.get("origin", "")):
        continue
    ctx = ((m.get("tuning") or {}).get("num_ctx")) or 32768
    approved[m["id"]] = {
        "id": m["id"],
        "name": m.get("name", m["id"]),
        "release_date": m.get("release_date", "2026-01-01"),
        "attachment": False,
        "reasoning": False,
        "temperature": True,
        "tool_call": True,
        "limit": {"context": ctx, "output": 8192},
    }

# One local, air-gapped provider carrying the approved models (no cloud providers, ever).
whitelist = {}
if approved:
    whitelist["aegis"] = {"id": "aegis", "name": "aegis (local, air-gapped)", "env": [], "models": approved}

json.dump(whitelist, open(out_path, "w"), indent=2)
open(out_path, "a").write("\n")
print(f"gen-model-whitelist: {len(approved)} origin-approved model(s) -> {out_path}: {', '.join(approved) or '(none)'}", file=sys.stderr)
