#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

inputs="$(
  git ls-files -z -- \
    Cargo.toml '**/Cargo.toml' Cargo.lock .cargo \
    package.json '**/package.json' pnpm-lock.yaml pnpm-workspace.yaml \
    .npmrc '**/.npmrc' '.pnpmfile.*' '**/.pnpmfile.*' 'patches/**' '**/patches/**' \
    .node-version .nvmrc rust-toolchain.toml \
    flake.nix flake.lock nix webdriver.json \
    sdks/react-native/test/consumers/native-toolchain.json dev/nix-shell \
    .capy/initialize.sh .capy/refresh.sh .capy/nix.sh |
    xargs -0 sha256sum | sha256sum
)"
stamp=.cache/capy/install-inputs
if [[ -f "$stamp" && -f node_modules/.modules.yaml && "$(cat "$stamp")" == "$inputs" ]]; then
  echo 'Capy dependencies are current.'
  exit 0
fi

bash .capy/nix.sh
dev/nix-shell 'dev/agent-run cargo fetch --locked && just install-js'
mkdir -p "$(dirname "$stamp")"
printf '%s\n' "$inputs" > "$stamp"
