export NIX_DEVSHELL := env("NIX_DEVSHELL", "js")

set shell := ["../../dev/nix-shell"]

# Install JS dependencies
install:
    yarn

# Install JS dependencies (CI - frozen lockfile)
install-ci:
    yarn install --immutable

# Build + stage the local bindings into bindings/<x>/dist via Nix
bindings:
    ./dev/bindings

# Typecheck both SDKs
check: install
    yarn workspaces foreach -A run typecheck

# Build both SDKs (needs bindings present)
build: install bindings
    yarn workspaces foreach -A run build

# Lint (eslint) both SDKs (needs bindings for type-aware rules)
lint: install bindings
    yarn lint

# Run all JS SDK tests locally (builds bindings first)
test: install bindings
    yarn workspace @xmtp/node-sdk run test
    yarn workspace @xmtp/browser-sdk run test

# CI: node-sdk tests. Bindings are a nix cache hit (built by test-node job).
# Extra args (e.g. --shard N/M) are forwarded to vitest.
test-node-sdk-ci *args="": install-ci bindings
    yarn workspace @xmtp/node-sdk run test {{ args }}

# CI: browser-sdk tests (Playwright). Bindings are a nix cache hit (test-wasm).
test-browser-sdk-ci *args="": install-ci bindings
    yarn workspace @xmtp/browser-sdk run test {{ args }}
