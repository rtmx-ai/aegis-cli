# Hardware Purchase Spec — aegis-cli inference laptop

Procurement reference for a laptop that **holds/processes ITAR-controlled data** and runs
the local-model stack (llama.cpp + harness + rtmx). Tiers are Minimum / Good / Best,
optimized for single-stream tokens/sec and responsiveness, with each tier pushed to the
largest model it runs responsively.

> Compliance note: this is performance engineering, not a compliance ruling. A machine
> that processes ITAR technical data is itself a controlled item (full-disk encryption,
> US-person-only access, device/inventory control, stays in-country; portability is an
> export-risk vector). Final sourcing, US-person handling, and Technology Control Plan
> sign-off are the security/export-control authority's call. See §ITAR sourcing.

## Selected configuration (decided)

- **Initial build host:** Ryzen 5950X / Ubuntu / 64 GB (CPU inference, llama.cpp `-ngl 0`).
  This is where the stack is built and validated first (`target=linux-cpu`).
- **Production / portable target:** **MacBook Pro 16" M5 Max · 128 GB unified · 2 TB (~$5,200)**
  — macOS "Best" tier below (`target=darwin-metal`, all layers on Metal). This is the
  ITAR-holding machine; see the compliance note above and the ITAR sourcing overlay below.

The serving layer is dual-target by design — one `calibration.json` plus `internal/serving`
drives both — so moving from the Ryzen to the Mac is a re-calibration, not a rewrite.

## Sizing assumptions

- **Model envelope:** our MoE plan — Gemma 4 26B-A4B and Qwen3.6-35B-A3B — plus the
  largest model each tier can also hold ("max that fits"). These MoEs activate only
  ~3–4B params/token, so they **decode like small models**: even Minimum runs them fast.
  Tiers differ mainly by *headroom* — bigger models and longer context before a memory wall.
- **Quantization:** Q4_K_M baseline. 26B-A4B ≈ 14 GB; 35B-A3B ≈ 20 GB; 70B ≈ 40 GB.
- **tok/s figures are projections** for our specific MoE models. Validate on the actual
  unit with `scripts/bench.sh` before accepting a tier — same calibration step the stack
  already uses.

## macOS — MacBook Pro M5 (shipping since March 2026)

Best blend of speed, capacity, and portability. MLX is the fastest backend on Apple
Silicon; llama.cpp/GGUF keeps parity with the air-gapped build.

| Tier | Machine | Memory / bandwidth | Max model | Proj. tok/s (our MoE) | Context | ~Price (USD) |
|---|---|---|---|---|---|---|
| Min | MBP 14" M5 Pro, 48 GB / 1 TB | 307 GB/s | our MoEs + 30–40B dense | ~50–90 | 32–64K | ~$2,700 |
| Good | MBP 16" M5 Max, 64 GB / 1 TB | 614 GB/s | adds 70B Q4 | ~90–140 | 128K | ~$4,000 |
| Best | MBP 16" M5 Max, 128 GB / 2 TB | 614 GB/s | 122B-class MoE (48–79 tok/s) | ~100–150 | full 256K | ~$5,100 |

## Windows

Two philosophies. NVIDIA mobile = highest tok/s when the model fits VRAM, capped at
24 GB (RTX 5090 mobile) / 16 GB (RTX 5080 mobile). Strix Halo = 128 GB unified at
~256 GB/s, fitting models NVIDIA laptops can't, at lower bandwidth.

| Tier | Machine | Memory / bandwidth | Max model | Proj. tok/s (our MoE) | Context | ~Price (USD) |
|---|---|---|---|---|---|---|
| Min | RTX 5080 16 GB laptop (ROG Strix SCAR 16, Lenovo ThinkPad P16, HP ZBook Fury) | 16 GB VRAM | 26B-A4B only | ~70–110 | 16–32K | ~$2,600–3,200 |
| Good | RTX 5090 24 GB laptop (Razer Blade 16, ROG Strix SCAR 18, Lenovo Legion Pro 7i) | 24 GB VRAM | both MoEs; ~35B ceiling | ~80–130 | 64–128K | ~$3,800–4,300 |
| Best | Strix Halo 128 GB (ASUS ROG Flow Z13, HP ZBook Ultra G1a) | 128 GB unified, ~256 GB/s, 96 GB→iGPU | 70B-class + huge context | ~45–75 | full 256K | ~$3,500–5,000 (scarce) |

Tension at "Best": Strix Halo maxes capacity; a 24 GB RTX 5090 is *snappier* on the
26–35B envelope. If you'll never exceed ~35B, the RTX 5090 24 GB is the better-feeling
pick; if you want 70B-class and 256K context, take Strix Halo.

## Linux

Same silicon; most air-gap-friendly and the best stack fit. Strix Halo on Linux exposes
~108 GB to the GPU (vs a smaller fixed carve-out on Windows), making it the strongest
big-model option. Prefer Linux-certified vendors.

