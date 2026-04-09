#!/usr/bin/env bash
# test-flake-outputs.sh — verify new and existing flake outputs evaluate
set -euo pipefail
for pkg in ios-xcframeworks ios-xcframeworks-fast ios-libs ios-libs-fast; do
    TYPE=$(nix eval ".#$pkg" --apply 'x: x.type' 2>&1)
    echo "$TYPE" | grep -q '"derivation"' || { echo "FAIL: $pkg does not evaluate to derivation (got: $TYPE)"; exit 1; }
    echo "  OK: $pkg evaluates"
done
echo "Flake output evaluation passed"
