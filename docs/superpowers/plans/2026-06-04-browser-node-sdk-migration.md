# Browser SDK + Node SDK Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move `@xmtp/browser-sdk` and `@xmtp/node-sdk` (with history) from xmtp-js into this monorepo at `sdks/js/`, sourcing the locally-built bindings via Yarn Berry `portal:`, with `just` + sharded CI that reuses the nix-cached binding builds, unified formatting via treefmt, and a release path that piggybacks the bindings nightly cadence.

**Architecture:** A new Yarn Berry workspace root at `sdks/js/` holds both SDKs. They reference `bindings/node` / `bindings/wasm` (siblings) via `portal:` paths; a `dev/bindings` script nix-builds the test-utils binding variants and copies their `dist/` into the binding dirs so `portal:` resolves real packages. CI gates SDK tests on the existing `test-node`/`test-wasm` jobs (no Rust rebuild — nix cache hit) and shards vitest across a matrix. Formatting joins treefmt; eslint+tsc run in a `lint-js` workflow.

**Tech Stack:** Yarn Berry 4.10.x (`node-modules` linker), Turborepo-free per-workspace scripts, Vitest (node + browser/Playwright), rollup, Nix flake (`#js` devShell, `node-bindings-test`/`wasm-bindings-test` packages), `git-filter-repo`, jj.

**Scope:** This plan covers the **libxmtp side** (Phases A + B from the spec): import, wiring, CI, release. The **xmtp-js teardown** (Phase C — different repo, must happen after first publish) is captured as a documented checklist in Task 13, not executed here.

**VCS note:** This repo is jj (`.jj/` present). Use `jj desc -m`/`jj new` per the jujutsu skill; the `git commit` lines below are the logical commit boundaries — translate each to `jj` (working copy is already a commit; `jj desc -m "..."` then `jj new` to start the next). The history import (Task 1) is special and uses `git-filter-repo` on a `/tmp` clone + `jj git fetch`.

---

## File Structure

