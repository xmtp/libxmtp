FROM rust:1-bookworm AS builder
WORKDIR /src

RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
 && rm -rf /var/lib/apt/lists/*

COPY . .

ARG FORCE_REBUILD=0
RUN echo "FORCE_REBUILD=$FORCE_REBUILD"

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --package xdbg

FROM debian:trixie-slim

WORKDIR /work

RUN apt-get update \
 && apt-get install -y --no-install-recommends tini bash ca-certificates procps libssl3 curl \
 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/xdbg /usr/local/bin/xdbg
COPY apps/xmtp_debug/docker/entrypoint.sh /usr/local/bin/entrypoint.sh
COPY apps/xmtp_debug/docker/web-healthcheck.sh /usr/local/bin/web-healthcheck.sh
RUN chmod +x /usr/local/bin/xdbg \
             /usr/local/bin/entrypoint.sh \
             /usr/local/bin/web-healthcheck.sh

# Liveness: confirm the runner is alive (xdbg sleeps between iterations,
# so check for the runner script rather than the xdbg binary itself)
HEALTHCHECK --interval=60s --timeout=10s --start-period=30s --retries=3 \
  CMD pgrep -f "bash.*entrypoint.sh" || exit 1

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/entrypoint.sh"]
