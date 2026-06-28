# Business case — local aegis-cli stack vs. GovCloud frontier

One-pager for leadership. Bottom line: for an ITAR workload at our token volume, a
one-time ~$5,200 device recovers its cost in **weeks to two months** versus any
ITAR-authorized cloud path, then runs at **zero marginal cost** while keeping controlled
data physically on-device.

## The decision

Build the agentic coding stack (aegis-cli + local MoE + rtmx) on hardware we own:
initial build on an existing Ryzen 5950X workstation, production on a MacBook Pro 16"
M5 Max (128 GB, ~$5,200). The alternative for ITAR data is an authorized GovCloud
frontier service — AWS GovCloud Bedrock or Azure Government AI Foundry.

## The reframe: "GovCloud frontier" isn't frontier

The premise that we'd be giving up frontier quality is weaker than it looks, because the
frontier models aren't in these environments:

- **Azure Government** tops out around GPT-4.1 / o3-mini — not GPT-5.5.
- **AWS GovCloud Bedrock** offers Claude Sonnet 4.5 — not Opus.

So the real comparison is "a local 26–35B MoE we own" vs. "a ~two-generations-behind
model we rent, metered, at a 20–30% government premium plus environment/ATO overhead." We
are not paying a premium for the frontier; we're paying it for a non-frontier model we
don't control.

## Break-even

Assumptions: device $5,200 one-time (+~$50/yr power); cloud baseline = AWS GovCloud
Bedrock Claude Sonnet 4.5 (commercial $3/$15 per M tokens + ~25% GovCloud premium), a
conservative ~$7.50/M blended effective rate for input-heavy agentic work. Agentic loops
amplify token consumption 5–8×, which inflates the cloud bill — not the local cost, which
is $0 per token.

| Monthly tokens | GovCloud cost / mo | Break-even on the device | Year-1 cloud spend |
|---|---|---|---|
| 100M | ~$750 | ~7 months | ~$9,000 |
| 300M | ~$2,250 | ~2.3 months | ~$27,000 |
| 1B | ~$7,500 | ~3 weeks | ~$90,000 |

The device pays for itself at ~600–700M cumulative tokens. Our historical usage (top
token consumer at two companies, long-running requirement chains) puts us in the
300M–1B+/month band — **break-even in weeks to two months**, then free forever while the
cloud meter would keep running every month after. Cited real-world agentic bills run
higher than this table, so actual break-even is likely faster.

## The experience trade

- **Responsiveness:** a win. On the M5 Max (614 GB/s) our small-active MoEs run
  ~100–150 tok/s with full context resident, no network round-trip, no throttling, and —
  for a high-rate user — no meter running. We stop rationing tokens.
- **Per-task quality on the hardest chains:** the honest cost. Sonnet 4.5 beats our local
  MoE on long-horizon autonomous work. This is mitigated by the architecture (rtmx scopes
  one tested requirement at a time; the verify loop catches failures) and by keeping a
  cloud escalation path for the hardest non-ITAR pieces. We sell "good-enough-on-the-bulk,
  owned, fast, free per token" — not "same quality."

## Honest caveats (so the case survives scrutiny)

- The device is **single-user**. If this becomes a shared team service with concurrency
  and SLAs, the comparison shifts to a small on-prem server, not a laptop.
- Exact GovCloud pricing needs a real quote (much is EA/contact-sales; Azure adds 20–40%
  in support/networking/infra on top of tokens).
- The quality gap on the hardest work is real — the pitch is "own the bulk locally, rent
  the hard escalations," not "replace frontier."

## Recommendation

Own the bulk locally; reserve a GovCloud escalation path for the hardest non-ITAR work.
The local stack wins decisively on cost and data control for our volume, and the cloud
alternative isn't even buying us the frontier. Approve the device; revisit only if this
becomes a multi-user service.
