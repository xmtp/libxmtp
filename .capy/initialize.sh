#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

bash .capy/nix.sh

nix --version
dev/nix-shell 'rustc --version && cargo --version && node --version && pnpm --version && just --list'
bash .capy/refresh.sh
dev/nix-shell 'dev/agent-run cargo clippy --locked -p xmtp_mls --all-targets --no-deps -- -D warnings'
