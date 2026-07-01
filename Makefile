# aegis-cli — Makefile
#
# SINGLE SOURCE OF TRUTH for the CI pipeline.
#
# The pipeline is defined exactly once, here, in the `ci` target. GitHub Actions
# (.github/workflows/ci.yml) runs `make ci` and nothing else; the pre-push git
# hook runs `make ci`; the pre-commit git hook runs the fast subset `make ci-fast`.
# Local and CI parity is therefore STRUCTURAL, not copy-pasted: every actor calls
# the same target. Do not duplicate step logic into the workflow or the hooks.
#
# rtmx is the dev-loop foundation: `make health` (rtmx health) is the TRACE=100%
# gate inside `make ci`, and `make verify` (rtmx verify) is closed-loop
# verification. The rtmx-generated targets (rtm, backlog, health, verify, ...)
# live in rtmx.mk and are included below so `make rtm` / `make backlog` work per
# project convention.

# Offline / vendored build. Std-lib-only deps mean a vendor/ dir is optional; if
# it is absent, `-mod=vendor` would error, so we select the flag dynamically and
# fall back cleanly to a normal offline build (GOFLAGS=-mod=mod GOPROXY=off).
GO            ?= go
BIN_DIR       := bin
BIN           := $(BIN_DIR)/aegis
PKG           := ./cmd/aegis
GOLDEN        := eval/golden
BASELINE      := eval/baseline.json
AIRGAP_CMD    := $(BIN) version
VERSION       := $(shell tr -d ' \n' < VERSION 2>/dev/null || echo dev)
LDFLAGS       := -ldflags "-s -w -X main.version=$(VERSION)"
COVERPROFILE  := coverage.out
GOBIN_DIR     := $(shell $(GO) env GOPATH)/bin

# Coverage-regression floor: `make cover-gate` fails if module statement
# coverage drops below this. Set at a round value the current total clears with
# margin (measured ~77.9% via `go test -coverpkg=./...`); raise as coverage
# grows. This is the coverage half of the regression story (the ACR-regression
# gate lives in `make metrics`).
COVER_MIN     ?= 70

# Pick vendored mode only when a real vendor/ tree exists; otherwise build
# offline against the module cache with no network (GOPROXY=off).
ifeq ($(wildcard vendor/modules.txt),)
GO_BUILD_ENV  := GOFLAGS=-mod=mod GOPROXY=off
else
GO_BUILD_ENV  := GOFLAGS=-mod=vendor
endif

.PHONY: all build fmt fmt-check vet test cover cover-gate lint race vuln \
        airgap airgap-run origin-gate integration-smoke metrics badges release verify-release ci ci-fast ci-darwin hooks-install clean help

all: build

## build: compile the static aegis binary offline (vendored if vendor/ exists)
build:
	@mkdir -p $(BIN_DIR)
	$(GO_BUILD_ENV) $(GO) build $(LDFLAGS) -o $(BIN) $(PKG)

## fmt: format all Go sources in place
fmt:
	$(GO) fmt ./...

## fmt-check: fail if any Go source is not gofmt-clean
fmt-check:
	@unformatted="$$(gofmt -l $$(find . -name '*.go' -not -path './vendor/*'))"; \
	if [ -n "$$unformatted" ]; then \
		echo "gofmt needed on:"; echo "$$unformatted"; exit 1; \
	fi; \
	echo "fmt-check: clean"

## vet: run go vet across the module
vet:
	$(GO) vet ./...

## test: run the unit + integration test suite
test:
	$(GO) test ./...

## cover: run tests with coverage and print the module total
cover:
	$(GO) test -coverpkg=./... -coverprofile=$(COVERPROFILE) ./...
	@$(GO) tool cover -func=$(COVERPROFILE) | awk '/^total:/ {print "total coverage: "$$3}'

## cover-gate: fail if module statement coverage is below COVER_MIN
cover-gate: cover
	@total="$$($(GO) tool cover -func=$(COVERPROFILE) | awk '/^total:/ {gsub(/%/,"",$$3); print $$3}')"; \
	echo "cover-gate: measured $$total% vs floor $(COVER_MIN)%"; \
	awk -v t="$$total" -v m="$(COVER_MIN)" 'BEGIN { exit !(t+0 >= m+0) }' || { \
		echo "cover-gate: FAIL — coverage $$total% is below the $(COVER_MIN)% floor" >&2; exit 1; }; \
	echo "cover-gate: OK"

