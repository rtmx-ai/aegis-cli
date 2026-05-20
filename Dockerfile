# Multi-stage build: produce minimal container with static aegis binary.
# REQ-BUILD-074: Minimal container image for air-gapped and container deployments.

FROM rust:1-bookworm AS builder
WORKDIR /build
COPY . .
RUN apt-get update && apt-get install -y musl-tools build-essential pkg-config clang && \
    rustup target add x86_64-unknown-linux-musl && \
    cargo build --release --target x86_64-unknown-linux-musl --package aegis-cli && \
    strip target/x86_64-unknown-linux-musl/release/aegis

FROM scratch
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/aegis /aegis
ENTRYPOINT ["/aegis"]
