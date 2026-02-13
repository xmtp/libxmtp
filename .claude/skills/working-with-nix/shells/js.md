# JavaScript Shell (`nix develop .#js`)

For JavaScript/Node.js development and browser testing. **No Rust toolchain** — this is a pure JS/browser shell.

**Source:** `nix/js.nix`

## Environment Variables

| Variable | Value | Purpose |
|----------|-------|---------|
| `PLAYWRIGHT_BROWSERS_PATH` | Browser path | Pre-built browsers |
| `PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS` | `true` | Skip host checks |
| `PLAYWRIGHT_VERSION` | Version string | Playwright version |
| `VITE_PROJECT_ID` | `2ca676e2e5e9322c40c68f10dca637e5` | Vite configuration |

## Tools Included

- `corepack` — Node.js package managers (yarn, pnpm)
- `playwright` — Browser automation
- `playwright-driver.browsers` — Pre-built browsers
- `geckodriver` — Firefox WebDriver
- `buf` — Protocol buffers
- `curl` — HTTP client
- `mktemp` — Temporary file creation
- Darwin only: `darwin.cctools`

## Use Cases

- Running `yarn` and Node.js scripts
- Browser-based testing with Playwright
- Protocol buffer code generation