## lint: run golangci-lint (degrades to a note if not installed — CI enforces it)
## Resolves the tool from PATH OR $(GOPATH)/bin, so the git hooks (which run with
## a minimal PATH) still find a `go install`-ed linter — closing the parity gap
## that let lint findings reach CI green-locally.
lint:
	@bin="$$(command -v golangci-lint || true)"; \
	[ -n "$$bin" ] || { [ -x "$(GOBIN_DIR)/golangci-lint" ] && bin="$(GOBIN_DIR)/golangci-lint"; }; \
	if [ -n "$$bin" ]; then "$$bin" run ./...; \
	else echo "lint: note — golangci-lint not installed (PATH or $(GOBIN_DIR)); CI installs + enforces it."; fi

## race: run the test suite under the race detector
race:
	$(GO) test -race ./...

## vuln: run govulncheck (degrades to a note if not installed — CI enforces it)
vuln:
	@bin="$$(command -v govulncheck || true)"; \
	[ -n "$$bin" ] || { [ -x "$(GOBIN_DIR)/govulncheck" ] && bin="$(GOBIN_DIR)/govulncheck"; }; \
	if [ -n "$$bin" ]; then "$$bin" ./...; \
	else echo "vuln: note — govulncheck not installed (PATH or $(GOBIN_DIR)); CI installs + enforces it."; fi

## security: E2E-006 supply-chain + secret gate — offline scanners (gitleaks secrets, gosec SAST).
## Tolerant like vuln: runs the scanners that are installed, notes the rest. Findings fail the gate.
## Enforced in CI once the scanners are added to the toolchain setup (operator step, mirrors metrics
## needing the model) — so this never breaks an absent-tool environment.
security:
	@gl="$$(command -v gitleaks || true)"; \
	if [ -n "$$gl" ]; then "$$gl" detect --no-git --no-banner --redact --exit-code 1; \
	else echo "security: note — gitleaks not installed; CI installs + enforces it (secrets)."; fi
	@gs="$$(command -v gosec || true)"; \
	[ -n "$$gs" ] || { [ -x "$(GOBIN_DIR)/gosec" ] && gs="$(GOBIN_DIR)/gosec"; }; \
	if [ -n "$$gs" ]; then "$$gs" -quiet -exclude-dir=vendor $(GOSEC_EXCLUDE) ./...; \
	else echo "security: note — gosec not installed; CI installs + enforces it (SAST)."; fi

# GOSEC_EXCLUDE: rule classes that are noise for a systems CLI, not signal —
#   G104 errcheck (golangci-lint's errcheck already enforces this, repo passes it)
#   G204 subprocess (aegis's job is launching opencode/git/llama-server)
#   G301/G302/G306 file perms (0644/0755 on local non-secret config/map/ledger files
#         is intentional; secrets like the audit log are already 0600)
#   G304 file inclusion (a CLI reads operator/config-supplied paths — its function)
# The high-signal rules stay on: G101 (creds), G115 (int overflow), crypto (G40x/G50x).
GOSEC_EXCLUDE ?= -exclude=G104,G204,G301,G302,G304,G306

## badges: regenerate live README badge data (coverage, version) into badges/
badges:
	scripts/gen-badges.sh

## airgap: EGRESS=0 gate — run a representative command under egress capture
airgap: build
	scripts/verify-airgap.sh -- $(AIRGAP_CMD)

## airgap-run: ENCLAVE-001 whole-group EGRESS=0 proof — launch the bundled OpenCode
## (its bootstrap is where the egress vectors fire: ripgrep/npm/models.dev) under the
## egress gate and confirm it reaches readiness loopback-only. Skips with a note if
## OpenCode/ripgrep are not staged (bundle completeness is OC-009/REL). Run in the
## full-stack tier where OpenCode is built; fail-closed under netns in CI.
airgap-run: build
	AIRGAP_STRICT=1 scripts/verify-airgap.sh -- $(BIN) verify-env --check-opencode

## origin-gate: MODEL-007 model-provenance gate — fail the build when the pinned model
## (MODEL_REF) has a country of origin not allowed by deploy/models/origin-policy.json.
## The policy file is the explicit, auditable override (set a country to "allow").
origin-gate: build
	$(BIN) verify-env --check-origin

