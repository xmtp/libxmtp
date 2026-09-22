export NIX_DEVSHELL := "docs"

root := justfile_directory()

set shell := ["../../dev/nix-shell"]

# Build and stage the local Node and WASM bindings before docs package tasks.
bindings:
    cd ../../sdks/js && ./dev/bindings

# Linux gets Chromium from Nix. Run this once on macOS.
browsers:
    PLAYWRIGHT_SKIP_BROWSER_GC=1 pnpm --filter @xmtp/docs exec playwright install chromium

dev:
    pnpm --filter @xmtp/docs run dev

build: bindings
    cd {{ root }} && pnpm -r --filter '@xmtp/docs...' run build

check-examples: bindings
    cd {{ root }} && pnpm -r --filter '@xmtp/docs...' run check:examples

check-agent-docs:
    pnpm --filter @xmtp/docs run check:agent-docs

lint:
    # Docs lint is not type-aware and does not need SDK build output.
    pnpm run lint

format-check:
    pnpm --filter @xmtp/docs run format-check

format:
    pnpm --filter @xmtp/docs run format

test:
    pnpm --filter @xmtp/docs run test

test-browser:
    pnpm --filter @xmtp/docs run test:browser

compose:
    node scripts/compose.mjs

check:
    node scripts/check-site.mjs --built
