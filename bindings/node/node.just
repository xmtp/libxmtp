export NIX_DEVSHELL := env("NIX_DEVSHELL", "default")

set shell := ["../../dev/nix-shell"]

nix_system := arch() + "-" + if os() == "macos" { "darwin" } else { "linux" }

# Build NAPI bindings and assemble dist (needs Rust — always uses default shell)
[private]
[script("bash")]
_prepare-dist *extra_flags="":
    set -euo pipefail
    nix develop .#default --command bash -euc '\
      rm -rf dist && \
      pnpm --filter @xmtp/node-bindings exec napi build --platform --esm {{ extra_flags }} && \
      mkdir -p dist && mv index.js dist && mv index.d.ts dist && mv *.node dist'

# Build Node.js NAPI bindings
check: (_prepare-dist "--release")

# Check TypeScript formatting
lint:
    pnpm --filter @xmtp/node-bindings exec prettier -c .

# Format TypeScript files
format:
    pnpm --filter @xmtp/node-bindings exec prettier -w .

# Run Node.js tests (builds NAPI bindings with test features + vitest)
test: (_prepare-dist "--features test-utils")
    pnpm --filter @xmtp/node-bindings exec vitest run

# tests using built with nix. closer to ci but no local incremental compilation.
test-ci:
    rm -rf dist
    nix build .#packages.{{ nix_system }}.node-bindings-test
    ../../dev/ci/warm-deps .#packages.{{ nix_system }}.node-bindings-test
    cp -r result/dist dist
    chmod -R u+w dist
    pnpm --filter @xmtp/node-bindings exec vitest run

# Build Node.js NAPI bindings (alias)
build: check
