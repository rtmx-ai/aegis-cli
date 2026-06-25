# Requirement Specification — Expose aegis on rtmx.ai (SITE-001..003)

**Thread:** `SITE-001..003` · **Phase 9 / sprint v1.0** · Status: PLANNED
**Cross-repo:** the implementation lands in `rtmx-ai/rtmx.ai` (a PR); this repo
provides the canonical assets the site references.

## 0. How rtmx is exposed on rtmx.ai (the pattern to mirror)

`rtmx.ai` is an **Astro** site. rtmx (the tool) is surfaced by:

- a **git submodule** — `.gitmodules`: `rtmx → https://github.com/rtmx-ai/rtmx.git`,
- a **docs collection** — `src/content/docs/` (rendered pages) backed by the repo's
  own docs,
- **pages** — `src/pages/` (index, pricing, roadmap, about, _enterprise, _security),
- **downloads** — release binaries linked from the site (GitHub Releases), with
  marketing assets in `public/`.

aegis should appear **the same way**: a submodule, a docs section, a landing/nav
entry, and download references to its signed releases.

## 1. Requirements

- **SITE-001 — aegis-side assets (this repo).** aegis-cli exposes a stable,
  site-consumable entry: a one-paragraph product description + a docs entry point
  (this README + `docs/`) the site can reference, so the rtmx.ai PR *references*
  aegis rather than duplicating its content. Verifiable here.
- **SITE-002 — rtmx.ai PR: submodule + docs.** A PR to `rtmx-ai/rtmx.ai` adds
  `aegis-cli` as a git submodule and an aegis **docs collection entry** +
  nav/landing link, mirroring rtmx's submodule + `src/content/docs` pattern.
  Closed when the PR merges (Manual).
- **SITE-003 — rtmx.ai PR: download refs (gated).** The site's aegis section links
  **download references to signed aegis releases** (the `REL-002` artifacts +
  `aegis.pub`/minisign verification line). Gated on a real signed release existing.
  Closed when the PR merges (Manual).

## 2. The PR (what lands in rtmx.ai)

1. `.gitmodules` + submodule: `aegis-cli → https://github.com/rtmx-ai/aegis-cli.git`.
2. `src/content/docs/aegis/` — an overview page sourced from aegis-cli's README /
   `docs/` (architecture, the three non-negotiables, setup, run verbs).
3. A nav/landing entry alongside rtmx (and a card on the index page).
4. A **Downloads** block: links to the latest signed `aegis` release per target
   (`linux-cpu`, `darwin-metal`) + the `minisign` verify command and public key.

## 3. Dependencies + gating

- SITE-001 → none new (the assets exist in this repo).
- SITE-002 → SITE-001 (references the assets).
- SITE-003 → SITE-002 **and** `REL-002` (a signed v1.0 release to point at). Until
  REL-002 lands, the site can show "coming soon" / build-from-source instructions.

## 4. Exit criteria

SITE-001 COMPLETE here (`rtmx verify`); SITE-002/003 close when the rtmx.ai PR
merges. The aegis page on rtmx.ai renders from the submodule, and Downloads point
to verifiable signed releases (post-REL-002).
