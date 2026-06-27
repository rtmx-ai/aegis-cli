# Requirement Specification — Model-origin governance

**Thread:** `MODEL-005..008` · **Phase (model governance)** · Status: PLANNED
**Tracked in:** `.rtmx/database.csv` · **Companion:** `docs/model-compliance.md`, `docs/models.md`
**Follows from:** the bundle default switching to a PRC-origin model (qwen3-coder) — which
removed the fail-safe (controlled work no longer gets a US-origin model by default). This
thread makes model-origin an explicit, enforced, configurable policy rather than a doc note.

## 1. Why

`docs/model-compliance.md` establishes that model **provenance** (country of origin) is a
real compliance axis (889 is narrow, but contract terms / FASCSA / agency rules can bar a
PRC-origin model). Today that is guidance only — nothing stops a controlled bundle from
shipping a denied-origin model. This thread adds: origin **metadata**, a per-country
allow/deny **policy** the operator controls, a build/verify **gate** that enforces it, and an
**init prompt** so the policy is set deliberately at setup.

## 2. Requirements

### REQ-MODEL-005 — Catalog records model origin country
**The catalog shall** record each model's country of origin (ISO-3166 alpha-2, e.g. `US`,
`CN`) so origin is machine-checkable. *Target:* every `deploy/models/catalog.json` model
carries a valid `origin`. *Test:* `test::TestCatalogOriginRecorded`. *Depends on:* `REQ-MODEL-003`.

### REQ-MODEL-006 — Per-country origin policy (the local config)
**aegis shall** load + validate a per-country origin policy from a local config
(`deploy/models/origin-policy.json`; `AEGIS_ORIGIN_POLICY` overrides the path): a `default`
disposition (`allow`/`deny`) for unlisted/unknown origins plus per-country `allow`/`deny`
entries. *Target:* `origin.LoadPolicy` reads + validates it; `Allows(country)` resolves a
country to allow/deny (listed entry wins, else `default`). The shipped default is
**default-deny** with `US` + `CN` allowed (so an *un-classified* origin is rejected until
reviewed, without breaking the current qwen default). *Test:* `internal/origin::TestOriginPolicy`.

### REQ-MODEL-007 — Origin gate (build + verify-env)
**aegis shall** fail when the selected model's (`MODEL_REF`) origin is not policy-allowed:
`aegis verify-env --check-origin` reports + non-zero-exits, and a build gate (`make
origin-gate`, wired into `ci-full`) enforces it. Unknown origin → denied under the
default-deny policy. The policy file is the explicit, auditable override (set a country to
`allow`). *Target:* `origin.CheckModel(modelRef, catalog, policy)` returns a typed denial;
verify-env + the gate honor it. *Test:* `internal/origin::TestOriginGate`. *Depends on:*
`REQ-MODEL-005`, `REQ-MODEL-006`.

### REQ-MODEL-008 — Init prompt writes the origin policy
**Setup shall** prompt the operator for the per-country origin policy at init (which origins
to allow/deny, defaulting to the catalog's countries) and write `origin-policy.json`;
non-interactive runs use the shipped default. *Target:* the setup orchestrator has an
origin-policy step that persists the operator's choice. *Test:*
`scripts/setup::test_origin_policy_prompt`. *Depends on:* `REQ-MODEL-006`.

## 3. Design notes

- **Default is deny-unknown, not deny-CN.** The shipped policy allows `US`+`CN` (the origins
  in use) so it does not break the current qwen default, but denies *un-classified* origins
  — a new model with no/unknown origin is rejected until reviewed. A controlled deployment
  tightens it (set `CN: deny`) via the init prompt or by editing the file; the gate then
  fails for a PRC-origin `MODEL_REF`, forcing the switch to gemma.
- **The policy file is the override.** No magic env bypass — allowing a denied origin is an
  explicit, version-controllable edit to the policy (auditable), per the air-gap ethos.
- **Origin ≠ egress.** This is supply-chain/provenance governance, complementary to the
  GUARD egress gate, not a replacement.

## 4. Exit criteria

All four COMPLETE: catalog carries origins; the policy loads/validates; verify-env + the
build gate enforce it (unknown denied); setup prompts for + persists the policy.
