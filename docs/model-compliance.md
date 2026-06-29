# Model provenance + compliance posture

How to think about model provenance — the bundle default is the US-origin model
(Gemma); a PRC-origin model (Qwen) is opt-in only — in a defense/ITAR context.
Companion to [`docs/models.md`](models.md).

> **Not legal advice.** Compliance is contract- and agency-specific, and the rules on
> Chinese-origin AI are moving fast. Treat this as engineering context to raise with the
> contracting officer / counsel / security lead — get the determination in writing for the
> specific program. Last reviewed 2026-06-27.

## Does Section 889 bar Qwen (or other Chinese open-weight models)?

**No — not by its own terms.** Section 889 of the FY2019 NDAA is narrow and
named-entity-based. "Covered telecommunications equipment or services" means telecom /
video-surveillance **equipment and services** from five specifically named companies:
**Huawei, ZTE, Hytera, Hikvision, Dahua** (the last three only when used for
security/surveillance purposes). It does not reach Qwen because:

1. **Alibaba is not one of the five named entities.** 889 is a denylist of specific
   companies, not a general "no Chinese products" rule.
2. **An open-weight model is not "telecommunications equipment" or a "video surveillance
   service."** Model weights are software/data — not telecom hardware or a telecom service —
   so the covered category does not fit.

Neither prong of 889 — (a) cannot *procure* covered gear, (b) cannot *use* covered gear —
is triggered by running a local model.

## The risk is real — it just lives under other (newer) authorities

889 is the wrong instrument to rely on here. What actually governs PRC-origin AI in
federal/defense work is newer and broader:

- **DeepSeek-specific prohibitions.** The *No DeepSeek on Government Devices Act*
  (HR 1121, Feb 2025) and a bipartisan Senate bill (Cassidy/Rosen) would bar federal
  **contractors** from using DeepSeek for contract work, extending to **successor models
  from the same developer**. DoD/Pentagon were early movers banning it from devices. These
  currently name **DeepSeek specifically — not Qwen** — but the trajectory is toward
  broader "Chinese-origin AI" restrictions, and the FAR Council could extend prohibitions
  to all contractors.
- **Broader supply-chain levers** that *can* reach a model: **FASCSA** exclusion orders
  (Federal Acquisition Supply Chain Security Act) and the **NDAA §1260H** "Chinese military
  companies" list — both wider than 889.
- **Contract / agency terms.** A given program can impose country-of-origin or
  prohibited-source terms stricter than any statute. This is usually the binding constraint.

## What the air-gap architecture does — and does not — mitigate

A major driver of the DeepSeek bans is **data exfiltration** (the hosted service sends user
data back to China). That concern is about the **hosted API/app**.

aegis runs **open weights locally, air-gapped, loopback-only, egress = build-failing**. The
weights are inert; running them in the enclave sends nothing to the model's authors. So the
headline exfiltration risk is **architecturally mitigated** here.

What does **not** go away:
- **Provenance / integrity** — weights could in principle be backdoored, poisoned, or
  trained to behave adversarially (harder to detect than a network call). Pin + verify the
  GGUF digest (`MODEL_REF`, `catalog.json`, `TestModelPinsConcrete`) so at least the *bits*
  are the audited ones.
- **Policy / optics / contract terms** — many programs will decline a PRC-origin model
  regardless of how it is run.

## Posture (US-origin default; non-US is opt-in)

- **`MODEL_REF` defaults to Gemma-4-26B-A4B (US-origin, Google)** — the provenance-safe default
  for a defense/ITAR posture, and the SERVE-016 capability winner. A controlled deployment gets a
  US-origin model with no extra step.
- **The default fails safe.** The shipped origin policy is **US-only**: CN (e.g. Qwen/DeepSeek)
  and every other non-US origin are denied unless the operator explicitly opts in. The bundle
  default and the policy agree — there is no PRC-origin model anywhere in the default path
  (model, gate, *or* the OpenCode picker whitelist, OC-013).
