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
        airgap metrics badges release ci ci-fast ci-darwin hooks-install clean help

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
lint:
	@if command -v golangci-lint >/dev/null 2>&1; then \
		golangci-lint run ./...; \
	else \
		echo "lint: note — golangci-lint not installed; skipping locally (CI installs + enforces it)."; \
	fi

## race: run the test suite under the race detector
race:
	$(GO) test -race ./...

## vuln: run govulncheck (degrades to a note if not installed — CI enforces it)
vuln:
	@if command -v govulncheck >/dev/null 2>&1; then \
		govulncheck ./...; \
	else \
		echo "vuln: note — govulncheck not installed; skipping locally (CI installs + enforces it)."; \
	fi

## badges: regenerate live README badge data (coverage, version) into badges/
badges:
	scripts/gen-badges.sh

## airgap: EGRESS=0 gate — run a representative command under egress capture
airgap: build
	scripts/verify-airgap.sh -- $(AIRGAP_CMD)

## metrics: compute golden-set dashboard metrics + enforce the ACR-regression gate
metrics:
	python3 scripts/ci-metrics.py --golden $(GOLDEN) --baseline $(BASELINE)

## ci-fast: pre-commit subset — fast feedback before a commit
ci-fast: fmt-check lint vet build test health
	@echo "ci-fast: OK"

## ci: THE pipeline. Identical target run by GitHub Actions (linux leg) and the pre-push hook.
## Stages: fmt/lint/vet -> build -> unit + race + cover-gate -> vuln -> airgap (EGRESS=0) -> health (TRACE=100%) -> metrics (ACR-regression)
ci: fmt-check lint vet build test race cover-gate vuln airgap health metrics
	@echo "ci: OK (all three hard gates held: EGRESS=0, TRACE=100%, ACR-regression)"

## ci-darwin: macOS CI leg — `ci` minus `airgap`. The netns/ss egress proof in
## scripts/verify-airgap.sh is linux-specific (macOS has neither unshare -rn nor
## ss); the EGRESS=0 ITAR gate is enforced on the linux enclave host + linux CI
## job. This leg exists for darwin-metal build/test parity, NOT the airgap proof.
ci-darwin: fmt-check lint vet build test race cover-gate vuln health metrics
	@echo "ci-darwin: OK (darwin-metal build/test parity; EGRESS=0 proof runs on the linux leg)"

## hooks-install: install the pre-commit + pre-push git hooks (idempotent)
hooks-install:
	scripts/install-hooks.sh

## release: reproducible offline signed release (binaries + SBOM + checksums) into dist/
release:
	scripts/release.sh

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
