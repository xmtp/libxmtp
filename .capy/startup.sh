#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

exec dev/nix-shell 'just backend db-up'
