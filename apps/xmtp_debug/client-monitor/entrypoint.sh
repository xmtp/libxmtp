#!/bin/bash
# set -uo pipefail is used instead of set -euo pipefail intentionally:
# -e is omitted so that individual xdbg failures do not kill the outer
# monitoring loop — the daemon recovers on the next iteration.
set -uo pipefail

: "${XDBG_LOOP_PAUSE:=300}" # default interval between loop iterations

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


while true; do
  log "Reset environment.."
  XDBG_LOOP_PAUSE=0 xdbg -d -b "${BACKEND}" --clear \
    || log "WARNING: --clear failed; proceeding with existing state"
  XDBG_LOOP_PAUSE=0 xdbg -d -b "${BACKEND}" generate --entity identity --amount 5 --concurrency 1 \
    || log "WARNING: identity generation failed"
  XDBG_LOOP_PAUSE=0 xdbg -d -b "${BACKEND}" generate --entity group --amount 1 --concurrency 1 --invite 1 \
    || log "WARNING: group generation failed"
  log "Reset complete, starting tests"

  for x in {1..10}; do
    log "Identities..."
    XDBG_LOOP_PAUSE=0 xdbg -d -b "${BACKEND}" generate --entity identity --amount 1 --concurrency 1 \
      || log "WARNING: identity step $x failed"
    log "Sleeping 20s..."
    sleep 20
    log "Groups..."
    XDBG_LOOP_PAUSE=0 xdbg -d -b "${BACKEND}" generate --entity group --amount 1 --concurrency 1 --invite 1 \
      || log "WARNING: group step $x failed"
    log "Sleeping 20s..."
    sleep 20
    log "Messages..."
    XDBG_LOOP_PAUSE=0 xdbg -d -b "${BACKEND}" generate --entity message --amount 1 --concurrency 1 \
      || log "WARNING: message step $x failed"
    log "Running health checks..."
    bash "$(dirname "$0")/web-healthcheck.sh" || log "WARNING: health check failed"
    log "Sleeping ${XDBG_LOOP_PAUSE} seconds..."
    sleep "${XDBG_LOOP_PAUSE}"
  done
done