## integration-smoke: BUILD-012 full-stack smoke — bring up llama-server (--jinja) + the
## model + OpenCode and drive `aegis run` on a tiny task under the egress gate (EGRESS=0).
## Release-tier + gated: needs the built stack + a model GGUF (set MODEL_OUT to a local one).
integration-smoke: build
	scripts/integration-smoke.sh

## metrics: compute golden-set dashboard metrics + enforce the ACR-regression gate
metrics:
	python3 scripts/ci-metrics.py --golden $(GOLDEN) --baseline $(BASELINE)

## ci-fast: pre-commit subset — fast feedback before a commit
ci-fast: fmt-check lint vet build test health
	@echo "ci-fast: OK"

## ci: THE pipeline. Identical target run by GitHub Actions (linux leg) and the pre-push hook.
## Stages: fmt/lint/vet -> build -> unit + race + cover-gate -> vuln -> security -> airgap (EGRESS=0) -> health (TRACE=100%) -> metrics (ACR-regression)
ci: fmt-check lint vet build test race cover-gate vuln security airgap health metrics
	@echo "ci: OK (all three hard gates held: EGRESS=0, TRACE=100%, ACR-regression)"

## ci-darwin: macOS CI leg — `ci` minus `airgap`. The netns/ss egress proof in
## scripts/verify-airgap.sh is linux-specific (macOS has neither unshare -rn nor
## ss); the EGRESS=0 ITAR gate is enforced on the linux enclave host + linux CI
## job. This leg exists for darwin-metal build/test parity, NOT the airgap proof.
ci-darwin: fmt-check lint vet build test race cover-gate vuln security health metrics
	@echo "ci-darwin: OK (darwin-metal build/test parity; EGRESS=0 proof runs on the linux leg)"

## ci-full: BUILD-010 — the full-stack tier (release/nightly cadence) run locally.
## `make ci` (Go gates) + build OpenCode + llama.cpp from pinned source + stage the
## model if pinned. Heavy (minutes); NOT a per-commit gate — gives local parity
## with the release/nightly build. Needs bun + a C/C++ toolchain on the host.
ci-full: ci
	@echo "ci-full: building the full stack from pinned source"
	scripts/build-opencode.sh
	scripts/build-llama.sh
	@scripts/stage-model.sh 2>/dev/null || echo "ci-full: model pin pending (SERVE-016) — skipping stage-model"
	$(MAKE) origin-gate  # MODEL-007: model-provenance gate (MODEL_REF origin vs policy)
	$(MAKE) airgap-run   # ENCLAVE-001: whole-group EGRESS=0 proof with OpenCode staged
	@echo "ci-full: OK (aegis + OpenCode + llama-server built from pinned source)"

## hooks-install: install the pre-commit + pre-push git hooks (idempotent)
hooks-install:
	scripts/install-hooks.sh

## release: reproducible offline signed release (binaries + SBOM + checksums) into dist/
release:
	scripts/release.sh

## verify-release: verify dist/ checksums + the offline detached signature (consumer trust check)
verify-release:
	@set -e; \
	[ -f dist/SHA256SUMS ] || { echo "verify-release: dist/SHA256SUMS not found (run make release first)"; exit 1; }; \
	if [ -f dist/SHA256SUMS.minisig ] && command -v minisign >/dev/null 2>&1 && [ -f deploy/release/aegis-minisign.pub ]; then \
		minisign -Vm dist/SHA256SUMS -p deploy/release/aegis-minisign.pub && echo "signature: OK (minisign)"; \
	elif [ -f dist/SHA256SUMS.asc ] && command -v gpg >/dev/null 2>&1; then \
		gpg --verify dist/SHA256SUMS.asc dist/SHA256SUMS && echo "signature: OK (gpg)"; \
	else \
		echo "verify-release: NOTE — no usable signature/public key; checksums only (provision a key per docs/release-signing.md)."; \
	fi; \
	( cd dist && (command -v sha256sum >/dev/null 2>&1 && sha256sum -c SHA256SUMS || shasum -a 256 -c SHA256SUMS) >/dev/null ) && echo "checksums: OK"

## clean: remove build artifacts
clean:
	rm -rf $(BIN_DIR) dist

## help: list documented targets
help:
	@grep -hE '^##' $(MAKEFILE_LIST) | sed 's/^## //'

# rtmx-generated targets (rtm, backlog, health, verify, deps, cycles, ...).
# `health` and `verify` defined there are reused by the ci targets above so the
# rtmx commands are never duplicated.
include rtmx.mk
