export NIX_DEVSHELL := env("NIX_DEVSHELL", "default")
set shell := ["../../dev/nix-shell"]

# Install JS dependencies
install:
  yarn

# Install JS dependencies (CI - frozen lockfile)
install-ci:
  yarn install --immutable

# Build NAPI bindings and assemble dist (needs Rust — always uses default shell)
[private]
[script("bash")]
_prepare-dist *extra_flags="":
  set -euo pipefail
  nix develop .#default --command bash -euc '\
    rm -rf dist && \
    yarn napi build --platform --esm {{extra_flags}} && \
    mkdir -p dist && mv index.js dist && mv index.d.ts dist && mv *.node dist'

# Build Node.js NAPI bindings
check: install (_prepare-dist "--release")

# Check TypeScript formatting
lint: install
  yarn prettier -c .

# Format TypeScript files
format: install
  yarn prettier -w .

# Run Node.js tests (builds NAPI bindings with test features + vitest)
test: install (_prepare-dist "--features test-utils")
  yarn vitest run

# Build Node.js NAPI bindings (alias)
build: check
