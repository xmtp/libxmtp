#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
if [[ ! -d "$root/$1/node_modules" ]]; then
  echo "Missing JavaScript dependencies for $1. Run: dev/nix-shell 'just install-js'" >&2
  exit 1
fi
