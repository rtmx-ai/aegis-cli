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
Resolves the "gemma4-qat:32k shows as local but isn't used" inconsistency. **Verify:** `cmd/aegis::TestOllamaUsableCandidate`. **Deps:** OC-036.

## REQ-OC-044 — Explain why provisioning failed
The provisioning screen's failure state shows the actual error (sha256 mismatch, no network, disk full,
no host-fit) — the last error line from `aegis provision` — not a bare "Provisioning failed." **Verify:** `test::TestHarnessProvisionFailureReason`. **Deps:** OC-038.

## REQ-OC-045 — Model discovery (find compatible models already on the machine)
`aegis provision --find [filter]` does a broader filesystem scan ON REQUEST and lists usable models
(GGUF on disk + Ollama), so the operator can connect to one rather than download. The default launch
path stays cheap (configured dir only); the deep scan is explicit. **Verify:** `cmd/aegis::TestProvisionFind`. **Deps:** OC-024.

## REQ-OC-046 — Prefer the best already-available model (one-keypress, surfaced)
On launch, aegis surfaces the best already-available model (a GGUF in the configured dir + a working
Ollama model) on the provisioning screen for one-keypress use — recommending the dedicated download as
the alternative; download is the last resort. An available-but-unverified model is surfaced for
explicit consent, never silently auto-connected. **Verify:** `cmd/aegis::TestBestAvailableModel`. **Deps:** OC-043.

## REQ-OC-047 — One-keypress connect to a surfaced available model
OC-046 surfaces the best already-available model; OC-047 adds the connect mechanic — a keypress that
serves a GGUF via `--browse` on the local endpoint, or relaunches opencode pointed at the Ollama
endpoint for an Ollama tag (opencode's backend can't be repointed live). Deferred from OC-046, which
surfaces it. **Verify:** `cmd/aegis::TestConnectAvailable`. **Deps:** OC-046.
