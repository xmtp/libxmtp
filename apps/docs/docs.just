export NIX_DEVSHELL := "docs"
set shell := ["../../dev/nix-shell"]

install:
    npm ci

# Linux gets Chromium from Nix. Run this once on macOS.
browsers:
    PLAYWRIGHT_SKIP_BROWSER_GC=1 npx playwright install chromium

dev:
    npm run dev

build:
    npm run build

check-examples:
    npm run check:examples

check-agent-docs:
    npm run check:agent-docs

lint:
    npm run lint

format-check:
    npm run format-check

format:
    npm run format

test:
    npm test

test-browser:
    npm run test:browser

compose:
    node scripts/compose.mjs

check:
    node scripts/check-site.mjs --built
