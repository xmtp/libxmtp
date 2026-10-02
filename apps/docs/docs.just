export NIX_DEVSHELL := "docs"

root := justfile_directory()

set shell := ["../../dev/nix-shell"]

# Stage the local Node SDK and the remaining WASM binding before docs tasks.
bindings:
    ../../dev/js/sdk-package node
    ../../dev/js/bindings wasm

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
    pnpm --filter @xmtp/docs run format:check

format:
    pnpm --filter @xmtp/docs run format

test:
    pnpm --filter @xmtp/docs run test

test-browser:
    pnpm --filter @xmtp/docs run test:browser

typecheck: bindings
    cd {{ root }} && pnpm -r --filter '@xmtp/docs...' run typecheck

compose:
    node scripts/compose.mjs

check:
    node scripts/check-site.mjs --built

# Check external URLs with the flags and HTML scope removed from docs CI.
check-external:
    node scripts/site-freshness.mjs
    DIST_ROOT=dist SITE_ROOT=_site REDIRECTS=parity/redirects.json node scripts/compose.mjs
    test -s _site/index.html || { echo "Composed site is missing or empty. Follow docs/AGENTS.md." >&2; exit 1; }
    lychee --no-progress --max-retries 2 --scheme https --scheme http \
        --root-dir '{{ root }}/apps/docs/_site' \
        --exclude '^https://docs\.xmtp\.org/.*' \
        --exclude '^https://self-hosted-docs\.xmtp\.to/.*' \
        --exclude '^https://github\.com/xmtp/libxmtp/edit/.*' \
        --exclude '^https://github\.com/xmtp/libxmtp/blob/[0-9a-f]{40}/.*' \
        --exclude '^http://collector:4317/?$' \
        --exclude '^https://www\.iana\.org/assignments/' \
        '_site/*.html' \
        '_site/agents/**/*.html' \
        '_site/chat-apps/**/*.html' \
        '_site/content-types/**/*.html' \
        '_site/get-started/**/*.html' \
        '_site/privacy/**/*.html' \
        '_site/protocol/**/*.html' \
        '_site/sdk/**/*.html' \
        '_site/specs/**/*.html' \
        '_site/terms/**/*.html' \
        '_site/tools/**/*.html' \
        '_site/reference/browser-sdk/**/*.html' \
        '_site/reference/node-sdk/**/*.html' \
        '_site/reference/agent-sdk/**/*.html' \
        '_site/reference/error-glossary/**/*.html' \
        '_site/reference/limits/**/*.html'
