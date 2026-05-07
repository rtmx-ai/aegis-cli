# Changelog

All notable changes to aegis-cli will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-05-07

### Added

- Terminal-native TUI with streaming markdown, syntax highlighting, and diff rendering
- REA agent loop with tool execution and context injection
- Human-in-the-loop (HITL) approval gate for mutating tool calls
- Immutable JSONL audit ledger with identity binding (metadata only, never CUI)
- LLM provider abstraction supporting Vertex AI, Bedrock, Azure, and local models
- Security layer with .aegisignore mandatory blocklist and sandboxing
- Infrastructure plugin host with aegis-infra/v1 NDJSON protocol
- Onboarding state machine with connected, self-managed, and air-gapped modes

### Changed

### Fixed

### Removed
