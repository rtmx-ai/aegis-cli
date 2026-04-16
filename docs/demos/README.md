# aegis-cli demo GIFs

This directory holds terminal demo recordings used in the top-level README
and in marketing materials. Every GIF is driven by a
[VHS](https://github.com/charmbracelet/vhs) tape script under `tapes/`, so
the recordings are reproducible and reviewable in code review.

Requirements tracked here:

- REQ-BUILD-014: VHS tape scripts for terminal demo GIF generation
- REQ-BUILD-017: Hero demo tape (`tapes/hero.tape`)
- REQ-BUILD-018: HITL approval flow demo tape (`tapes/hitl-approval.tape`)

## Why VHS

- Deterministic: the same tape produces the same frames, so changes to a
  GIF show up as a clear diff in the tape rather than an opaque binary
  churn.
- Scriptable: tapes are plain text; they live next to the code and are
  reviewed together with the features they advertise.
- In-repo: no SaaS recorder, no manual screen capture, no
  platform-specific tooling beyond a single static binary.

## Layout

```
docs/demos/
  README.md              -- this file
  hero.gif               -- rendered from tapes/hero.gif (generated)
  hitl-approval.gif      -- rendered from tapes/hitl-approval.tape (generated)
  ...additional *.gif    -- one per tape (generated)
  dev-loop.gif           -- real tmux/Claude recording (generated manually)
  tapes/
    hero.tape            -- REQ-BUILD-017
    hitl-approval.tape   -- REQ-BUILD-018
    01-hero.tape         -- legacy numbered tapes kept for reference
    02-hitl-approval.tape
    03-airgapped.tape
    04-audit-ledger.tape
    05-plugin-provision.tape
    06-aegisignore.tape
    dev-loop.tape        -- records a real aegis session; regen manually
```

The `*.gif` files are generated artifacts. They may be committed to the
repository so that GitHub can render them in README previews without
pulling LFS.

## Prerequisites

Install VHS:

```bash
# macOS
brew install vhs

# Linux / other platforms
#   See https://github.com/charmbracelet/vhs#installation
```

VHS needs `ttyd` and `ffmpeg` on the PATH; the Homebrew formula pulls
these in automatically.

## Regenerating the GIFs

From the repository root:

```bash
./scripts/regen-demos.sh
```

This walks every tape in `docs/demos/tapes/`, invokes `vhs` on it, and
writes the output GIF to the location declared in the tape's `Output`
directive (typically `docs/demos/<name>.gif`). The `dev-loop` tape is
skipped because it records a real tmux + Claude session and consumes API
tokens; regenerate that one manually when `scripts/dev.sh` changes:

```bash
vhs docs/demos/tapes/dev-loop.tape
```

## Verifying the tapes (no VHS required)

A fast, CI-friendly syntactic check:

```bash
./scripts/verify-tapes.sh
```

This confirms that:

- `docs/demos/tapes/` exists and holds at least two tapes.
- Every tape declares an `Output` directive and at least one action
  (`Type`, `Sleep`, or `Enter`).
- `hero.tape` and `hitl-approval.tape` exist at the paths referenced by
  REQ-BUILD-017 and REQ-BUILD-018, with their `rtmx:req` markers intact.

The verify script does not invoke `vhs`, so it works in air-gapped CI
runners that only ship Bash and coreutils.

## Adding a new tape

1. Pick a short, kebab-case name (e.g. `streaming-response`).
2. Create `docs/demos/tapes/<name>.tape`.
3. Declare `Output docs/demos/<name>.gif` near the top.
4. Set `Width`, `Height`, `FontSize`, `Theme`, `TypingSpeed`, and
   `Padding` to match the other tapes (1200x700, FontSize 14,
   Catppuccin Mocha) unless the demo genuinely needs different sizing.
5. Reference fictional file paths in prompts so the tape does not depend
   on any specific repo state.
6. If the tape covers a specific requirement, add a top-of-file comment
   of the form `# rtmx:req REQ-XXX-NNN` so traceability tooling can link
   the tape to its requirement.
7. Run `./scripts/verify-tapes.sh` to confirm the tape is well-formed.
8. Run `./scripts/regen-demos.sh` (or `vhs docs/demos/tapes/<name>.tape`)
   to produce the GIF and eyeball the result.
9. Commit both the `.tape` and the rendered `.gif` together.

## Tape authoring conventions

- Keep each tape between 25 and 60 seconds of wall time. Anything longer
  becomes a heavyweight GIF that GitHub will not inline cleanly.
- Prefer `Sleep 2s`/`Sleep 3s` breaks between logical beats over faster
  pacing; viewers need a moment to read the TUI.
- Do not reference real internal hostnames, customer names, or
  production paths. If a demo needs an IaC plugin, use
  `gcp-assured-workloads` (the reference plugin in
  `@aegis/infra-sdk`).
- The only fonts/themes we ship as defaults are the VHS built-ins; avoid
  custom fonts so regeneration works on any developer laptop and in CI.
