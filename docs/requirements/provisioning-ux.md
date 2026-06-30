# Provisioning UX requirements (OC-038..043)

Operator feedback after the v1.3.8 provisioning screen landed (2026-06-30). Decisions captured inline.

## REQ-OC-038 — Operator-initiated provisioning (no auto-download)
The no-model screen shows the recommended model, its size, and the download URL, and **waits for a
keypress** to begin — it does not auto-download (reverses OC-034's auto-start). Safe on a metered/
hotspot connection. **Verify:** `test::TestHarnessManualProvision`. **Deps:** OC-034.

## REQ-OC-039 — Show the download source URL
The provisioning screen displays the URL the model is fetched from (aegis passes it via env), so the
operator sees exactly where bytes come from before consenting. **Verify:** `test::TestHarnessShowsModelURL`. **Deps:** OC-038.

## REQ-OC-040 — Configurable model garden (endpoint override + custom catalog)
Operator can point aegis at an enterprise-trusted source two ways: (a) an **endpoint override**
(AEGIS_MODEL_GARDEN env / config) that rewrites the catalog's download host while keeping the pinned
filenames + SHA256s (integrity unchanged); and (b) a **custom catalog** path (AEGIS_CATALOG env /
config) pointing at an operator-supplied catalog.json. **Verify:** `cmd/aegis::TestModelGardenOverride`. **Deps:** OC-033.

## REQ-OC-041 — Progress + ETA during model startup
After the download, while the model loads/serves, the screen shows a spinner/progress with an ETA
(not a frozen "starting…"). **Verify:** `test::TestHarnessStartupProgress`. **Deps:** OC-034.

## REQ-OC-042 — Explain the recommendation
The provisioning screen states WHY the recommended (larger) model is recommended — capability,
host-fit, US-origin — versus the operator's local Ollama models. **Verify:** `cmd/aegis::TestRecommendationRationale`. **Deps:** OC-024.

## REQ-OC-043 — Offer a working Ollama model as a "use now" stopgap
aegis iterates past a crashing Ollama model (OC-036) to find a working one and surfaces it on the
provisioning screen with a key to use it immediately, while still recommending the dedicated download.
Resolves the "gemma4-qat:32k shows as local but isn't used" inconsistency. **Verify:** `cmd/aegis::TestOllamaUsableCandidate`. **Deps:** OC-036, OC-038.
