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

# Pick vendored mode only when a real vendor/ tree exists; otherwise build
# offline against the module cache with no network (GOPROXY=off).
ifeq ($(wildcard vendor/modules.txt),)
GO_BUILD_ENV  := GOFLAGS=-mod=mod GOPROXY=off
else
GO_BUILD_ENV  := GOFLAGS=-mod=vendor
endif

.PHONY: all build fmt fmt-check vet test airgap metrics \
        ci ci-fast hooks-install clean help

all: build

## build: compile the static aegis binary offline (vendored if vendor/ exists)
build:
	@mkdir -p $(BIN_DIR)
	$(GO_BUILD_ENV) $(GO) build -o $(BIN) $(PKG)

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

## airgap: EGRESS=0 gate — run a representative command under egress capture
airgap: build
	scripts/verify-airgap.sh -- $(AIRGAP_CMD)

## metrics: compute golden-set dashboard metrics + enforce the ACR-regression gate
metrics:
	python3 scripts/ci-metrics.py --golden $(GOLDEN) --baseline $(BASELINE)

## ci-fast: pre-commit subset — fast feedback before a commit
ci-fast: fmt-check vet build test health
	@echo "ci-fast: OK"

## ci: THE pipeline. Identical target run by GitHub Actions and the pre-push hook.
## Stages: build -> unit -> airgap gate (EGRESS=0) -> trace/health (TRACE=100%) -> golden metrics (ACR-regression)
ci: fmt-check vet build test airgap health metrics
	@echo "ci: OK (all three hard gates held: EGRESS=0, TRACE=100%, ACR-regression)"

## hooks-install: install the pre-commit + pre-push git hooks (idempotent)
hooks-install:
	scripts/install-hooks.sh

## clean: remove build artifacts
clean:
	rm -rf $(BIN_DIR)

## help: list documented targets
help:
	@grep -hE '^##' $(MAKEFILE_LIST) | sed 's/^## //'

# rtmx-generated targets (rtm, backlog, health, verify, deps, cycles, ...).
# `health` and `verify` defined there are reused by the ci targets above so the
# rtmx commands are never duplicated.
include rtmx.mk
