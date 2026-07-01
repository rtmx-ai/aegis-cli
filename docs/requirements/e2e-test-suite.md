# E2E test suite — performance, accuracy, precision, egress/security

An end-to-end suite that exercises the *whole* aegis loop (local model → OpenCode harness → rtmx verify)
on the golden set, across four axes, entirely offline. Every tool below runs air-gapped (no cloud, no
hosted service) — a hard requirement, since the suite must pass inside the same closed enclave aegis
ships into. Extends the existing METRIC (golden-set metrics), GUARD (egress default-deny +
verify-airgap.sh) and BENCH work rather than replacing it.

## The four axes and the OSS tool suite (all air-gap-safe / offline)

| Axis | What it measures | OSS tools to leverage | Already in repo |
|---|---|---|---|
| **Performance** | tok/s per target, prefill/decode split, WCR (wall-clock/req), TCR (tokens/req), latency percentiles | **hyperfine** (CLI timing), `scripts/bench.sh` (thread/batch tok-s sweep), the `internal/metrics` pipeline; optional **k6/vegeta** for `opencode serve` API load | bench.sh, ci-metrics.py, per-stage timing (METRIC-003) |
| **Accuracy** | ACR (autonomous completion rate), pass@1 — closed-by-verify ÷ attempted, no human step | **SWE-bench / SWE-bench-Verified** harness run *offline* (pre-fetched task images) as a periodic bench; **terminal-bench** (optional); the frozen `eval/golden` set as the always-on core; **pytest** as the runner | eval/golden, baseline.json, ACR gate (METRIC-002) |
| **Precision** | TCVR (tool-call validity), patch-applies-cleanly, no-regression on unrelated tests, false-positive closes | **pytest** + `git apply --check` / `git diff --exit-code`, the harness's tool-call log; diff-coverage via **diff-cover** | TCVR in the dashboard |
| **Egress / security** | zero non-loopback egress; no secrets, no known vulns, no obvious SAST findings; sandboxed execution | egress: **tcpdump/libpcap**, **unshare -n** / **bubblewrap** / **nsjail** / **firejail** (network-deny sandbox), optional eBPF **Falco/Tetragon**; supply-chain: **gitleaks**/**trufflehog** (secrets), **govulncheck** (Go), **grype**+**syft** or **trivy** (vuln + SBOM), **semgrep** (SAST, local rules); signing: **minisign**/**cosign** | verify-airgap.sh (tcpdump), minisign, GUARD gate |

Air-gap notes: SWE-bench runs offline once its task images/repos are pre-fetched into the enclave;
semgrep/grype/trivy run with local rule/vuln DBs (ship the DB, disable update-check); every scanner
above has an offline mode. Prefer the deterministic, no-network path for each — matching the GUARD
doctrine that any egress is a build-failing condition.

## Requirements (category E2E, phase 11 — validates the phase-10 agent-capability work)

- **E2E-001** — E2E golden-set harness: drive the full loop over `eval/golden` end-to-end in a captured
  sandbox and emit the whole metric dashboard (ACR/TCVR/FPVR/MTC/WCR/TCR). Ties METRIC + the golden set
  into one runnable gate. (pytest runner + `scripts/ci-metrics.py`.)
- **E2E-002** — Performance suite: per-target tok/s, prefill/decode split, WCR/TCR, latency percentiles,
  with rolling baselines + a regression gate. (hyperfine + bench.sh + metrics.)
- **E2E-003** — Accuracy suite: ACR/pass@1 on `eval/golden` (always-on) plus an offline
  SWE-bench-Verified subset (periodic), test-as-oracle. (SWE-bench harness offline.)
- **E2E-004** — Precision suite: TCVR, patch-applies-cleanly, no-regression on unrelated tests, and a
  false-positive-close check (a "done" that a re-run of the test refutes). (pytest + `git apply --check`.)
- **E2E-005** — Egress-zero gate (hardened): run the whole suite inside a network-denied sandbox and
  assert zero non-loopback packets via pcap. Extends GUARD-001 from a spot check to a suite-wide gate.
  (unshare -n / bubblewrap + tcpdump.)
- **E2E-006** — Supply-chain & secret gate: offline secret scan + Go-vuln + SBOM + SAST on the repo and
  any agent-generated code. (gitleaks, govulncheck, grype+syft, semgrep.)
- **E2E-007** — Sandboxed agent execution: agent-generated code/tests run in a locked sandbox (no
  network, restricted FS, resource + seccomp/landlock caps) so a golden-set run cannot escape or egress.
  (bubblewrap / nsjail.)
- **E2E-008** — E2E CI wiring: the suite runs as staged CI gates (build → unit → egress-zero → golden
  metrics → security); any red gate fails the build. Extends `.ci/pipeline.yml`.

## Deferred / optional

- **Cloud benchmark leaderboards** (hosted SWE-bench submission) — out by air-gap; run the harness
  locally and keep results in-enclave.
- **eBPF runtime security (Falco/Tetragon)** — powerful syscall/network visibility, but heavier than
  the pcap + network-namespace approach; adopt only if the pcap gate proves insufficient.