**Created:**
- `sdks/js/package.json` — private Yarn Berry workspace root (workspaces: browser-sdk, node-sdk)
- `sdks/js/.yarnrc.yml` — `nodeLinker: node-modules`, yarn release path
- `sdks/js/.yarn/releases/yarn-4.10.3.cjs` — pinned yarn (copied from xmtp-js)
- `sdks/js/js.just` — just recipes (install/bindings/check/build/test/test-*-ci)
- `sdks/js/dev/.setup` — repo-root + nix-shell helper (JS variant of android's)
- `sdks/js/dev/bindings` — nix-build test-utils bindings → populate `bindings/<x>/dist`
- `sdks/js/tsconfig.json` — workspace tsconfig (references the two SDKs)
- `sdks/js/eslint.config.js` — flat eslint config for the workspace (ported, prettier plugin removed)
- `.github/workflows/test-node-sdk.yml` — sharded node-sdk test (reusable)
- `.github/workflows/test-browser-sdk.yml` — sharded browser-sdk test (reusable)
- `.github/workflows/lint-js.yml` — eslint + tsc (reusable)
- `.github/workflows/release-node-sdk.yml` — npm publish node-sdk
- `.github/workflows/release-browser-sdk.yml` — npm publish browser-sdk

**Imported with history (Task 1), then edited:**
- `sdks/js/browser-sdk/**` (from xmtp-js `sdks/browser-sdk`)
- `sdks/js/node-sdk/**` (from xmtp-js `sdks/node-sdk`)
- `sdks/js/browser-sdk/package.json` — binding dep → `portal:`
- `sdks/js/node-sdk/package.json` — binding dep → `portal:`

**Modified:**
- `justfile` — add `mod js 'sdks/js/js.just'`
- `.github/workflows/test.yml` — detect-changes outputs + filters + two gated jobs + aggregate `needs`
- `.github/workflows/lint.yml` — add `lint-js` to fan-out
- `nix/fmt.nix` — add `prettier` program scoped to `sdks/js/**`
- `.gitignore` — ensure `sdks/js/**/dist`, `bindings/*/dist`, `bindings/*/.nix-dist` ignored (verify; mostly covered)

---

## Task 1: Import the two SDKs with history

**Files:**
- Create (via import): `sdks/js/browser-sdk/**`, `sdks/js/node-sdk/**`

- [ ] **Step 1: Get git-filter-repo into /tmp**

Run:
```bash
curl -fsSL https://raw.githubusercontent.com/newren/git-filter-repo/main/git-filter-repo \
  -o /tmp/git-filter-repo && chmod +x /tmp/git-filter-repo
/tmp/git-filter-repo --version
```
Expected: prints a version string (e.g. `fb3de42e`).

- [ ] **Step 2: Clone xmtp-js fresh and extract both folders with history**

Run:
```bash
rm -rf /tmp/xmtp-js-extract
git clone https://github.com/xmtp/xmtp-js.git /tmp/xmtp-js-extract
cd /tmp/xmtp-js-extract
/tmp/git-filter-repo \
  --path sdks/browser-sdk/ --path-rename sdks/browser-sdk/:sdks/js/browser-sdk/ \
  --path sdks/node-sdk/    --path-rename sdks/node-sdk/:sdks/js/node-sdk/
```

- [ ] **Step 3: Verify the extraction**

Run:
```bash
cd /tmp/xmtp-js-extract
git ls-tree -r --name-only HEAD | grep -vE '^sdks/js/(browser-sdk|node-sdk)/' || echo "CLEAN: only the two SDKs remain"
git rev-list --count HEAD
```
Expected: prints `CLEAN: only the two SDKs remain` and a commit count around `415`.

- [ ] **Step 4: Land into the jj monorepo from the colocated main workspace**

The working dir is a secondary jj workspace; the git store lives at the colocated `main/` workspace. Determine it:
```bash
MAIN_WS="$(jj root)/../main"   # adjust if your main workspace lives elsewhere
ls "$MAIN_WS/.git" >/dev/null && echo "colocated main found at $MAIN_WS"
```
Then, from that main workspace, ensure clean, fetch, and merge:
```bash
cd "$MAIN_WS"
jj st                                            # working copy should be clean
jj git remote add xmtpjs /tmp/xmtp-js-extract
jj git fetch --remote xmtpjs --branch main
jj new main main@xmtpjs -m "Import xmtp-js browser-sdk + node-sdk history into sdks/js"
```

- [ ] **Step 5: Verify the merge brought in the SDKs with history**

Run:
```bash
jj st                                            # expect sdks/js/browser-sdk + sdks/js/node-sdk present
jj log -r '::@ & files("sdks/js/browser-sdk")' --no-graph -T 'description.first_line() ++ "\n"' | head
```
Expected: working copy shows the two SDK trees; log shows imported commits (e.g. renovate "Update xmtp bindings", original SDK commits) with their original authorship.

- [ ] **Step 6: Advance the bookmark and clean up the temp remote**

```bash
jj bookmark move main --to @
jj git remote remove xmtpjs
jj st
```
Expected: `main` bookmark now at the merge commit; no `xmtpjs` remote.

> Note: this is the one task that uses raw git (in /tmp) + jj git fetch. All later tasks add files in the current workspace and commit with `jj desc`/`jj new`.

---

## Task 2: Workspace root scaffolding

**Files:**
- Create: `sdks/js/package.json`, `sdks/js/.yarnrc.yml`, `sdks/js/.yarn/releases/yarn-4.10.3.cjs`

- [ ] **Step 1: Copy the pinned yarn release from the extract**

```bash
cd /home/insipx/code/xmtp/workspaces/libxmtp/insipx/xmtp-js-monorepo
mkdir -p sdks/js/.yarn/releases
cp /tmp/xmtp-js-extract/.yarn/releases/yarn-4.10.3.cjs sdks/js/.yarn/releases/ 2>/dev/null \
  || cp /tmp/xmtp-js-research/.yarn/releases/yarn-*.cjs sdks/js/.yarn/releases/
ls sdks/js/.yarn/releases/
```
Expected: a `yarn-4.10.3.cjs` (or matching pinned version) file present.

- [ ] **Step 2: Write the workspace root package.json**

Create `sdks/js/package.json`:
```json
{
  "name": "@xmtp/js-sdks",
  "version": "0.0.0",
  "private": true,
  "type": "module",
  "packageManager": "yarn@4.10.3",
  "workspaces": ["browser-sdk", "node-sdk"],
  "scripts": {
    "typecheck": "yarn workspaces foreach -A run typecheck",
    "build": "yarn workspaces foreach -A run build",
    "lint": "eslint .",
    "test": "yarn workspaces foreach -A run test"
  },
  "devDependencies": {
    "@eslint/compat": "^1.4.0",
    "@eslint/js": "^9.40.0",
    "eslint": "^9.40.0",
    "globals": "^16.5.0",
    "typescript": "^5.9.3",
    "typescript-eslint": "^8.46.0"
  },
  "engines": {
    "node": ">=22"
  }
}
```
(devDependency versions: match what xmtp-js root `package.json` pins — verify against `/tmp/xmtp-js-research/package.json` and adjust if they differ.)

- [ ] **Step 3: Write `.yarnrc.yml`**

Create `sdks/js/.yarnrc.yml`:
```yaml
nodeLinker: node-modules
enableTelemetry: false
yarnPath: .yarn/releases/yarn-4.10.3.cjs
```

- [ ] **Step 4: Verify yarn recognises the workspace**

```bash
cd sdks/js && corepack enable 2>/dev/null; yarn workspaces list
```
Expected: lists `.`, `browser-sdk`, `node-sdk` (the SDK package dirs). If `yarn` isn't found, run inside the nix js shell: `nix develop ../../#js --command yarn workspaces list`.

- [ ] **Step 5: Commit**

```bash
# jj: jj desc -m "feat(sdks/js): add Yarn Berry workspace root" ; then jj new
git add sdks/js/package.json sdks/js/.yarnrc.yml sdks/js/.yarn
git commit -m "feat(sdks/js): add Yarn Berry workspace root"
```

---

## Task 3: Point SDK binding deps at portal:

**Files:**
- Modify: `sdks/js/browser-sdk/package.json` (dependencies)
- Modify: `sdks/js/node-sdk/package.json` (dependencies)

- [ ] **Step 1: Confirm the binding package names match**

```bash
cd /home/insipx/code/xmtp/workspaces/libxmtp/insipx/xmtp-js-monorepo
grep '"name"' bindings/node/package.json bindings/wasm/package.json
```
Expected: `@xmtp/node-bindings` and `@xmtp/wasm-bindings`. If either differs, the `portal:` target resolves by directory regardless, but the SDK import specifier must match the binding's `name` — note any mismatch.

- [ ] **Step 2: Edit browser-sdk binding dep to portal:**

In `sdks/js/browser-sdk/package.json`, change the dependency:
```json
"dependencies": {
  "@xmtp/content-type-primitives": "3.0.0",
  "@xmtp/wasm-bindings": "portal:../../../bindings/wasm"
}
```

- [ ] **Step 3: Edit node-sdk binding dep to portal:**

In `sdks/js/node-sdk/package.json`, change the dependency:
```json
"dependencies": {
  "@xmtp/content-type-primitives": "3.0.0",
  "@xmtp/node-bindings": "portal:../../../bindings/node"
}
```
(Keep node-sdk's other deps unchanged; only the binding line changes.)

- [ ] **Step 4: Commit**

```bash
# jj: jj desc -m "feat(sdks/js): source bindings via portal: to local binding dirs" ; then jj new
git add sdks/js/browser-sdk/package.json sdks/js/node-sdk/package.json
git commit -m "feat(sdks/js): source bindings via portal: to local binding dirs"
```

---

## Task 4: dev scripts (.setup + bindings)

**Files:**
- Create: `sdks/js/dev/.setup`, `sdks/js/dev/bindings`

- [ ] **Step 1: Write `sdks/js/dev/.setup`**

Create `sdks/js/dev/.setup` (modeled on `sdks/android/dev/.setup`):
```bash
#!/bin/bash
# Common setup for JS SDK dev scripts. Source this file, don't execute directly.
set -eou pipefail

if ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"; then
    :
elif ROOT="$(jj root 2>/dev/null)"; then
    :
else
    echo "error: not in a git or jj working tree" >&2
    exit 1
fi
SDK_ROOT="${ROOT}/sdks/js"

ensure_nix_shell() {
    if [[ "${XMTP_DEV_SHELL:-}" != "js" && "${XMTP_DEV_SHELL:-}" != "local" ]]; then
        exec nix develop "${ROOT}#js" --command "$0" "$@"
    fi
}
```

- [ ] **Step 2: Write `sdks/js/dev/bindings`**

Create `sdks/js/dev/bindings`:
```bash
#!/bin/bash
# Build the test-utils JS bindings via Nix and populate bindings/<x>/dist so
# the SDKs' portal: deps resolve real packages. 100% cache hit when the
# node/wasm binding test jobs already built these.
source "$(dirname "$0")/.setup"

nix_system="$(nix eval --impure --raw --expr 'builtins.currentSystem')"

build_and_stage() {
  local pkg="$1" dir="$2"
  echo "Building ${pkg} via Nix..."
  nix build "${ROOT}#packages.${nix_system}.${pkg}" \
    --out-link "${ROOT}/bindings/${dir}/.nix-dist"
  mkdir -p "${ROOT}/bindings/${dir}/dist"
  cp -rL "${ROOT}/bindings/${dir}/.nix-dist/dist/." "${ROOT}/bindings/${dir}/dist/"
}

build_and_stage node-bindings-test node
build_and_stage wasm-bindings-test wasm

echo "JS bindings staged:"
echo "  node: ${ROOT}/bindings/node/dist"
echo "  wasm: ${ROOT}/bindings/wasm/dist"
```

- [ ] **Step 3: Make them executable**

```bash
chmod +x sdks/js/dev/.setup sdks/js/dev/bindings
```

- [ ] **Step 4: Run dev/bindings and verify dist is populated**

```bash
./sdks/js/dev/bindings
ls sdks/js/../../bindings/node/dist/*.node bindings/node/dist/index.js bindings/wasm/dist/*.wasm
```
Expected: a `bindings_node.<target>.node` + `index.js` in `bindings/node/dist`, and a `.wasm` + JS glue in `bindings/wasm/dist`. (First run compiles or pulls from cachix; subsequent runs are instant.)

- [ ] **Step 5: Commit**

```bash
# jj: jj desc -m "feat(sdks/js): add dev/.setup + dev/bindings (nix-staged bindings)" ; then jj new
git add sdks/js/dev/.setup sdks/js/dev/bindings
git commit -m "feat(sdks/js): add dev/.setup + dev/bindings (nix-staged bindings)"
```

---

## Task 5: Install + verify portal resolution

**Files:**
- Create (generated): `sdks/js/yarn.lock`

- [ ] **Step 1: Install dependencies in the js shell**

```bash
cd /home/insipx/code/xmtp/workspaces/libxmtp/insipx/xmtp-js-monorepo
nix develop .#js --command bash -euc 'cd sdks/js && corepack enable && yarn install'
```
Expected: install completes; a `sdks/js/yarn.lock` is created. The `@xmtp/node-bindings` / `@xmtp/wasm-bindings` entries resolve via `portal:`.

- [ ] **Step 2: Verify the portal-linked binding is importable**

```bash
ls sdks/js/node_modules/@xmtp/node-bindings/dist/*.node
ls sdks/js/node_modules/@xmtp/wasm-bindings/dist/*.wasm
```
Expected: the `.node` / `.wasm` artifacts are present under the portal-linked package (symlinked into `bindings/<x>` which now has a populated `dist/` from Task 4).

- [ ] **Step 3: Commit the lockfile**

```bash
# jj: jj desc -m "feat(sdks/js): add yarn.lock (portal-linked bindings)" ; then jj new
git add sdks/js/yarn.lock
git commit -m "feat(sdks/js): add yarn.lock (portal-linked bindings)"
```

---

## Task 6: js.just recipes + root justfile wiring

**Files:**
- Create: `sdks/js/js.just`
- Modify: `justfile:1-4` (add `mod js`)

- [ ] **Step 1: Write `sdks/js/js.just`**

Create `sdks/js/js.just`:
```just
export NIX_DEVSHELL := env("NIX_DEVSHELL", "js")

set shell := ["../../dev/nix-shell"]

# Install JS dependencies
install:
    cd {{ justfile_directory() }} && yarn

# Install JS dependencies (CI - frozen lockfile)
install-ci:
    cd {{ justfile_directory() }} && yarn install --immutable

# Build + stage the local bindings into bindings/<x>/dist via Nix
bindings:
    {{ justfile_directory() }}/dev/bindings

# Typecheck both SDKs
check: install
    cd {{ justfile_directory() }} && yarn workspaces foreach -A run typecheck

# Build both SDKs (needs bindings present)
build: install bindings
    cd {{ justfile_directory() }} && yarn workspaces foreach -A run build

# Lint (eslint) both SDKs (needs bindings for type-aware rules)
lint: install bindings
    cd {{ justfile_directory() }} && yarn lint

# Run all JS SDK tests locally (builds bindings first)
test: install bindings
    cd {{ justfile_directory() }} && yarn workspace @xmtp/node-sdk run test
    cd {{ justfile_directory() }} && yarn workspace @xmtp/browser-sdk run test

# CI: node-sdk tests. Bindings are a nix cache hit (built by test-node job).
# Extra args (e.g. --shard N/M) are forwarded to vitest.
test-node-sdk-ci *args="": install-ci bindings
    cd {{ justfile_directory() }} && yarn workspace @xmtp/node-sdk run test {{ args }}

# CI: browser-sdk tests (Playwright). Bindings are a nix cache hit (test-wasm).
test-browser-sdk-ci *args="": install-ci bindings
    cd {{ justfile_directory() }} && yarn workspace @xmtp/browser-sdk run test {{ args }}
```
(`justfile_directory()` resolves to `sdks/js`. The `cd` keeps recipes runnable from the repo root via `just js <recipe>`.)

- [ ] **Step 2: Wire the module into the root justfile**

In `justfile`, add after the existing `mod` lines (after line 4):
```just
mod js 'sdks/js/js.just'
```
Result (top of justfile):
```just
mod android 'sdks/android/android.just'
mod ios 'sdks/ios/ios.just'
mod node 'bindings/node/node.just'
mod wasm 'bindings/wasm/wasm.just'
mod js 'sdks/js/js.just'
```

- [ ] **Step 3: Verify the recipes are discoverable**

```bash
just --list --list-submodules | grep -A8 '^    js'
```
Expected: lists `js install`, `js install-ci`, `js bindings`, `js check`, `js build`, `js lint`, `js test`, `js test-node-sdk-ci`, `js test-browser-sdk-ci`.

- [ ] **Step 4: Run node-sdk tests locally end-to-end**

```bash
just backend up
just js test-node-sdk-ci
```
Expected: backend starts; node-sdk vitest runs against the portal-linked nix binding and passes. (If the binding `dist` is stale, `just js bindings` refreshes it — it's a recipe dependency.)

- [ ] **Step 5: Commit**

```bash
# jj: jj desc -m "feat(sdks/js): add js.just recipes and wire into root justfile" ; then jj new
git add sdks/js/js.just justfile
git commit -m "feat(sdks/js): add js.just recipes and wire into root justfile"
```

---

## Task 7: Workspace tsconfig + eslint config

**Files:**
- Create: `sdks/js/tsconfig.json`, `sdks/js/eslint.config.js`

- [ ] **Step 1: Inspect the imported per-SDK tsconfigs**

```bash
cat sdks/js/browser-sdk/tsconfig.json sdks/js/node-sdk/tsconfig.json
```
Note their `extends`/`compilerOptions` so the workspace tsconfig is compatible (they likely extend a shared base in xmtp-js root — recreate that base here).

- [ ] **Step 2: Write the workspace base tsconfig**

Create `sdks/js/tsconfig.json` (port the relevant base from xmtp-js root `tsconfig.json` in `/tmp/xmtp-js-research/tsconfig.json`; typical content):
```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "lib": ["ES2023", "DOM", "DOM.Iterable"],
    "strict": true,
    "skipLibCheck": true,
    "esModuleInterop": true,
    "forceConsistentCasingInFileNames": true,
    "verbatimModuleSyntax": true,
    "resolveJsonModule": true,
    "declaration": true,
    "noEmit": true
  },
  "include": ["browser-sdk/src", "node-sdk/src"]
}
```
Adjust `include`/`compilerOptions` to match exactly what the imported per-SDK `tsconfig.json` files expect (they `extends` this base). If each SDK has a self-contained tsconfig, this root one only needs `files: []` + `references` to the two; verify by running typecheck in Step 5.

- [ ] **Step 3: Port the eslint flat config (drop the prettier plugin)**

Create `sdks/js/eslint.config.js`, adapted from xmtp-js `eslint.config.js`. Remove `eslint-plugin-prettier/recommended` (formatting is delegated to treefmt in Task 11) and the prettier-related rules, keeping the type-checked ruleset:
```js
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { includeIgnoreFile } from "@eslint/compat";
import eslint from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const gitignorePath = path.resolve(__dirname, "../../.gitignore");

export default tseslint.config(
  includeIgnoreFile(gitignorePath),
  {
    ignores: [".yarn/**/*", "**/dist/**/*"],
  },
  eslint.configs.recommended,
  ...tseslint.configs.strictTypeChecked,
  {
    languageOptions: {
      parserOptions: {
        projectService: {
          defaultProject: "tsconfig.json",
        },
        tsconfigRootDir: __dirname,
      },
    },
  },
  {
    rules: {
      "@typescript-eslint/no-unnecessary-type-parameters": "off",
      "@typescript-eslint/consistent-type-imports": "error",
      "@typescript-eslint/consistent-type-exports": [
        "error",
        { fixMixedExportsWithInlineTypeSpecifier: false },
      ],
    },
  },
);
```
(Copy the FULL rule set from the imported xmtp-js `eslint.config.js` — the head was inspected; preserve all rules except the prettier integration. Add eslint devDeps already declared in Task 2.)

- [ ] **Step 4: Verify typecheck + eslint run**

```bash
nix develop .#js --command bash -euc 'cd sdks/js && yarn workspaces foreach -A run typecheck && yarn lint'
```
Expected: tsc reports no errors (bindings present from Task 4); eslint runs clean (or reports only real issues to fix). The binding `.d.ts` resolves via the portal link.

- [ ] **Step 5: Commit**

```bash
# jj: jj desc -m "feat(sdks/js): add workspace tsconfig + eslint config" ; then jj new
git add sdks/js/tsconfig.json sdks/js/eslint.config.js
git commit -m "feat(sdks/js): add workspace tsconfig + eslint config"
```

---

## Task 8: Reusable test workflow — node-sdk (sharded)

**Files:**
- Create: `.github/workflows/test-node-sdk.yml`

- [ ] **Step 1: Write the workflow**

Create `.github/workflows/test-node-sdk.yml`:
```yaml
name: Test Node SDK
on:
  workflow_call:
env:
  NIX_DEVSHELL: js
jobs:
  test:
    name: Test (node-sdk) shard ${{ matrix.shard }}
    runs-on: warp-ubuntu-latest-x64-16x
    timeout-minutes: 30
    strategy:
      fail-fast: false
      matrix:
        shard: [1, 2]
    steps:
      - name: Checkout
        uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
          with-warpbuild-cache: "false"
      - uses: taiki-e/install-action@just
      - name: Install dependencies
        run: just js install-ci
      - name: Start backend
        run: just backend up
      - name: Run node-sdk tests (shard ${{ matrix.shard }}/2)
        run: just js test-node-sdk-ci -- --shard ${{ matrix.shard }}/2
```

- [ ] **Step 2: Sanity-check the YAML**

```bash
nix develop .#default --command bash -euc 'yq . .github/workflows/test-node-sdk.yml >/dev/null && echo OK' \
  || python3 -c "import yaml; yaml.safe_load(open('.github/workflows/test-node-sdk.yml')); print('OK')"
```
Expected: `OK`.

- [ ] **Step 3: Commit**

```bash
# jj: jj desc -m "ci: add sharded test-node-sdk reusable workflow" ; then jj new
git add .github/workflows/test-node-sdk.yml
git commit -m "ci: add sharded test-node-sdk reusable workflow"
```

---

## Task 9: Reusable test workflow — browser-sdk (sharded, Playwright)

**Files:**
- Create: `.github/workflows/test-browser-sdk.yml`

- [ ] **Step 1: Write the workflow**

Create `.github/workflows/test-browser-sdk.yml`:
```yaml
name: Test Browser SDK
on:
  workflow_call:
env:
  NIX_DEVSHELL: js
jobs:
  test:
    name: Test (browser-sdk) shard ${{ matrix.shard }}
    runs-on: warp-ubuntu-latest-x64-16x
    timeout-minutes: 60
    strategy:
      fail-fast: false
      matrix:
        shard: [1, 2, 3, 4]
    steps:
      - name: Checkout
        uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
          cachix-auth-token: ${{ secrets.CACHIX_AUTH_TOKEN }}
          with-warpbuild-cache: "false"
      - uses: taiki-e/install-action@just
      - name: Install dependencies
        run: just js install-ci
      - name: Start backend
        run: just backend up
      - name: Run browser-sdk tests (shard ${{ matrix.shard }}/4)
        run: just js test-browser-sdk-ci -- --shard ${{ matrix.shard }}/4
```
(Playwright browsers come from the `#js` devShell's `PLAYWRIGHT_BROWSERS_PATH`. `cachix-auth-token` is passed so the wasm-bindings-test build can be pulled/pushed — matches `test-wasm.yml`.)

- [ ] **Step 2: Sanity-check the YAML**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/test-browser-sdk.yml')); print('OK')"
```
Expected: `OK`.

- [ ] **Step 3: Commit**

```bash
# jj: jj desc -m "ci: add sharded test-browser-sdk reusable workflow" ; then jj new
git add .github/workflows/test-browser-sdk.yml
git commit -m "ci: add sharded test-browser-sdk reusable workflow"
```

---

## Task 10: Wire SDK test jobs into test.yml with gating

**Files:**
- Modify: `.github/workflows/test.yml` (detect-changes outputs + filters; two new jobs; aggregate `needs`)

- [ ] **Step 1: Add detect-changes outputs**

In `.github/workflows/test.yml`, in the `detect-changes` job `outputs:` map, add:
```yaml
      node_sdk: ${{ steps.filter.outputs.node_sdk }}
      browser_sdk: ${{ steps.filter.outputs.browser_sdk }}
```

- [ ] **Step 2: Add detect-changes filters**

In the `dorny/paths-filter` `filters:` block, add:
```yaml
            node_sdk:
              - 'sdks/js/node-sdk/**'
              - 'sdks/js/package.json'
              - 'sdks/js/yarn.lock'
              - 'sdks/js/.yarnrc.yml'
              - 'sdks/js/tsconfig.json'
              - 'sdks/js/eslint.config.js'
              - 'sdks/js/js.just'
              - 'sdks/js/dev/**'
              - 'bindings/node/**'
              - 'crates/**'
              - 'nix/**'
              - 'flake.lock'
              - 'dev/docker/**'
              - '.github/workflows/test-node-sdk*'
            browser_sdk:
              - 'sdks/js/browser-sdk/**'
              - 'sdks/js/package.json'
              - 'sdks/js/yarn.lock'
              - 'sdks/js/.yarnrc.yml'
              - 'sdks/js/tsconfig.json'
              - 'sdks/js/eslint.config.js'
              - 'sdks/js/js.just'
              - 'sdks/js/dev/**'
              - 'bindings/wasm/**'
              - 'crates/**'
              - 'nix/**'
              - 'flake.lock'
              - 'dev/docker/**'
              - '.github/workflows/test-browser-sdk*'
```

- [ ] **Step 3: Add the two gated jobs**

After the `test-wasm:` job block, add:
```yaml
  test-node-sdk:
    needs: [detect-changes, test-node]
    if: >-
      !cancelled() && needs.test-node.result != 'failure'
      && needs.detect-changes.outputs.node_sdk == 'true'
    uses: ./.github/workflows/test-node-sdk.yml
    secrets: inherit

  test-browser-sdk:
    needs: [detect-changes, test-wasm]
    if: >-
      !cancelled() && needs.test-wasm.result != 'failure'
      && needs.detect-changes.outputs.browser_sdk == 'true'
    uses: ./.github/workflows/test-browser-sdk.yml
    secrets: inherit
```

- [ ] **Step 4: Add them to the aggregate `test` job's `needs`**

In the final `test:` job, add to `needs:`:
```yaml
      - test-node-sdk
      - test-browser-sdk
```
(The aggregate already fails on any `failure`/`cancelled` in `needs.*.result`. A skipped SDK job has result `skipped`, which is neither — so it won't fail the gate. This is correct: SDK job skips when its paths didn't change.)

- [ ] **Step 5: Validate YAML + gating logic on paper**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/test.yml')); print('OK')"
```
Expected: `OK`. Reason through: (a) change only `sdks/js/node-sdk/**` → `node_sdk=true`, `node=false` → `test-node` skipped (result `skipped`) → `test-node-sdk` `if` passes (`!= 'failure'` true, `node_sdk==true`) → runs. (b) change `bindings/node/**` → both `node` and `node_sdk` true → `test-node` runs first, `test-node-sdk` waits, runs on success. (c) `test-node` fails → `test-node-sdk` `if` fails → skipped. Correct.

- [ ] **Step 6: Commit**

```bash
# jj: jj desc -m "ci: gate sdk tests on binding tests in test.yml" ; then jj new
git add .github/workflows/test.yml
git commit -m "ci: gate sdk tests on binding tests in test.yml"
```

---

## Task 11: Unified formatting via treefmt

**Files:**
- Modify: `nix/fmt.nix` (add `prettier` program)

- [ ] **Step 1: Add the prettier program to treefmt**

In `nix/fmt.nix`, inside `treefmt.programs`, add:
```nix
          prettier = {
            enable = true;
            includes = [
              "sdks/js/**/*.ts"
              "sdks/js/**/*.tsx"
              "sdks/js/**/*.js"
              "sdks/js/**/*.cjs"
              "sdks/js/**/*.mjs"
              "sdks/js/**/*.json"
              "sdks/js/**/*.md"
            ];
            excludes = [
              "sdks/js/**/dist/**"
              "sdks/js/.yarn/**"
              "**/node_modules/**"
            ];
          };
```
Then add prettier settings matching xmtp-js `.prettierrc.cjs` under `settings.formatter.prettier` (treefmt passes these as CLI options). The xmtp-js prettier uses import-sorting + packagejson plugins; those are NOT bundled with treefmt's prettier. Set the core options and skip the plugins (import order becomes a non-enforced convention; eslint's `consistent-type-imports` still runs in lint-js):
```nix
        settings.formatter.prettier.options = [
          "--print-width" "80"
          "--tab-width" "2"
          "--trailing-comma" "all"
          "--bracket-same-line" "true"
        ];
```
(These mirror `.prettierrc.cjs`. If exact-match formatting incl. import sort is required, that stays enforced by eslint-plugin-prettier in the SDK; but per the design, prettier-via-treefmt is the formatter of record and the eslint prettier plugin was dropped in Task 7.)

- [ ] **Step 2: Run treefmt and verify it touches JS files**

```bash
nix fmt -- sdks/js/node-sdk/src 2>&1 | tail -5
```
Expected: treefmt runs prettier over the JS/TS files (reformats or reports clean). No error about unknown formatter.

- [ ] **Step 3: Verify the lint-config gate sees JS**

```bash
nix fmt -- --fail-on-change 2>&1 | tail -10 || echo "found unformatted files (expected if any remain)"
```
Expected: either clean, or lists JS files needing format (then run `nix fmt` to fix and re-run until clean).

- [ ] **Step 4: Commit**

```bash
# jj: jj desc -m "feat(nix): format sdks/js via treefmt prettier" ; then jj new
git add nix/fmt.nix
git commit -m "feat(nix): format sdks/js via treefmt prettier"
```

---

## Task 12: lint-js workflow (eslint + tsc) wired into lint.yml

**Files:**
- Create: `.github/workflows/lint-js.yml`
- Modify: `.github/workflows/lint.yml` (detect-changes + job + aggregate)

- [ ] **Step 1: Read lint.yml to match its structure**

```bash
cat .github/workflows/lint.yml
```
Note its `detect-changes` outputs/filters and the aggregate `lint` job's `needs` list (mirror `lint-node`).

- [ ] **Step 2: Write `.github/workflows/lint-js.yml`**

Create `.github/workflows/lint-js.yml`:
```yaml
name: Lint JS
on:
  workflow_call:
env:
  NIX_DEVSHELL: js
jobs:
  lint:
    name: Lint (JS SDKs)
    runs-on: warp-ubuntu-latest-x64-16x
    timeout-minutes: 30
    steps:
      - name: Checkout
        uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
          with-warpbuild-cache: "false"
      - uses: taiki-e/install-action@just
      - name: Install dependencies
        run: just js install-ci
      - name: Build bindings (typecheck needs the .d.ts)
        run: just js bindings
      - name: Typecheck
        run: just js check
      - name: ESLint
        run: just js lint
```

- [ ] **Step 3: Wire into lint.yml**

In `.github/workflows/lint.yml`, add a `js` output + filter to `detect-changes` (filter paths: `sdks/js/**`, `bindings/node/**`, `bindings/wasm/**`, `crates/**`, `nix/**`, `flake.lock`, `.github/workflows/lint-js*`), add the job:
```yaml
  lint-js:
    needs: detect-changes
    if: needs.detect-changes.outputs.js == 'true'
    uses: ./.github/workflows/lint-js.yml
    secrets: inherit
```
and add `- lint-js` to the aggregate `lint` job's `needs:`.

- [ ] **Step 4: Validate YAML**

```bash
python3 -c "import yaml; [yaml.safe_load(open(f)) for f in ['.github/workflows/lint-js.yml','.github/workflows/lint.yml']]; print('OK')"
```
Expected: `OK`.

- [ ] **Step 5: Commit**

```bash
# jj: jj desc -m "ci: add lint-js (eslint + tsc) workflow" ; then jj new
git add .github/workflows/lint-js.yml .github/workflows/lint.yml
git commit -m "ci: add lint-js (eslint + tsc) workflow"
```

---

## Task 13: Release workflows + xmtp-js teardown checklist

**Files:**
- Create: `.github/workflows/release-node-sdk.yml`, `.github/workflows/release-browser-sdk.yml`
- Create: `docs/superpowers/specs/xmtp-js-teardown-checklist.md` (the Phase C follow-up)

- [ ] **Step 1: Read the existing release patterns**

```bash
cat .github/workflows/release-node.yml .github/workflows/release-wasm.yml .github/workflows/npm-publish.yml
```
Note: how they `nix build` the publishable binding (`.#node-bindings-js` / `.#wasm-bindings`), build the artifact, and call `npm-publish.yml` (version via `xmtp-release set-manifest-version`, `npm publish --provenance --tag`).

- [ ] **Step 2: Write release-node-sdk.yml**

Create `.github/workflows/release-node-sdk.yml` modeled on `release-node.yml`: a `build` job that runs in the `#js` shell — `nix build .#packages.<system>.node-bindings-js` (publishable, no test-utils), stages into `bindings/node/dist`, `yarn install --immutable`, `yarn workspace @xmtp/node-sdk run build` — uploads `sdks/js/node-sdk/dist` as an artifact, then a `publish` job calling `npm-publish.yml` with `package-dir: sdks/js/node-sdk` and the appropriate `--tag` (latest for stable, nightly to match the bindings nightly cadence). Use `release-node.yml` as the structural template; substitute the build/publish dirs.

(Exact YAML: copy `release-node.yml`, replace the binding-only build with the SDK build above, set `working-directory`/`package-dir` to the node-sdk path, and trigger on the same nightly/dispatch events libxmtp uses for bindings.)

- [ ] **Step 3: Write release-browser-sdk.yml**

Create `.github/workflows/release-browser-sdk.yml` analogously, modeled on `release-wasm.yml`: build `.#packages.<system>.wasm-bindings`, stage into `bindings/wasm/dist`, `yarn workspace @xmtp/browser-sdk run build`, publish `sdks/js/browser-sdk`.

- [ ] **Step 4: Validate YAML**

```bash
python3 -c "import yaml; [yaml.safe_load(open(f)) for f in ['.github/workflows/release-node-sdk.yml','.github/workflows/release-browser-sdk.yml']]; print('OK')"
```
Expected: `OK`.

- [ ] **Step 5: Write the xmtp-js teardown checklist (Phase C, executed later in the other repo)**

Create `docs/superpowers/specs/xmtp-js-teardown-checklist.md`:
```markdown
# xmtp-js Teardown (Phase C) — run AFTER browser/node-sdk first publish from libxmtp

Prerequisite: `@xmtp/browser-sdk` and `@xmtp/node-sdk` have been published from
libxmtp (Tasks in the libxmtp plan, Phase B) so the npm versions exist.

In the xmtp/xmtp-js repo:
1. Remove `sdks/browser-sdk` and `sdks/node-sdk`.
2. Remove their CI: `.github/workflows/browser-sdk.yml`, `node-sdk.yml`.
3. In `.github/workflows/release.yml`, drop the `prerelease`/auto-prerelease
   handling for browser-sdk + node-sdk (that automation now lives in libxmtp).
   Keep agent-sdk handling.
4. Rewire consumers to published npm versions:
   - `apps/xmtp.chat`: `@xmtp/browser-sdk: workspace:^` -> `^7.x` (published).
   - `sdks/agent-sdk`: `@xmtp/node-sdk: 6.0.0` -> `^6.x` (published).
   - `packages/xmtp-cli`: `@xmtp/node-sdk: 6.0.0` -> `^6.x` (published).
   - content-types devDeps already use published `@xmtp/node-sdk@4.6.0`; bump as needed.
5. Update `renovate.json` to also track `@xmtp/node-sdk` / `@xmtp/browser-sdk`.
6. Remove the now-orphaned changeset config entries for the two SDKs.
7. `yarn install` to regenerate the lockfile; run agent-sdk / xmtp-cli / xmtp.chat
   builds to confirm they resolve the published SDKs.
8. Document the cross-repo invariant: `@xmtp/content-type-primitives` and the
   (now-external) `@xmtp/node-sdk` must reference the same `@xmtp/node-bindings`
   version; Renovate keeps both on the same published nightly.
```

- [ ] **Step 6: Commit**

```bash
# jj: jj desc -m "ci: add sdk release workflows + xmtp-js teardown checklist" ; then jj new
git add .github/workflows/release-node-sdk.yml .github/workflows/release-browser-sdk.yml docs/superpowers/specs/xmtp-js-teardown-checklist.md
git commit -m "ci: add sdk release workflows + xmtp-js teardown checklist"
```

---

## Task 14: Final verification

- [ ] **Step 1: Lint (rust + config + markdown, incl. treefmt JS)**

```bash
just lint-config
just lint-markdown
```
Expected: treefmt (incl. JS prettier) reports clean; markdownlint passes on the new docs.

- [ ] **Step 2: Full local JS test run**

```bash
just backend up
just js test
just backend down
```
Expected: both SDKs' vitest suites pass against the portal-linked nix bindings.

- [ ] **Step 3: Confirm bindings stay gitignored**

```bash
git status --porcelain bindings/node/dist bindings/wasm/dist sdks/js/node_modules 2>/dev/null | head
```
Expected: empty (all ignored). If anything shows, add to `.gitignore`:
```
bindings/*/dist/
bindings/*/.nix-dist
sdks/js/**/dist/
```

- [ ] **Step 4: Confirm the imported history is intact**

```bash
jj log -r '::@ & files("sdks/js")' --no-graph -T 'commit_id.short() ++ " " ++ author.email() ++ " " ++ description.first_line() ++ "\n"' | wc -l
```
Expected: a few hundred commits with varied original authors — history preserved.

---

## Self-Review Notes

- **Spec coverage:** S1 import → Task 1; S1 layout/scaffold → Tasks 2,4,6,7; S2 portal + dev/bindings + js.just → Tasks 3,4,5,6; S3 sharded tests + gating + lint-js → Tasks 8,9,10,12; S4 treefmt format → Task 11, release → Task 13, teardown → Task 13 checklist. All covered.
- **Phase C** intentionally a checklist (different repo, post-publish), not executable tasks here.
- **Open verification points flagged inline** (not placeholders): exact eslint rule set to copy (Task 7 Step 3), per-SDK tsconfig compatibility (Task 7), release YAML cloned from existing release-node/wasm (Task 13) — each names the concrete source file to copy from.
