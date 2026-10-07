#!/usr/bin/env bash
set -euo pipefail
repository_root="$(git rev-parse --show-toplevel)"
output_path="${1:-apps/docs/generated/reference/swift}"
case "$output_path" in /*) ;; *) output_path="$repository_root/$output_path" ;; esac
mkdir -p "$(dirname "$output_path")"
product="$repository_root/target/ios-docs-input"
nix build --max-jobs 1 "$repository_root#ios-xcframeworks-fast" --out-link "$product"
receipt="${RUNNER_TEMP:-$repository_root/target}/docs-swift-inputs.json"
python3.11 "$repository_root/sdks/ios/script/docs-package.py" \
  --product "$product" --output "$output_path" --receipt "$receipt"
