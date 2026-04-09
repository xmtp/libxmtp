#!/usr/bin/env bash
# test-workflow-structure.sh
set -euo pipefail
WF=".github/workflows/release-ios.yml"
# Must NOT contain old steps or patterns
! grep -q "Build iOS libs" "$WF" || { echo "FAIL: Old 'Build iOS libs' step present"; exit 1; }
! grep -q "Validate xcframeworks" "$WF" || { echo "FAIL: Old 'Validate xcframeworks' step present"; exit 1; }
! grep -q "Package zip" "$WF" || { echo "FAIL: Old 'Package zip' step present"; exit 1; }
! grep -q "Package dynamic zip" "$WF" || { echo "FAIL: Old 'Package dynamic zip' step present"; exit 1; }
! grep -q "nix develop" "$WF" || { echo "FAIL: nix develop still referenced"; exit 1; }
! grep -q "make " "$WF" || { echo "FAIL: make still referenced"; exit 1; }
# Must contain new steps
grep -q "Build xcframeworks" "$WF" || { echo "FAIL: Missing 'Build xcframeworks' step"; exit 1; }
grep -q "Package and checksum" "$WF" || { echo "FAIL: Missing 'Package and checksum' step"; exit 1; }
echo "Workflow structure validation passed"
