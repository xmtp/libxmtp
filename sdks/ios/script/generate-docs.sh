#!/usr/bin/env bash
set -euo pipefail
repository_root="$(git rev-parse --show-toplevel)"
output_path="${1:-apps/docs/generated/reference/swift}"
case "$output_path" in /*) ;; *) output_path="$repository_root/$output_path" ;; esac
mkdir -p "$(dirname "$output_path")"
cd "$repository_root"
swift package --allow-writing-to-directory "$output_path" generate-documentation \
  --target XmtpSdk --disable-indexing --hosting-base-path reference/swift --output-path "$output_path"
