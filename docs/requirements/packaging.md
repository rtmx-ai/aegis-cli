# Requirement Specification — First-class packaging (`apt` / `brew`)

**Thread:** `REL-005..007` · **Companions:** `docs/requirements/release-packaging.md`
(BUILD-002..009), `REL-001/002`. Status: PLANNED.

## 1. Why — the gap the intent-bench integration exposed

Today `aegis` is a single Go binary, but the **running system is a process group**: aegis +
its bundled OpenCode + ripgrep + config-seed (+ llama-server + a side-loaded model). The
binary finds those helpers only **alongside its own exe** or **cwd-relative
`deploy/opencode/...`**. So:

- The shipped **`.deb` installs only `/usr/bin/aegis`** (BUILD-008) — *not* opencode/rg/seed.
  `apt install aegis` would yield an aegis that immediately fails with *"OpenCode is not
  installed or bundled"* — exactly the error the intent-bench agent wrapper hit, which it had
  to work around by `cd`-ing into the source tree.
- There is **no apt repository and no Homebrew tap**, so neither `apt install aegis` nor
  `brew install rtmx-ai/tap/aegis` is possible at all.

The goal: `aegis` installs as a **first-class package that just works on `PATH`**.

## 2. Architecture decision — package the harness, side-load the model

The model GGUF is 14–18 GB — it cannot live in an apt/brew package, and (air-gap)
must be side-loaded anyway. So the package boundary is:

- **In the package** (~200 MB): `aegis` + OpenCode + ripgrep + config-seed + `llama-server`.
  Installed to a standard layout (Linux: `/usr/bin/aegis` + `/usr/lib/aegis/{opencode,rg,
  oc-config,llama-server}`; Homebrew: `bin/aegis` + `libexec/`).
- **Side-loaded, not packaged**: the model GGUF (the existing `stage-model.sh` / catalog
  flow) and the operator's `origin-policy.json` / calibration. The package ships sane
  defaults; the model is acquired per the air-gap procedure.

## 3. Requirements

### REQ-REL-005 — Install-path helper resolution
**aegis shall** resolve its bundled helpers (OpenCode, ripgrep, config-seed, llama-server)
from a **package install layout** — the libexec dir next to the install prefix
(`<prefix>/lib/aegis/` for `<prefix>/bin/aegis`), overridable by `AEGIS_LIBEXEC` — in
addition to the existing alongside-exe / cwd-relative search. *Target:* a packaged
`/usr/bin/aegis` finds `/usr/lib/aegis/opencode` etc. with no cwd gymnastics; the
intent-bench wrapper drops its `cd`/`AEGIS_ROOT` workaround. *Test:*
`internal/opencode::TestResolveFromLibexec`.

### REQ-REL-006 — Complete, working package (deb + Homebrew formula)
**The release shall** produce a package that bundles the whole harness into the standard
layout: the `.deb` installs `aegis` + the libexec helpers (not just the binary), and a
Homebrew formula (`Formula/aegis.rb`) installs `bin/aegis` + `libexec/`. *Target:*
`apt install ./aegis_<v>_<arch>.deb` (or `brew install` from the tap) yields an `aegis` that
passes `aegis verify-env --check-opencode` with no extra setup. *Test:*
`test::TestDebBundlesHarness` (inspects the built `.deb` contents) + `scripts` formula lint.
*Depends on:* `REQ-REL-005`.

### REQ-REL-007 — Distribution channels (apt repo + Homebrew tap)
**aegis shall** be installable from public channels: a signed apt repository (**GitHub
Pages-hosted** `deb` repo — self-contained, no Launchpad/PPA) and a Homebrew tap (`github.com/rtmx-ai/homebrew-tap`,
`brew install rtmx-ai/tap/aegis`), both serving the signed packages from `REL-001`. *Target:*
documented `apt`/`brew` install one-liners resolve + install a working aegis. *Test:*
`test::TestTapFormulaPinned` (the tap formula references the released version + sha256).
*Depends on:* `REQ-REL-006`, `REQ-REL-001`.

## 4. Notes

- This is **out-of-enclave distribution** (apt/brew need network) — for the connected build/
  install host. The enclave still receives a side-loaded, signed bundle (BUILD-009 /
  REL-002). apt/brew is the *convenience* path for non-enclave users + the build host.
- REL-005 is foundational and also **removes the intent-bench wrapper's `cd` workaround** —
  do it first.
