#!/bin/bash
# set -uo pipefail is used instead of set -euo pipefail intentionally:
# -e is omitted so that individual xdbg failures do not kill the outer
# monitoring loop — the daemon recovers on the next iteration.
set -uo pipefail

: "${XDBG_LOOP_PAUSE:=300}"    # default interval between loop iterations
: "${XMTP_BACKEND_URL:?Set XMTP_BACKEND_URL to the self-hosted backend URL}"

function log {
    echo "[$(date '+%F %T')] $*"
}

# --metrics is required here: the monitor consumes xdbg's CSV stdout lines
# (latency_seconds,…, throughput_events,…) via the container log pipeline.
# Prometheus PushGateway is activated independently by PUSHGATEWAY_URL.
while true; do
  log "Reset environment.."
  XDBG_LOOP_PAUSE=0 xdbg --metrics --url "${XMTP_BACKEND_URL}" --clear \
    || log "WARNING: --clear failed; proceeding with existing state"
  XDBG_LOOP_PAUSE=0 xdbg --metrics --url "${XMTP_BACKEND_URL}" generate --entity identity --amount 5 --concurrency 1 \
    || log "WARNING: identity generation failed"
  XDBG_LOOP_PAUSE=0 xdbg --metrics --url "${XMTP_BACKEND_URL}" generate --entity group --amount 1 --concurrency 1 --invite 1 \
    || log "WARNING: group generation failed"
  log "Reset complete, starting tests"

  for x in {1..10}; do
    log "Identities..."
    XDBG_LOOP_PAUSE=0 xdbg --metrics --url "${XMTP_BACKEND_URL}" generate --entity identity --amount 1 --concurrency 1 \
      || log "WARNING: identity step $x failed"
    log "Sleeping 20s..."
    sleep 20
    log "Groups..."
    XDBG_LOOP_PAUSE=0 xdbg --metrics --url "${XMTP_BACKEND_URL}" generate --entity group --amount 1 --concurrency 1 --invite 1 \
      || log "WARNING: group step $x failed"
    log "Sleeping 20s..."
    sleep 20
    log "Messages..."
    XDBG_LOOP_PAUSE=0 xdbg --metrics --url "${XMTP_BACKEND_URL}" generate --entity message --amount 1 --concurrency 1 \
      || log "WARNING: message step $x failed"
    log "Running health checks..."
    bash "$(dirname "$0")/web-healthcheck.sh" || log "WARNING: health check failed"

    log "Sleeping ${XDBG_LOOP_PAUSE} seconds..."
    sleep "${XDBG_LOOP_PAUSE}"
  done
done
