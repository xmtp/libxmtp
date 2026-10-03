export NIX_DEVSHELL := env("NIX_DEVSHELL", "js")

root := justfile_directory()

# Every JS suite needs a backend, and its port differs per worktree. `_env`
# loads dev/docker/.env (written by dev/worktree-env) so each recipe picks up
# this worktree's XMTP_BACKEND_URL. An already-set value always wins, so CI and
# one-off overrides are untouched. The URL uses the IPv4 form on purpose:
# `localhost` can resolve to IPv6, where the Docker port forward resets.
_env := root + "/dev/worktree-env && . " + root + "/dev/docker/load-env"

set shell := ["../dev/nix-shell"]

# Stage the selected SDK products for local workspace imports.
sdk-products target="all":
    case "{{ target }}" in all) ../dev/js/sdk-stage node && ../dev/js/sdk-stage browser ;; node) ../dev/js/sdk-stage node ;; wasm|browser) ../dev/js/sdk-stage browser ;; *) exit 2 ;; esac

build-node-sdk:
    ../dev/js/sdk-package node

bindings-wasm:
    ../dev/js/bindings wasm

# Typecheck all SDKs with their task dependencies. SDK products are staged first.
check: sdk-products
    cd {{ root }} && pnpm -r --filter './sdks/*' run typecheck

check-node: build-node-sdk
    cd {{ root }} && pnpm -r --filter '@xmtp/agent-sdk...' run typecheck
    pnpm --filter @xmtp/node-sdk exec tsc -p type-tests/tsconfig.json

# Check the published Node push API and its absence from Browser and WASM.
check-notification-surface: sdk-products
    cd {{ root }} && pnpm -r --filter '@xmtp/node-sdk' --filter '@xmtp/browser-sdk' run build
    pnpm --filter @xmtp/node-sdk exec tsc -p type-tests/tsconfig.json
    pnpm --filter @xmtp/browser-sdk exec tsc -p type-tests/tsconfig.json

# Build all SDKs with their task dependencies. SDK products are staged first.
build: sdk-products
    cd {{ root }} && pnpm -r --filter './sdks/*' run build

# Lint all SDKs with their task dependencies. SDK products are staged first.
lint: sdk-products
    cd {{ root }} && pnpm -r --filter './sdks/*' run lint

lint-node: build-node-sdk
    cd {{ root }} && pnpm -r --filter '@xmtp/agent-sdk...' run lint

build-node: build-node-sdk
    cd {{ root }} && pnpm -r --filter '@xmtp/agent-sdk...' run build

# Run all JavaScript SDK tests with their selected products.
test: sdk-products
    {{ _env }} && pnpm --filter @xmtp/node-sdk run test
    {{ _env }} && pnpm --filter @xmtp/browser-sdk run test
    cd {{ root }} && pnpm -r --filter '@xmtp/agent-sdk...' run build
    {{ _env }} && pnpm --filter @xmtp/agent-sdk run test

# CI: Node tests use the matched public SDK product.
# Extra args (e.g. --shard N/M) are forwarded to vitest.
test-node-sdk-ci *args="": build-node-sdk
    {{ _env }} && pnpm --filter @xmtp/node-sdk run test {{ args }}

# CI: browser-sdk tests (Playwright). Bindings are a nix cache hit (test-wasm).
test-browser-sdk-ci *args="": bindings-wasm
    {{ _env }} && pnpm --filter @xmtp/browser-sdk run test {{ args }}

# CI: agent-sdk tests. Runs against the local node-sdk (workspace dependency),
# which must be built first so its dist exists at runtime.
test-agent-sdk-ci *args="": build-node-sdk
    cd {{ root }} && pnpm -r --filter '@xmtp/agent-sdk...' run build
    {{ _env }} && pnpm --filter @xmtp/agent-sdk run test {{ args }}
