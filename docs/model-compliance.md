# Model provenance + compliance posture

How to think about model provenance — the current bundle default is the PRC-origin model
(Qwen), with a US-origin alternative (gemma) one command away — in a defense/ITAR context.
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

## Posture (bundle default + the controlled-work switch)

- **`MODEL_REF` defaults to Qwen3-Coder-30B-A3B (PRC-origin, Alibaba)** — chosen for agentic
  capability (the SERVE-016 forward pick: purpose-built for tool use, non-thinking). This is
  the right default for development and non-controlled work.
- **Gemma-4-26B-A4B (US-origin, Google) is the provenance-safe switch** for controlled/ITAR
  work — one command away (`scripts/pin-model.sh ~/models/gemma-…gguf`), and also the
  SERVE-016 capability winner. See [`docs/models.md`](models.md).
- **The default does not fail safe.** Because the bundle default is now PRC-origin, a
  controlled deployment must switch to gemma **explicitly** — it no longer gets a US-origin
  model by default. Put that switch on the deployment checklist for any controlled program.
- **Rule of thumb:** treat a Chinese-origin model as a compliance item to **clear explicitly
  per contract**, not something 889 settles; switch to the non-PRC model (gemma) for
  controlled work.

## Sources

- [What is Section 889 of the FY2019 NDAA? — U.S. Election Assistance Commission](https://www.eac.gov/what-section-889-fy-2019-ndaa)
- [NDAA Section 889 Rule on Huawei, ZTE and Video Companies — Wiley](https://www.wiley.law/alert-Long-Awaited-Controversial-NDAA-Section-889-Rule-on-Huawei-ZTE-and-Video-Companies-Emerges-from-FAR-Council)
- [What Federal Contractors Need to Know about Section 889 — Coalition for Government Procurement](https://thecgp.org/current-issues/cybersecurity/what-federal-contractors-need-to-know-about-section-889/)
- [Senators move to quash use of Chinese AI by federal contractors — CyberScoop](https://cyberscoop.com/deepseek-ban-congress-cassidy-rosen-contractors/)
- [U.S. Federal and State Governments Moving Quickly to Restrict Use of DeepSeek — Inside Government Contracts](https://www.insidegovernmentcontracts.com/2025/02/u-s-federal-and-states-governments-moving-quickly-to-restrict-use-of-deepseek/)
- [US bans Chinese AI LLM DeepSeek from government devices — Digital Watch Observatory](https://dig.watch/updates/us-bans-chinese-ai-llm-deepseek-from-government-devices)
- [House Select Committee on the CCP — DeepSeek report (PDF)](https://chinaselectcommittee.house.gov/sites/evo-subsites/selectcommitteeontheccp.house.gov/files/evo-media-document/DeepSeek%20Final.pdf)