| Tier | Machine | Memory / bandwidth | Max model | Proj. tok/s (our MoE) | Context | ~Price (USD) |
|---|---|---|---|---|---|---|
| Min | System76 / Framework 16 / ThinkPad P16, RTX 5080 16 GB | 16 GB VRAM | 26B-A4B | ~70–110 | 16–32K | ~$2,500–3,200 |
| Good | ThinkPad P16 / Dell Precision / System76, RTX 5090 24 GB | 24 GB VRAM | both MoEs; ~35B ceiling | ~85–135 | 64–128K | ~$3,800–4,500 |
| Best | Strix Halo 128 GB on Linux (HP ZBook Ultra G1a, ROG Flow Z13) | 128 GB unified, ~108 GB→GPU | 70B-class + 256K | ~50–80 | full 256K | ~$3,500–5,000 |

For the air-gapped llama.cpp build, **Strix Halo on Linux is the top-tier sweet spot** —
biggest models, most usable memory, cleanest stack match.

## Priced-out configurations (mid-2026 street, USD)

Concrete buildable configs. Windows/Linux split into **Value** (gaming chassis) and
**Workstation** (ITAR-preferred: TPM/vPro, SED options, supply-chain assurance — at a
premium). These supersede the ~ranges in the tier tables above. Verify against your
reseller/contract pricing before purchase.

### macOS (Apple Store)

| Tier | Configuration | Price |
|---|---|---|
| Min | MBP 16" M5 Pro · 48 GB · 1 TB | $3,099 |
| Good | MBP 16" M5 Max (40-core) · 64 GB · 1 TB | ~$4,300 |
| Best | MBP 16" M5 Max · 128 GB · 2 TB | ~$5,200 |

### Windows / Linux — Value (gaming) chassis

| Tier | Configuration | Price |
|---|---|---|
| Min | ROG Strix SCAR 16 / Razer Blade 16 · RTX 5080 16 GB · 32 GB · 2 TB | ~$2,800 |
| Good | Razer Blade 16 / ROG SCAR 18 · RTX 5090 24 GB · 32 GB · 2 TB | ~$3,999 |
| Best | ROG Flow Z13 · Ryzen AI Max+ 395 · 128 GB · 2 TB | ~$3,700 |

### Windows / Linux — Workstation chassis (ITAR-preferred, premium)

| Tier | Configuration | Price |
|---|---|---|
| Min | Lenovo ThinkPad T16g Gen 3 · RTX 5080 16 GB · 1 TB | ~$4,500 |
| Good | Dell Pro Max / Schenker Key 18 · RTX 5090 24 GB | ~$5,400 |
| Best | **HP ZBook Ultra G1a 14 · Ryzen AI Max+ PRO 395 · 128 GB · 2 TB** | ~$4,049 |

**Standout:** the ZBook Ultra G1a 128 GB / 2 TB is workstation-grade (the ITAR-preferred
chassis), capacity-king, and — in the 2 TB config — priced near a gaming laptop. Spec the
2 TB: the 1 TB build is oddly ~$6,445 and 4 TB ~$8,250. Tradeoff: modest AI throughput
(~256 GB/s) — fine for our small-active MoEs (~45–80 tok/s), not fast.

Budget +$200–600/unit for ITAR add-ons (FIPS self-encrypting drive, extended/24×7
warranty, asset tagging). These are single units; volume/GSA pricing will differ.

## Reading the numbers + validation

- tok/s are single-stream projections for our small-active MoEs; treat as a starting
  point. On receipt, run `scripts/bench.sh --model <gguf>` to record the host-tuned
  config and the real decode/prefill numbers (writes `deploy/llama-server/calibration.json`).
- Confirm the machine passes the closed-environment gate (`scripts/verify-airgap.sh`)
  with zero egress before it processes any controlled data.
- Prefer llama.cpp built from source on every platform for a clean air-gap posture
  (Metal on macOS, CUDA on NVIDIA, ROCm/Vulkan on Strix Halo).

## The laptop ceiling

Portable hardware tops out at 24 GB discrete VRAM or ~128 GB unified, and ~130–150 tok/s
on our models. If you later need >128 GB or higher throughput, that is a desktop
workstation — outside this spec.

## ITAR sourcing overlay (engineering input, not a compliance ruling)

- Prefer **workstation-class chassis** (Lenovo ThinkPad P, Dell Precision, HP ZBook) over
  gaming lines for TPM/vPro manageability, FIPS self-encrypting drives, and stronger
  supply-chain assurance — at a price premium, and often via RTX Pro (Blackwell) mobile
  GPUs rather than the GeForce parts named above.
- Lean **Linux** for air-gap hygiene and stack fit.
- AMD markets the Ryzen AI Halo platform as US-available and tested against applicable US
  regulatory requirements; this helps the sourcing conversation but does not substitute
  for your authority's device sourcing, US-person handling, and TCP determination.

## Price volatility

Mid-2026 USD, representative not quoted. The market is in an LPDDR5/high-VRAM price surge:
unified-memory laptops have spiked and Strix Halo laptops are frequently out of stock
(expect +$300–500 over list). Re-verify at purchase.