- **Qwen3-Coder-30B-A3B (PRC-origin, Alibaba) is opt-in only** — the SERVE-016 agentic-capability
  pick, available for development / non-controlled work, but only after a deliberate, auditable
  choice: add `"CN": "allow"` to `origin-policy.json` **and** pin it (`scripts/pin-model.sh
  ~/models/Qwen…gguf`). We asked the Qwen/DeepSeek questions to map the ITAR / supply-chain
  envelope; allowing a Chinese-origin model is never the default.
- **Rule of thumb:** a non-US-origin model is a compliance item to **clear explicitly per
  contract** before opting in — not something 889 settles.

## Derived / fine-tuned models — origin follows the base, not the publisher

Provenance tracks the **base-model lineage**, not the Hugging Face uploader or the company that did
the fine-tuning. A US-based lab post-training a PRC base model does **not** launder the weights'
origin — the result carries the base's provenance and supply-chain envelope.

**Worked example — DeepReinforce Ornith-1.0 (Jun 2026).** An MIT-licensed, agentic-coding model
family from a Santa Clara company (DeepReinforce, founder ex-Shannon.AI). Its self-scaffolding RL is a
US contribution, but the **published GGUF weights (9B, 35B) are post-trained on Qwen 3.5** (Alibaba,
PRC) → **CN lineage → opt-in only**, exactly like raw Qwen. The only announced US-lineage member (a
31B on a Gemma 4 base) is **not published**, so there is no US-origin Ornith to bundle. A US wrapper
around PRC weights stays CN.

**Rule:** classify a catalog model's `origin` by its **base model's** country. A fine-tune inherits
the base's disposition under the origin policy — `internal/origin` gates the result, not the uploader.

## Enforcement (MODEL-005..008)

The posture above is **enforced**, not just documented:

- **Origin metadata** — every catalog model records an ISO `origin` (`deploy/models/catalog.json`).
- **Policy** — a per-country allow/deny file (`deploy/models/origin-policy.json`,
  `AEGIS_ORIGIN_POLICY`-overridable) that the operator controls. Shipped **default-deny** with
  only `US` allowed: CN and every other non-US origin are rejected until the operator opts in.
  Allowing a denied origin is an explicit, version-controllable edit — no env bypass.
- **Gate** — `aegis verify-env --check-origin` and `make origin-gate` (wired into `ci-full`)
  fail when the pinned model's (`MODEL_REF`) origin is not allowed. The default ships gemma (US)
  and passes; opting in to a CN model means adding `"CN": "allow"` AND pinning it — a deliberate,
  gated, auditable choice, never the default.
- **Init prompt** — `setup` asks per-country (the catalog's origins) and writes the policy at
  init; non-interactive runs keep the shipped default.

Spec: `docs/requirements/model-origin-governance.md`.

## Sources

- [What is Section 889 of the FY2019 NDAA? — U.S. Election Assistance Commission](https://www.eac.gov/what-section-889-fy-2019-ndaa)
- [NDAA Section 889 Rule on Huawei, ZTE and Video Companies — Wiley](https://www.wiley.law/alert-Long-Awaited-Controversial-NDAA-Section-889-Rule-on-Huawei-ZTE-and-Video-Companies-Emerges-from-FAR-Council)
- [What Federal Contractors Need to Know about Section 889 — Coalition for Government Procurement](https://thecgp.org/current-issues/cybersecurity/what-federal-contractors-need-to-know-about-section-889/)
- [Senators move to quash use of Chinese AI by federal contractors — CyberScoop](https://cyberscoop.com/deepseek-ban-congress-cassidy-rosen-contractors/)
- [U.S. Federal and State Governments Moving Quickly to Restrict Use of DeepSeek — Inside Government Contracts](https://www.insidegovernmentcontracts.com/2025/02/u-s-federal-and-states-governments-moving-quickly-to-restrict-use-of-deepseek/)
- [US bans Chinese AI LLM DeepSeek from government devices — Digital Watch Observatory](https://dig.watch/updates/us-bans-chinese-ai-llm-deepseek-from-government-devices)
- [House Select Committee on the CCP — DeepSeek report (PDF)](https://chinaselectcommittee.house.gov/sites/evo-subsites/selectcommitteeontheccp.house.gov/files/evo-media-document/DeepSeek%20Final.pdf)
