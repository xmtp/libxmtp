#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

bash .capy/nix.sh

nix --version
dev/nix-shell 'rustc --version && cargo --version && node --version && pnpm --version && just --list'
bash .capy/refresh.sh
dev/nix-shell 'dev/agent-run cargo clippy --locked -p xmtp_mls --all-targets --no-deps -- -D warnings'
dev/nix-shell 'RUST_MIN_STACK=8388608 dev/agent-run cargo nextest run --locked --profile ci -p xmtp_mls --lib -E "test(=storage_location::tests::deployment_dirs_do_not_collide)"'
