export NIX_DEVSHELL := env("NIX_DEVSHELL", "js")

root := justfile_directory()

# Every JS suite needs a backend, and its port differs per worktree. `_env`
# loads dev/docker/.env (written by dev/worktree-env) so each recipe picks up
# this worktree's XMTP_BACKEND_URL. An already-set value always wins, so CI and
# one-off overrides are untouched. The URL uses the IPv4 form on purpose:
# `localhost` can resolve to IPv6, where the Docker port forward resets.
_env := root + "/dev/worktree-env && . " + root + "/dev/docker/load-env"

set shell := ["../../dev/nix-shell"]

# Build + stage the local bindings into bindings/<x>/dist via Nix
bindings target="all":
    ./dev/bindings {{ target }}

bindings-node:
    ./dev/bindings node

bindings-wasm:
    ./dev/bindings wasm

# Typecheck all SDKs with their task dependencies. Native bindings are staged first.
check: bindings
    cd {{ root }} && pnpm -r --filter './sdks/js/*' run typecheck

check-node: bindings-node
    cd {{ root }} && pnpm -r --filter '@xmtp/agent-sdk...' run typecheck
    pnpm --filter @xmtp/node-sdk exec tsc -p type-tests/tsconfig.json

# Check the published Node push API and its absence from Browser and WASM.
check-notification-surface: bindings
    cd {{ root }} && pnpm -r --filter '@xmtp/node-sdk' --filter '@xmtp/browser-sdk' run build
    pnpm --filter @xmtp/node-sdk exec tsc -p type-tests/tsconfig.json
    pnpm --filter @xmtp/browser-sdk exec tsc -p type-tests/tsconfig.json

# Build all SDKs with their task dependencies. Native bindings are staged first.
build: bindings
    cd {{ root }} && pnpm -r --filter './sdks/js/*' run build

# Lint (eslint) all SDKs (needs bindings for type-aware rules)
lint: bindings
    pnpm --filter @xmtp/node-sdk run build
    pnpm --filter @xmtp/node-sdk exec eslint . ../browser-sdk ../agent-sdk

lint-node: bindings-node
    pnpm --filter @xmtp/node-sdk run build
    pnpm --filter @xmtp/node-sdk exec eslint . ../agent-sdk

build-node: bindings-node
    cd {{ root }} && pnpm -r --filter '@xmtp/agent-sdk...' run build

# Run all JS SDK tests locally (builds bindings first)
test: bindings
    {{ _env }} && pnpm --filter @xmtp/node-sdk run test
    {{ _env }} && pnpm --filter @xmtp/browser-sdk run test
    cd {{ root }} && pnpm -r --filter '@xmtp/agent-sdk...' run build
    {{ _env }} && pnpm --filter @xmtp/agent-sdk run test

# CI: node-sdk tests. Bindings are a nix cache hit (built by test-node job).
# Extra args (e.g. --shard N/M) are forwarded to vitest.
test-node-sdk-ci *args="": bindings-node
    {{ _env }} && pnpm --filter @xmtp/node-sdk run test {{ args }}

# CI: browser-sdk tests (Playwright). Bindings are a nix cache hit (test-wasm).
test-browser-sdk-ci *args="": bindings
    {{ _env }} && pnpm --filter @xmtp/browser-sdk run test {{ args }}

# CI: agent-sdk tests. Runs against the local node-sdk (workspace dependency),
# which must be built first so its dist exists at runtime.
test-agent-sdk-ci *args="": bindings-node
    cd {{ root }} && pnpm -r --filter '@xmtp/agent-sdk...' run build
    {{ _env }} && pnpm --filter @xmtp/agent-sdk run test {{ args }}
