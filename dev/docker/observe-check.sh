#!/usr/bin/env bash
# Run through `just backend observe-check` so build tools come from Nix.
set -euo pipefail
cd "$(dirname "$0")/../.."
# Ports differ per worktree, so read them rather than assuming the defaults.
# shellcheck source=load-env
. dev/docker/load-env
# Cachix serves the xdbg compile across commits; only its wrapper names the commit.
xdbg="$(nix build --no-link --print-out-paths .#xdbg)" || { echo 'FAIL: build xdbg' >&2; exit 1; }
export OBSERVE_XDBG="$xdbg/bin/xdbg"
# The wrapper labels the build with the flake's short revision: the first 7 hex
# digits of HEAD, with `-dirty` for a dirty tree. In CI, HEAD is the merge commit.
# The version keys stored state, so an inherited XDBG_GIT_SHA must not change it.
revision="$(git rev-parse HEAD | cut -c1-7)"
check_version() {
  local label="$1" state version
  shift
  state="$(mktemp -d)"
  version="$(env -u RUST_LOG "$@" XDBG_DB_ROOT="$state" "$OBSERVE_XDBG" --version 2>&1)" \
    || { echo "FAIL: xdbg --version ($label): $version" >&2; exit 1; }
  rm -r "$state"
  case "$version" in
    *"-$revision"*) echo "PASS: xdbg --version ($label) names $revision" ;;
    *) echo "FAIL: xdbg --version ($label) does not name $revision: $version" >&2; exit 1 ;;
  esac
}
check_version "XDBG_GIT_SHA unset" -u XDBG_GIT_SHA
check_version "XDBG_GIT_SHA=bogus" XDBG_GIT_SHA=bogus
export OBSERVE_TEMPO_URL="http://127.0.0.1:${XMTP_TEMPO_PORT}"
export OBSERVE_PROMETHEUS_URL="http://127.0.0.1:${XMTP_PROMETHEUS_PORT}"
export OBSERVE_GRAFANA_URL="http://127.0.0.1:${XMTP_GRAFANA_PORT}"
export OBSERVE_BACKEND_URL="${XMTP_BACKEND_URL}"
export OBSERVE_OTLP_ENDPOINT="http://127.0.0.1:${XMTP_OTLP_GRPC_PORT}"
export OBSERVE_METRICS_URL="http://127.0.0.1:${XMTP_BACKEND_METRICS_PORT}/metrics"
exec python3.11 -B dev/docker/observe_check.py
