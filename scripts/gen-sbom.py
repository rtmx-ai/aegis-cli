#!/usr/bin/env python3
"""gen-sbom.py — emit a CycloneDX SBOM from the vendored Go module set (BUILD-003).

Reads `go list -m -json all` output (a stream of concatenated JSON objects) and
writes a minimal CycloneDX 1.5 JSON SBOM to stdout. No third-party deps — the
SBOM describes the BUILD inputs (the shipped binary is std-lib-only; the modules
are test-only/vendored, but the supply-chain record must still cover them).

Usage: gen-sbom.py <modules.json> <version>
"""
import json
import sys


def load_modules(path):
    with open(path) as f:
        text = f.read()
    dec = json.JSONDecoder()
    mods, i, n = [], 0, len(text)
    while i < n:
        while i < n and text[i].isspace():
            i += 1
        if i >= n:
            break
        obj, end = dec.raw_decode(text, i)
        mods.append(obj)
        i = end
    return mods


def main():
    if len(sys.argv) != 3:
        sys.stderr.write("usage: gen-sbom.py <modules.json> <version>\n")
        return 2
    mods = load_modules(sys.argv[1])
    version = sys.argv[2]
    components = []
    for m in mods:
        path = m.get("Path")
        if not path or m.get("Main"):
            continue  # skip the main module
        ver = m.get("Version", "")
        components.append({
            "type": "library",
            "name": path,
            "version": ver,
            "purl": "pkg:golang/%s@%s" % (path, ver) if ver else "pkg:golang/%s" % path,
        })
    bom = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "component": {
                "type": "application",
                "name": "aegis-cli",
                "version": version,
            },
        },
        "components": components,
    }
    json.dump(bom, sys.stdout, indent=2)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
