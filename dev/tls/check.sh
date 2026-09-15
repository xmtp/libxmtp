#!/bin/bash
set -euo pipefail
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

case "${1:-}" in
  "")
    python3 "$script_dir/check.py"
    nix shell nixpkgs#grpc-health-probe --command grpc-health-probe \
      -tls -tls-ca-cert "$script_dir/.generated/server.crt" -addr 127.0.0.1:18443
    echo 'PASS 5: grpc-health-probe -tls'
    ;;
  --short-timeout)
    # Twelve seconds permits a grpc-go PING at its minimum interval of 10s.
    sed -e 's/timeout client 24h/timeout client 12s/' \
      -e 's/timeout server 24h/timeout server 12s/' \
      -e '/timeout connect 5s/a\
    timeout tunnel 1h
' \
      "$script_dir/haproxy.cfg" >"$script_dir/.generated/short.cfg"
    restore() {
      docker compose -f "$script_dir/compose.yml" -p libxmtp-tls \
        up --detach --wait --force-recreate haproxy
    }
    trap restore EXIT
    XMTP_TLS_CONFIG="$script_dir/.generated/short.cfg" \
      docker compose -f "$script_dir/compose.yml" -p libxmtp-tls \
      up --detach --wait --force-recreate haproxy
    python3 "$script_dir/check.py" --short-timeout
    ;;
  *) echo "usage: $0 [--short-timeout]" >&2; exit 2 ;;
esac
