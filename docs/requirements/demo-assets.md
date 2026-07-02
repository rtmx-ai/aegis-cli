# Demo assets — README + website (GIF + screenshots)

The README carries only badges and the rtmx.ai aegis page (SITE-002) is text-only — there is no visual
of aegis in action. A product surfaced on a website needs a demo. These requirements close that gap with
**reproducible, air-gap-friendly** assets.

## Approach (recommended: VHS)

Use **[VHS](https://github.com/charmbracelet/vhs)** — a `.tape` script (typed commands + timing) renders
a deterministic GIF/MP4/PNG. It is scriptable, reproducible, and runs locally (no cloud), so the asset is
regenerable and reviewable, and the `.tape` source is committed. asciinema (`.cast`) is the alternative
if we later prefer to feed the site's existing `AnimatedTerminal` / `TerminalCarousel` components from a
recording rather than embed a GIF.

Recording the actual frames needs a working interactive TUI (a live local model), so **asset generation
is M5-side**; the `.tape` script, the committed placeholder, and the README/site embeds are prep-able now.

## Requirements

### DOCS-005 — README demo GIF + screenshot (reproducible)
A committed `.tape` script drives a short aegis session (launch the TUI, drive one rtmx requirement to
green), rendered to a GIF and a static TUI screenshot under `docs/assets/`, and embedded near the top of
`README.md`. The `.tape` source is committed so the asset is regenerable.

**Acceptance criteria**
- `docs/assets/aegis-demo.tape` exists (the deterministic recipe).
- `README.md` embeds the demo GIF and at least one screenshot (`![...](docs/assets/…)`).
- The demo is loopback-only (air-gap doctrine): the recorded session makes no external call.

*Test:* `test::TestReadmeDemoAsset`

### SITE-004 — surface the demo on the rtmx.ai aegis page
The rtmx.ai aegis page (SITE-002) shows the demo — either the DOCS-005 GIF/screenshot embedded, or the
site's `AnimatedTerminal`/`TerminalCarousel` component fed from the same `.tape` recording. Depends on
SITE-002 (the page) and DOCS-005 (the asset).

**Acceptance criteria**
- The aegis page references the demo asset (image embed or terminal component).
- The asset is served from the site (no hotlink to an external host).

*Test:* `test::TestSiteDemoAsset`

## Non-goals
- Marketing video production — a short scripted terminal demo is enough.
- Per-release re-recording automation — regenerate on meaningful UX changes, not every release.
