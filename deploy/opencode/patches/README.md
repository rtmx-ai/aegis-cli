# OpenCode build-time patches (OC-012..018)

`scripts/build-opencode.sh` applies every `*.patch` in this directory over the **pinned**
upstream source (`deploy/opencode/OPENCODE_REF`) after checkout and before the build. This is
how aegis makes the bundled OpenCode air-gap / CUI / ITAR-suitable and rebrands it — a minimal,
reviewable patch set, **not a fork** (CLAUDE.md §1: *build-time patches are not a fork*).

## Contract

- Each patch must apply cleanly (`git apply --check`) to `OPENCODE_REF`. A conflict **fails the
  build loudly** — an upstream bump (OC-008) can never silently drop a control. Re-roll the
  patch against the new pin.
- Patches are `git diff` format, rooted at the opencode source tree (`packages/...`). Generate
  with: `git -C build/opencode-src diff > deploy/opencode/patches/NN-name.patch`.
- Keep patches small + single-purpose; name them `NN-purpose.patch` so apply order is stable.

## Planned patches (the suite)

| Patch | Requirement | Purpose |
|---|---|---|
| `10-model-whitelist.patch` | OC-012/013 | strip the embedded cloud catalog → local whitelist only |
| `20-rebrand-aegis.patch` | OC-014 | app name / TUI title / CLI scriptName / banner / user-agent → aegis |
| `30-docs-aegis.patch` | OC-015 | docs/help reference aegis (offline), not opencode.ai |

(Some controls — e.g. the catalog source — may instead be a build-time env in
`build-opencode.sh` rather than a source patch; the spec records which.)
