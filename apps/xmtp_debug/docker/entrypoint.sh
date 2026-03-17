#!/bin/bash
# set -uo pipefail is used instead of set -euo pipefail intentionally:
# -e is omitted so that individual xdbg failures do not kill the outer
# monitoring loop — the daemon recovers on the next iteration.
set -uo pipefail

: "${XDBG_LOOP_PAUSE:=300}"    # default interval between loop iterations
: "${XDBG_V4_NODE_URL:=}"      # V4/D14N node URL for migration latency test
: "${XDBG_MIGRATION_TIMEOUT:=120}" # timeout for migration latency polling

function log {
    echo "[$(date '+%F %T')] $*"
}

# Determine backend from WORKSPACE
WORKSPACE="${WORKSPACE:-}"
case "${WORKSPACE}" in
    testnet) BACKEND="production" ;;
    testnet-dev) BACKEND="dev" ;;
    testnet-staging) BACKEND="staging" ;;
    ""|*) BACKEND="local" ;;
esac
log "WORKSPACE='${WORKSPACE:-<unset>}' -> backend='${BACKEND}'"

# Derive V4 node URL from WORKSPACE if not explicitly set.
# Migration test writes to V3 (production) and reads from V4 (D14N testnet).
if [ -z "${XDBG_V4_NODE_URL}" ]; then
    case "${WORKSPACE}" in
        testnet)         XDBG_V4_NODE_URL="https://grpc.testnet.xmtp.network:443" ;;
        testnet-dev)     XDBG_V4_NODE_URL="https://grpc.dev.xmtp.network:443" ;;
        testnet-staging) XDBG_V4_NODE_URL="https://grpc.staging.xmtp.network:443" ;;
        *)               XDBG_V4_NODE_URL="" ;;
    esac
fi
if [ -n "${XDBG_V4_NODE_URL}" ]; then
    log "V4 node URL: ${XDBG_V4_NODE_URL}"
fi

while true; do
  log "Reset environment.."
  XDBG_LOOP_PAUSE=0 xdbg -d -b "${BACKEND}" --perf --clear \
    || log "WARNING: --clear failed; proceeding with existing state"
  XDBG_LOOP_PAUSE=0 xdbg -d -b "${BACKEND}" --perf generate --entity identity --amount 5 --concurrency 1 \
    || log "WARNING: identity generation failed"
  XDBG_LOOP_PAUSE=0 xdbg -d -b "${BACKEND}" --perf generate --entity group --amount 1 --concurrency 1 --invite 1 \
    || log "WARNING: group generation failed"
  log "Reset complete, starting tests"

  for x in {1..10}; do
    log "Identities..."
    XDBG_LOOP_PAUSE=0 xdbg -d -b "${BACKEND}" --perf generate --entity identity --amount 1 --concurrency 1 \
      || log "WARNING: identity step $x failed"
    log "Sleeping 20s..."
    sleep 20
    log "Groups..."
    XDBG_LOOP_PAUSE=0 xdbg -d -b "${BACKEND}" --perf generate --entity group --amount 1 --concurrency 1 --invite 1 \
      || log "WARNING: group step $x failed"
    log "Sleeping 20s..."
    sleep 20
    log "Messages..."
    XDBG_LOOP_PAUSE=0 xdbg -d -b "${BACKEND}" --perf generate --entity message --amount 1 --concurrency 1 \
      || log "WARNING: message step $x failed"
    log "Running health checks..."
    bash "$(dirname "$0")/web-healthcheck.sh" || log "WARNING: health check failed"

    # Migration latency test: writes to V3 (no -d), reads from V4 node
    if [ -n "${XDBG_V4_NODE_URL}" ]; then
      log "Migration latency test..."
      XDBG_LOOP_PAUSE=0 xdbg -b "${BACKEND}" --perf test migration-latency \
        --v4-node-url "${XDBG_V4_NODE_URL}" \
        --migration-timeout "${XDBG_MIGRATION_TIMEOUT}" \
        --iterations 1 \
        || log "WARNING: migration latency test $x failed"
    fi

    log "Sleeping ${XDBG_LOOP_PAUSE} seconds..."
    sleep "${XDBG_LOOP_PAUSE}"
  done
done
