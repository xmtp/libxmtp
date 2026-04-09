#!/usr/bin/env bash
# test-dev-script.sh
set -euo pipefail
SCRIPT="sdks/ios/dev/bindings"
! grep -q 'make ' "$SCRIPT" || { echo "FAIL: script still references make"; exit 1; }
! grep -q 'ensure_nix_shell' "$SCRIPT" || { echo "FAIL: script still uses ensure_nix_shell"; exit 1; }
grep -q 'ios-xcframeworks-fast' "$SCRIPT" || { echo "FAIL: fast path not using ios-xcframeworks-fast"; exit 1; }
grep -q 'ios-xcframeworks' "$SCRIPT" || { echo "FAIL: release path not using ios-xcframeworks"; exit 1; }
grep -q 'Libxmtp/xmtpv3.swift' "$SCRIPT" || { echo "FAIL: not copying xmtpv3.swift to SDK"; exit 1; }
echo "Dev script structure validation passed"
