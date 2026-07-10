# Multi-stage image for the keepalive-probe binary.
# Build context is the repo root (so the cargo build can see the workspace):
#   docker build -f apps/keepalive-probe/docker/Dockerfile .
FROM rust:1-bookworm AS builder
WORKDIR /src

COPY . .

ARG FORCE_REBUILD=0
RUN echo "FORCE_REBUILD=$FORCE_REBUILD"

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --package keepalive-probe

FROM debian:trixie-slim
WORKDIR /work

# tini = PID 1 so SIGTERM reaches the probe (it drains on SIGINT/SIGTERM);
# procps for the pgrep healthcheck; ca-certificates is belt-and-suspenders
# (the probe compiles in webpki-roots and doesn't use the system trust store).
RUN apt-get update \
 && apt-get install -y --no-install-recommends tini ca-certificates procps \
 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/keepalive-probe /usr/local/bin/keepalive-probe

ENV RUST_LOG=info

HEALTHCHECK --interval=60s --timeout=10s --start-period=30s --retries=3 \
  CMD pgrep -x keepalive-probe >/dev/null || exit 1

# Args (e.g. --continual --subscribe-group <id> ...) are supplied at deploy time.
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/keepalive-probe"]
