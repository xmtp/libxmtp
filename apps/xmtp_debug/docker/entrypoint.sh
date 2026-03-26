#!/bin/bash
# set -uo pipefail is used instead of set -euo pipefail intentionally:
# -e is omitted so that individual xdbg failures do not kill the outer
# monitoring loop — the daemon recovers on the next iteration.
set -uo pipefail

: "${XDBG_LOOP_PAUSE:=300}"    # default interval between loop iterations
: "${XDBG_V4_NODE_URL:=}"      # V4/D14N node URL for migration latency test
: "${XDBG_MIGRATION_TIMEOUT:=120}" # timeout for migration latency polling
: "${XDBG_CONTINUITY_MESSAGES:=5}" # messages per wallet continuity iteration

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

# Migration test: always writes to V3 production and reads from V4 testnet,
# because the only migrator path is V3 production → V4 testnet.
# XDBG_V4_NODE_URL can be overridden via env, but defaults to the testnet
# D14N replication node regardless of WORKSPACE.
: "${XDBG_V4_NODE_URL:=https://grpc.testnet.xmtp.network:443}"
log "V4 node URL (migration): ${XDBG_V4_NODE_URL}"

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

    # Migration latency test: always V3 production → V4 testnet (the only migration path).
    # Omits -d and --perf (D14N mode) since this writes to V3.
    # Uses -b production regardless of WORKSPACE.
    log "Migration latency test..."
    XDBG_LOOP_PAUSE=0 xdbg -b production test migration-latency \
      --v4-node-url "${XDBG_V4_NODE_URL}" \
      --migration-timeout "${XDBG_MIGRATION_TIMEOUT}" \
      --iterations 1 \
      || log "WARNING: migration latency test $x failed"

    # Content parity test: write structured payloads to V3, diff on V4.
    log "Content parity test..."
    XDBG_LOOP_PAUSE=0 xdbg -b production test content-parity \
      --v4-node-url "${XDBG_V4_NODE_URL}" \
      --migration-timeout "${XDBG_MIGRATION_TIMEOUT}" \
      --parity-messages "${XDBG_PARITY_MESSAGES:-5}" \
      --iterations 1 \
      || log "WARNING: content parity test $x failed"

    # Wallet continuity test: verify same wallet → same inbox_id on V4.
    log "Wallet continuity test..."
    XDBG_LOOP_PAUSE=0 xdbg -b production test wallet-continuity \
      --v4-node-url "${XDBG_V4_NODE_URL}" \
      --migration-timeout "${XDBG_MIGRATION_TIMEOUT}" \
      --continuity-messages "${XDBG_CONTINUITY_MESSAGES}" \
      --iterations 1 \
      || log "WARNING: wallet continuity test $x failed"

    log "Sleeping ${XDBG_LOOP_PAUSE} seconds..."
    sleep "${XDBG_LOOP_PAUSE}"
  done
done
