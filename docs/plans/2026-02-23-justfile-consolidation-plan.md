# Justfile & Dev Script Consolidation Implementation Plan

> **For Claude:** Execute tasks sequentially. All commits go on the current branch (`02-23-add_more_checks_to_justfile`). Use `git add` + `git commit` (not `gt create`).

**Goal:** Consolidate all check/lint/format/test commands into justfiles with Nix, make CI use `just` exclusively, and remove replaced dev scripts.

**Architecture:** Root justfile uses explicit `nix develop .#<shell>` per recipe with a configurable `NIX_DEVSHELL` env var (defaults to `default`). Module justfiles (android, ios, node, wasm) use `set shell` with the same env var. CI overrides `NIX_DEVSHELL` with leaner shells. `CARGO_TEST_CMD` env var allows CI to wrap tests in `cargo llvm-cov`.

**Tech Stack:** just (1.46.0), Nix flakes, GitHub Actions, cargo/nextest, yarn, gradle, swift

**IMPORTANT:** The full dev shell is named `default` (not `local`). All references use `default`.

---

### Task 1: Root Justfile — Check Recipes

**Files:**
- Modify: `justfile`

**Context:** Currently the root justfile has no `check` recipe. We need `just check` (full workspace) and `just check crate <name>` (specific crate).

**Step 1: Read the current justfile**

Read `justfile` to understand current state.

**Step 2: Write the updated root justfile**

Replace the entire `justfile` with:

```just
mod android 'sdks/android/android.just'
mod ios 'sdks/ios/ios.just'
mod node 'bindings/node/node.just'
mod wasm 'bindings/wasm/wasm.just'

# Nix devShell: defaults to `default` (full dev env). CI overrides with leaner shells.
devshell := env("NIX_DEVSHELL", "default")

# CI overrides to "cargo llvm-cov nextest --no-fail-fast --no-report" for coverage
cargo_test := env("CARGO_TEST_CMD", "cargo nextest run")

default:
  @just --list --list-submodules

# --- CHECK ---

# `just check`, `just check crate xmtp_mls`, `just check crate xmtp_mls xmtp_db`
check target="workspace" *args="":
  @just _check-{{target}} {{args}}

[private]
_check-workspace:
  nix develop .#{{devshell}} --command \
    cargo check --locked --workspace --exclude bindings_wasm

[private]
_check-crate +crates:
  nix develop .#{{devshell}} --command \
    cargo check --locked {{crates}}

# --- LINT ---

lint: lint-rust lint-config lint-markdown

lint-rust:
  nix develop .#{{devshell}} --command \
    cargo clippy --locked --workspace \
    --all-features --all-targets --no-deps --exclude bindings_wasm -- -Dwarnings
  nix develop .#{{devshell}} --command cargo fmt --check
  nix develop .#{{devshell}} --command cargo hakari generate --diff
  nix develop .#{{devshell}} --command cargo hakari manage-deps --dry-run

# Config linting: TOML, Nix, shell scripts
lint-config: lint-toml lint-nix lint-shell

lint-toml:
  nix develop .#{{devshell}} --command taplo format --check --diff
  nix develop .#{{devshell}} --command taplo check

lint-nix:
  nix develop .#{{devshell}} --command nixfmt --check nix/ flake.nix

lint-shell:
  nix fmt -- --fail-on-change

lint-markdown:
  nix develop .#{{devshell}} --command markdownlint "**/*.md" --disable MD001 MD013

# --- FORMAT ---

format: _format-workspace
  just android format
  just ios format
  just node format
  just wasm format

[private]
_format-workspace:
  nix fmt

# --- TEST ---

# `just test`, `just test v3`, `just test d14n`, `just test crate xmtp_mls`
test target="all" *args="":
  @just _test-{{target}} {{args}}

[private]
_test-all: (_test-v3) (_test-d14n)

[private]
_test-v3:
  nix develop .#{{devshell}} --command {{cargo_test}} \
    --profile ci --workspace --exclude bindings_wasm

[private]
_test-d14n:
  nix develop .#{{devshell}} --command {{cargo_test}} \
    --features d14n --profile ci-d14n \
    -E 'package(xmtp_mls)' -E 'rdeps(xmtp_mls)' \
    --workspace --exclude bindings_wasm

[private]
_test-crate +crates:
  nix develop .#{{devshell}} --command {{cargo_test}} \
    {{crates}}

# --- BACKEND ---

# `just backend up`, `just backend down`
backend command="up":
  @just _backend-{{command}}

[private]
_backend-up:
  nix build .#validation-service-image
  dev/docker/up

[private]
_backend-down:
  dev/docker/down
```

**Step 3: Verify root justfile compiles**

Run: `just --list --list-submodules`
Expected: All recipes listed without errors.

**Step 4: Test check recipe**

Run: `just check crate xmtp_common`
Expected: Cargo check passes for xmtp_common crate.

**Step 5: Test lint-toml recipe**

Run: `just lint-toml`
Expected: TOML format check passes.

**Step 6: Test lint-nix recipe**

Run: `just lint-nix`
Expected: Nix format check passes.

**Step 7: Test lint-markdown recipe**

Run: `just lint-markdown`
Expected: Markdown lint passes (or shows expected warnings).

**Step 8: Test lint-shell recipe**

Run: `just lint-shell`
Expected: Shellcheck via treefmt passes.

**Step 9: Test format recipe (workspace only)**

Run: `just _format-workspace`
Expected: treefmt runs (formats Rust, Nix, TOML).

**Step 10: Commit**

```bash
git add justfile
git commit -m "feat: consolidate root justfile with check/lint/format/test recipes

Add configurable NIX_DEVSHELL and CARGO_TEST_CMD env var overrides.
Add check (workspace + crate), lint (rust + config + markdown),
format (treefmt + module delegates), and test (all/v3/d14n/crate) recipes."
```

---

### Task 2: Android Justfile

**Files:**
- Modify: `sdks/android/android.just`

**Context:** Currently has test, test-integration, build. Needs check, lint, format. Uses `set shell` with configurable devshell.

**Step 1: Write the updated android.just**

Replace `sdks/android/android.just` with:

```just
# Defaults to `default` (full env). CI sets NIX_DEVSHELL=android for leaner builds.
devshell := env("NIX_DEVSHELL", "default")
set shell := ["nix", "develop", ".#" + devshell, "--command", "bash", "-c"]

# Verify Android SDK builds (bindings + Gradle)
check:
  ./dev/bindings && ./gradlew -p . build

# Run Spotless + Android Lint
lint:
  ./gradlew -p . spotlessCheck --continue && ./gradlew -p . :library:lintDebug

# Format Kotlin code
format:
  ./gradlew -p . spotlessApply

# Run unit tests
test:
  ./dev/bindings && ./gradlew -p . library:testDebug

# Run integration tests (requires emulator)
test-integration:
  run-test-emulator && ./gradlew -p . connectedCheck --continue

# Build native bindings
build:
  ./dev/bindings
```

**Step 2: Verify android recipes list**

Run: `just android --list`
Expected: check, lint, format, test, test-integration, build listed.

**Step 3: Commit**

```bash
git add sdks/android/android.just
git commit -m "feat: add check/lint/format recipes to android justfile

Use configurable NIX_DEVSHELL with set shell for all recipes."
```

---

### Task 3: iOS Justfile

**Files:**
- Modify: `sdks/ios/ios.just`

**Context:** Currently has test, build. Needs check, lint, format. Darwin only.

**Step 1: Write the updated ios.just**

Replace `sdks/ios/ios.just` with:

```just
# Defaults to `default` (full env). CI sets NIX_DEVSHELL=ios for leaner builds.
# Darwin only — will fail on Linux (no .#ios nix shell)
devshell := env("NIX_DEVSHELL", "default")
set shell := ["nix", "develop", ".#" + devshell, "--command", "bash", "-c"]

# Verify iOS SDK builds (bindings + Swift)
check:
  ./dev/bindings && swift build

# Run SwiftLint + SwiftFormat check
lint:
  swiftlint && swiftformat --lint .

# Format Swift code
format:
  swiftformat .

# Run Swift tests
test:
  ./dev/bindings && swift test --parallel

# Build iOS native bindings (xcframework)
build:
  ./dev/bindings
```

**Step 2: Verify ios recipes list**

Run: `just ios --list`
Expected: check, lint, format, test, build listed.

**Step 3: Test ios lint**

Run: `just ios lint`
Expected: SwiftLint + SwiftFormat check passes.

**Step 4: Commit**

```bash
git add sdks/ios/ios.just
git commit -m "feat: add check/lint/format recipes to ios justfile

Use configurable NIX_DEVSHELL with set shell for all recipes."
```

---

### Task 4: Node Justfile

**Files:**
- Modify: `bindings/node/node.just`

**Context:** Currently has test, build. Needs check, lint, format, install, install-ci. Commands inlined from `package.json` so the justfile is the single source of truth. All JS tools (`napi`, `prettier`, `vitest`) invoked via `yarn` to use the correct version from `node_modules/.bin`.

**Inlined package.json commands:**
- `build` → `rm -rf dist && yarn napi build --platform --release --esm && mkdir -p dist && mv index.js dist && mv index.d.ts dist && mv *.node dist`
- `build:test` → `yarn napi build --platform --esm --features test-utils`
- `format` → `yarn prettier -w .`
- `format:check` → `yarn prettier -c .`
- `test` → clean + build:test + `mkdir -p dist && mv ... && yarn vitest run`

**Step 1: Write the updated node.just**

Replace `bindings/node/node.just` with:

```just
# Defaults to `default` (full env). CI sets NIX_DEVSHELL=js for leaner builds.
devshell := env("NIX_DEVSHELL", "default")
set shell := ["nix", "develop", ".#" + devshell, "--command", "bash", "-c"]

# Install JS dependencies
install:
  yarn

# Install JS dependencies (CI - frozen lockfile)
install-ci:
  yarn install --immutable

# Build Node.js NAPI bindings
check: install
  rm -rf dist && \
  yarn napi build --platform --release --esm && \
  mkdir -p dist && mv index.js dist && mv index.d.ts dist && mv *.node dist

# Check TypeScript formatting
lint: install
  yarn prettier -c .

# Format TypeScript files
format: install
  yarn prettier -w .

# Run Node.js tests (builds NAPI bindings with test features + vitest)
test: install
  rm -rf dist && \
  yarn napi build --platform --esm --features test-utils && \
  mkdir -p dist && mv index.js dist && mv index.d.ts dist && mv *.node dist && \
  yarn vitest run

# Build Node.js NAPI bindings (alias)
build: check
```

**Step 2: Verify node recipes list**

Run: `just node --list`
Expected: install, install-ci, check, lint, format, test, build listed.

**Step 3: Test node lint**

Run: `just node lint`
Expected: Prettier format check passes (runs `yarn` via install dependency, then `yarn prettier -c .`).

**Step 4: Commit**

```bash
git add bindings/node/node.just
git commit -m "feat: add check/lint/format recipes to node justfile

Inline package.json commands into justfile recipes.
Add install/install-ci recipes for dependency management.
All JS tools invoked via yarn for correct node_modules resolution.
Use configurable NIX_DEVSHELL with set shell for all recipes."
```

---

### Task 5: WASM Justfile

**Files:**
- Modify: `bindings/wasm/wasm.just`

**Context:** Currently has test, test-integration, build. Needs check, lint, format, install, install-ci. Absorbs `lint-rust-wasm` from root. WASM defaults to `wasm` shell (exception). `test-integration` needs `.#js` shell. Commands inlined from `package.json` where possible — JS tools invoked via `yarn` for correct `node_modules` resolution. Exception: `test-integration` keeps `yarn build:test` because the RUSTFLAGS quoting is too complex to inline through nested `bash -c` invocations.

**Inlined package.json commands:**
- `format` → `yarn prettier -w .`
- `format:check` → `yarn prettier -c .`
- `typecheck` → `yarn tsc --noEmit`
- `lint:clippy` → `cargo clippy --locked --all-features --target wasm32-unknown-unknown --no-deps -- -D warnings`
- `lint:fmt` → `cargo fmt --check`
- `check` → `cargo check --locked --target wasm32-unknown-unknown`
- **Kept as yarn script:** `build:test` (complex RUSTFLAGS quoting)

**Step 1: Write the updated wasm.just**

Replace `bindings/wasm/wasm.just` with:

```just
# WASM requires its own shell (emscripten, wasm-pack, etc.)
devshell := env("NIX_DEVSHELL", "wasm")
set shell := ["nix", "develop", ".#" + devshell, "--command", "bash", "-c"]

# Install JS dependencies
install:
  yarn

# Install JS dependencies (CI - frozen lockfile)
install-ci:
  yarn install --immutable

# Verify WASM bindings compile
check:
  cargo check --locked --target wasm32-unknown-unknown --manifest-path Cargo.toml

# Check WASM Rust + TypeScript formatting
lint: install
  cargo clippy --locked --target wasm32-unknown-unknown \
    --manifest-path Cargo.toml --all-features --no-deps -- -Dwarnings && \
  cargo fmt --manifest-path Cargo.toml --check && \
  yarn prettier -c .

# Format TypeScript files
format: install
  yarn prettier -w .

# Run WASM unit tests (v3 + d14n)
test:
  RUST_LOG=off cargo test --locked --release --target wasm32-unknown-unknown \
    -p xmtp_mls -p xmtp_cryptography -p xmtp_common -p xmtp_api -p xmtp_id -p xmtp_db -p xmtp_api_d14n && \
  RUST_LOG=off cargo test --locked --release --target wasm32-unknown-unknown \
    --features d14n --no-fail-fast \
    -p xmtp_mls -p xmtp_cryptography -p xmtp_common -p xmtp_api -p xmtp_id -p xmtp_db -p xmtp_api_d14n

# Run WASM integration tests (vitest in browser — needs .#js shell for Playwright)
# Keeps yarn build:test delegation (RUSTFLAGS quoting too complex to inline through nested bash -c)
test-integration:
  nix develop .#js --command bash -c 'cd {{justfile_directory()}} && yarn && yarn tsc --noEmit && yarn build:test && yarn vitest run'

# Build WASM bindings (via Nix)
build:
  nix build .#wasm-bindings
```

**Step 2: Verify wasm recipes list**

Run: `just wasm --list`
Expected: install, install-ci, check, lint, format, test, test-integration, build listed.

**Step 3: Test wasm check**

Run: `just wasm check`
Expected: Cargo check for wasm32-unknown-unknown passes.

**Step 4: Commit**

```bash
git add bindings/wasm/wasm.just
git commit -m "feat: add check/lint/format recipes to wasm justfile

Absorb lint-rust-wasm from root. Default to wasm devshell.
Inline package.json commands, invoke JS tools via yarn.
Add install/install-ci recipes for dependency management.
Use explicit nix develop .#js for test-integration (needs Playwright)."
```

---

### Task 6: CI Workflow — lint-workspace.yml

**Files:**
- Modify: `.github/workflows/lint-workspace.yml`

**Context:** Currently uses `setup-rust` and direct cargo commands. Switch to `setup-nix` + `just lint-rust`.

**Step 1: Write the updated lint-workspace.yml**

Replace `.github/workflows/lint-workspace.yml` with:

```yaml
name: Lint Workspace
on:
  workflow_call:
env:
  NIX_DEVSHELL: rust
jobs:
  lint:
    name: Lint (Rust Workspace)
    runs-on: warp-ubuntu-latest-x64-16x
    steps:
      - name: Checkout
        uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
          cachix-auth-token: ${{ secrets.CACHIX_AUTH_TOKEN }}
          sccache: "true"
      - uses: taiki-e/install-action@just
      - name: Lint Rust workspace
        run: just lint-rust
```

**Step 2: Commit**

```bash
git add .github/workflows/lint-workspace.yml
git commit -m "ci: switch lint-workspace to use just lint-rust

Replace direct cargo clippy/fmt/hakari with just recipe.
Use setup-nix instead of setup-rust for Nix-managed toolchain."
```

---

### Task 7: CI Workflow — lint-wasm.yml

**Files:**
- Modify: `.github/workflows/lint-wasm.yml`

**Step 1: Write the updated lint-wasm.yml**

Replace `.github/workflows/lint-wasm.yml` with:

```yaml
name: Lint WASM
on:
  workflow_call:
env:
  NIX_DEVSHELL: wasm
jobs:
  lint:
    name: Lint (WASM)
    runs-on: warp-ubuntu-latest-x64-16x
    steps:
      - name: Checkout
        uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
          cachix-auth-token: ${{ secrets.CACHIX_AUTH_TOKEN }}
          sccache: "true"
      - uses: taiki-e/install-action@just
      - name: Lint WASM bindings
        run: just wasm lint
```

**Step 2: Commit**

```bash
git add .github/workflows/lint-wasm.yml
git commit -m "ci: switch lint-wasm to use just wasm lint

Replace direct cargo clippy + emscripten setup + yarn with just recipe.
Nix shell provides emscripten automatically."
```

---

### Task 8: CI Workflow — lint-node.yml

**Files:**
- Modify: `.github/workflows/lint-node.yml`

**Step 1: Write the updated lint-node.yml**

Replace `.github/workflows/lint-node.yml` with:

```yaml
name: Lint Node
on:
  workflow_call:
env:
  NIX_DEVSHELL: js
jobs:
  lint:
    name: Lint (Node)
    runs-on: warp-ubuntu-latest-x64-16x
    steps:
      - name: Checkout
        uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
      - uses: taiki-e/install-action@just
      - name: Lint Node bindings
        run: just node lint
```

**Step 2: Commit**

```bash
git add .github/workflows/lint-node.yml
git commit -m "ci: switch lint-node to use just node lint

Replace setup-node + yarn + yarn format:check with just recipe."
```

---

### Task 9: CI Workflow — lint-ios.yml

**Files:**
- Modify: `.github/workflows/lint-ios.yml`

**Step 1: Write the updated lint-ios.yml**

Replace `.github/workflows/lint-ios.yml` with:

```yaml
name: Lint iOS
on:
  workflow_call:
env:
  NIX_DEVSHELL: ios
jobs:
  lint:
    name: Lint (iOS)
    runs-on: macos-15
    steps:
      - uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
      - uses: taiki-e/install-action@just
      - name: Lint iOS SDK
        run: just ios lint
```

**Step 2: Commit**

```bash
git add .github/workflows/lint-ios.yml
git commit -m "ci: switch lint-ios to use just ios lint

Replace direct dev/lint + dev/fmt calls with just recipe."
```

---

### Task 10: CI Workflow — lint-android.yml

**Files:**
- Modify: `.github/workflows/lint-android.yml`

**Step 1: Write the updated lint-android.yml**

Replace `.github/workflows/lint-android.yml` with:

```yaml
name: Lint Android
on:
  workflow_call:
env:
  NIX_DEVSHELL: android
jobs:
  lint:
    name: Lint (Android)
    runs-on: warp-ubuntu-latest-x64-16x
    steps:
      - uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
      - uses: taiki-e/install-action@just
      - name: Build Android bindings (required for lint)
        run: just android build
      - name: Lint Android SDK
        run: just android lint
```

Note: Android lint needs compiled bindings for Spotless/lintDebug to resolve generated code. The `just android build` step is required before linting.

**Step 2: Commit**

```bash
git add .github/workflows/lint-android.yml
git commit -m "ci: switch lint-android to use just android lint

Replace direct gradlew + dev/bindings with just recipes.
Build bindings before lint (required for Spotless/lintDebug)."
```

---

### Task 11: CI Workflow — Create lint-config.yml, Delete lint-toml.yml and lint-nix.yml

**Files:**
- Create: `.github/workflows/lint-config.yml`
- Delete: `.github/workflows/lint-toml.yml`
- Delete: `.github/workflows/lint-nix.yml`
- Modify: `.github/workflows/lint.yml`

**Step 1: Create lint-config.yml**

```yaml
name: Lint Config
on:
  workflow_call:
env:
  NIX_DEVSHELL: rust
jobs:
  lint:
    name: Lint (Config)
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
      - uses: taiki-e/install-action@just
      - name: Lint config files (TOML, Nix, shell)
        run: just lint-config
```

**Step 2: Delete lint-toml.yml and lint-nix.yml**

```bash
rm .github/workflows/lint-toml.yml .github/workflows/lint-nix.yml
```

**Step 3: Update lint.yml orchestrator**

Read `lint.yml` and update:
- Remove `toml` and `nix` from path filter outputs
- Add `config` filter that matches `**/*.toml`, `**/*.nix`, `flake.nix`, `dev/**`
- Replace `lint-toml` and `lint-nix` job calls with single `lint-config` call
- Update the final `lint` gate job's needs list

**Step 4: Commit**

```bash
git add .github/workflows/lint-config.yml .github/workflows/lint.yml
git rm .github/workflows/lint-toml.yml .github/workflows/lint-nix.yml
git commit -m "ci: merge lint-toml + lint-nix into lint-config

New lint-config workflow runs just lint-config (TOML + Nix + shellcheck).
Delete separate lint-toml.yml and lint-nix.yml workflows.
Update lint.yml orchestrator to call lint-config."
```

---

### Task 12: CI Workflow — test-workspace.yml

**Files:**
- Modify: `.github/workflows/test-workspace.yml`

**Context:** Switch from direct `cargo llvm-cov nextest` to `just test` with `CARGO_TEST_CMD` override.

**Step 1: Write the updated test-workspace.yml**

Replace `.github/workflows/test-workspace.yml` with:

```yaml
name: Test Workspace
on:
  workflow_call:
env:
  NIX_DEVSHELL: rust
  CARGO_TEST_CMD: "cargo llvm-cov nextest --no-fail-fast --no-report"
jobs:
  test:
    name: Test (Rust Workspace)
    runs-on: warp-ubuntu-latest-x64-16x
    steps:
      - name: Checkout
        uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
          cachix-auth-token: ${{ secrets.CACHIX_AUTH_TOKEN }}
          sccache: "true"
      - uses: taiki-e/install-action@just
      - name: Start backend
        run: just backend up
      - name: Dump docker logs on failure
        if: failure()
        uses: jwalton/gh-docker-logs@v2
      - name: Run tests (v3 + d14n with coverage)
        run: just test
      - name: Generate coverage report
        run: nix develop .#rust --command cargo llvm-cov report --output-path lcov.info --lcov
      - name: Upload coverage
        uses: codecov/codecov-action@v5
        with:
          files: lcov.info
          token: ${{ secrets.CODECOV_TOKEN }}
```

**Step 2: Commit**

```bash
git add .github/workflows/test-workspace.yml
git commit -m "ci: switch test-workspace to use just test

Use CARGO_TEST_CMD override for llvm-cov coverage.
Replace direct cargo commands with just recipes.
Use just backend up for Docker services."
```

---

### Task 13: CI Workflow — test-wasm.yml

**Files:**
- Modify: `.github/workflows/test-wasm.yml`

**Step 1: Write the updated test-wasm.yml**

Replace `.github/workflows/test-wasm.yml` with:

```yaml
name: Test WASM
on:
  workflow_call:
env:
  NIX_DEVSHELL: wasm
jobs:
  wasm-ci:
    name: Test (WASM)
    runs-on: warp-ubuntu-latest-x64-16x
    steps:
      - name: Checkout
        uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
          cachix-auth-token: ${{ secrets.CACHIX_AUTH_TOKEN }}
          sccache: "true"
      - uses: taiki-e/install-action@just
      - name: Start backend
        run: just backend up
      - name: Run WASM tests
        run: just wasm test

  wasm-integration:
    name: WASM Integration Tests
    runs-on: warp-ubuntu-latest-x64-16x
    steps:
      - name: Checkout
        uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
          cachix-auth-token: ${{ secrets.CACHIX_AUTH_TOKEN }}
          sccache: "true"
      - uses: taiki-e/install-action@just
      - name: Start backend
        run: just backend up
      - name: Run WASM integration tests
        run: just wasm test-integration
```

**Step 2: Commit**

```bash
git add .github/workflows/test-wasm.yml
git commit -m "ci: switch test-wasm to use just wasm test

Replace dev/test/wasm-ci with inlined logic in wasm.just.
Use just backend up for Docker services."
```

---

### Task 14: CI Workflow — test-node.yml

**Files:**
- Modify: `.github/workflows/test-node.yml`

**Step 1: Write the updated test-node.yml**

Replace `.github/workflows/test-node.yml` with:

```yaml
name: Test Node
on:
  workflow_call:
env:
  NIX_DEVSHELL: js
jobs:
  test:
    name: Test (Node)
    runs-on: warp-ubuntu-latest-x64-16x
    steps:
      - name: Checkout
        uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
      - uses: taiki-e/install-action@just
      - name: Start backend
        run: just backend up
      - name: Run Node tests
        run: just node test
```

**Step 2: Commit**

```bash
git add .github/workflows/test-node.yml
git commit -m "ci: switch test-node to use just node test

Replace setup-rust + setup-node + yarn with just recipe.
Use just backend up for Docker services."
```

---

### Task 15: CI Workflow — test-ios.yml

**Files:**
- Modify: `.github/workflows/test-ios.yml`

**Context:** Keep Fly.io deploy/cleanup jobs. Only change the test job steps.

**Step 1: Read current test-ios.yml**

Read `.github/workflows/test-ios.yml` to preserve deploy-backend and cleanup jobs.

**Step 2: Update only the tests job**

In the `tests` job, after checkout and setup-nix and setup-xcode, replace build/test steps with:

```yaml
      - uses: taiki-e/install-action@just
      - name: Build and test
        env:
          NIX_DEVSHELL: ios
          XMTP_NODE_ADDRESS: ${{ needs.deploy-backend.outputs.node_url }}
          XMTP_HISTORY_SERVER_ADDRESS: ${{ needs.deploy-backend.outputs.history_url }}
        run: just ios test
```

**Step 3: Commit**

```bash
git add .github/workflows/test-ios.yml
git commit -m "ci: switch test-ios to use just ios test

Replace direct nix develop + dev/build + dev/test with just recipe.
Preserve Fly.io deploy/cleanup infrastructure jobs."
```

---

### Task 16: CI Workflow — test-android.yml

**Files:**
- Modify: `.github/workflows/test-android.yml`

**Step 1: Read current test-android.yml**

Read `.github/workflows/test-android.yml` to understand the two-job structure.

**Step 2: Update unit-tests job**

Replace build/test steps with:

```yaml
      - uses: taiki-e/install-action@just
      - name: Start backend
        env:
          NIX_DEVSHELL: android
        run: just backend up
      - name: Run unit tests
        env:
          NIX_DEVSHELL: android
        run: just android test
```

**Step 3: Update integration-tests job**

Replace build/test steps with (preserving KVM setup):

```yaml
      - uses: taiki-e/install-action@just
      - name: Start backend
        env:
          NIX_DEVSHELL: android
        run: just backend up
      - name: Enable KVM
        run: |
          echo 'KERNEL=="kvm", GROUP="kvm", MODE="0666", OPTIONS+="static_node=kvm"' | sudo tee /etc/udev/rules.d/99-kvm4all.rules
          sudo udevadm control --reload-rules
          sudo udevadm trigger --name-match=kvm
      - name: Run integration tests
        timeout-minutes: 20
        env:
          NIX_DEVSHELL: android
          NIX_ANDROID_EMULATOR_FLAGS: "-no-snapshot-save -no-window -gpu swiftshader_indirect -noaudio -memory 4096 -partition-size 8192"
        run: just android test-integration
```

**Step 4: Commit**

```bash
git add .github/workflows/test-android.yml
git commit -m "ci: switch test-android to use just android test

Replace direct nix develop + gradlew with just recipes.
Preserve KVM setup for emulator integration tests."
```

---

### Task 17: CI Workflow — test-bindings-check.yml

**Files:**
- Modify: `.github/workflows/test-bindings-check.yml`

**Step 1: Write the updated test-bindings-check.yml**

Replace `.github/workflows/test-bindings-check.yml` with:

```yaml
name: Test Bindings Check
on:
  workflow_call:
jobs:
  check-swift:
    runs-on: warp-macos-13-arm64-6x
    env:
      NIX_DEVSHELL: ios
    steps:
      - name: Checkout
        uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
          cachix-auth-token: ${{ secrets.CACHIX_AUTH_TOKEN }}
          sccache: "true"
      - uses: taiki-e/install-action@just
      - name: Check iOS bindings compile
        run: just ios check

  check-android:
    runs-on: warp-ubuntu-latest-x64-16x
    env:
      NIX_DEVSHELL: android
    steps:
      - name: Checkout
        uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
          cachix-auth-token: ${{ secrets.CACHIX_AUTH_TOKEN }}
          sccache: "true"
      - uses: taiki-e/install-action@just
      - name: Check Android bindings compile
        run: just android check
```

**Step 2: Commit**

```bash
git add .github/workflows/test-bindings-check.yml
git commit -m "ci: switch test-bindings-check to use just ios/android check

Replace direct cargo check/ndk with just recipes."
```

---

### Task 18: Dev Script Cleanup

**Files:**
- Delete: `dev/fmt`, `dev/lint-shellcheck`, `dev/lint-markdown`
- Delete: `dev/check-wasm`, `dev/check-android`, `dev/check-swift`
- Delete: `dev/test/v3`, `dev/test/d14n`, `dev/test/wasm`, `dev/test/wasm-ci`, `dev/test/wasm-nextest`
- Delete: `sdks/ios/dev/test`, `sdks/ios/dev/fmt`, `sdks/ios/dev/lint`, `sdks/ios/dev/build`
- Check: `dev/lint-rust` (delete if exists and is replaced)

**Step 1: Verify no remaining references to deleted scripts**

Search the codebase for references to each script being deleted. Check:
- `.github/workflows/*.yml` — should all use `just` now
- Other `dev/` scripts — some may source/call deleted ones
- `CLAUDE.md` and `sdks/*/CLAUDE.md` — update documentation references
- `README.md` — update if it references dev scripts

**Step 2: Delete the scripts**

```bash
rm -f dev/fmt dev/lint-shellcheck dev/lint-markdown
rm -f dev/check-wasm dev/check-android dev/check-swift
rm -f dev/test/v3 dev/test/d14n dev/test/wasm dev/test/wasm-ci dev/test/wasm-nextest
rm -f sdks/ios/dev/test sdks/ios/dev/fmt sdks/ios/dev/lint sdks/ios/dev/build
# Check if dev/lint-rust exists and remove if replaced
[ -f dev/lint-rust ] && rm dev/lint-rust
```

**Step 3: Update CLAUDE.md files**

Update the root `CLAUDE.md` "Development Commands" section to reference `just` commands instead of `dev/` scripts. Update `sdks/android/CLAUDE.md`, `sdks/ios/CLAUDE.md`, `bindings/node/CLAUDE.md`.

**Step 4: Commit**

```bash
git add -A dev/ sdks/ios/dev/ CLAUDE.md sdks/android/CLAUDE.md sdks/ios/CLAUDE.md bindings/node/CLAUDE.md
git commit -m "chore: remove dev scripts replaced by justfile recipes

Delete: dev/fmt, dev/lint-*, dev/check-*, dev/test/v3,
dev/test/d14n, dev/test/wasm*, sdks/ios/dev/{test,fmt,lint,build}.
Update CLAUDE.md documentation to reference just commands."
```

---

### Task 19: Update Design Doc

**Files:**
- Modify: `docs/plans/2026-02-23-justfile-consolidation-design.md`

**Step 1: Update the design doc**

Fix `NIX_DEVSHELL` default from `local` to `default` (the actual Nix shell name).

**Step 2: Commit**

```bash
git add docs/plans/2026-02-23-justfile-consolidation-design.md
git commit -m "docs: fix NIX_DEVSHELL default to 'default' in design doc"
```

---

### Task 20: End-to-End Verification (Native macOS)

**No files changed — verification only.**

Run every `just` command natively and verify it works:

**Step 1: Root workspace commands**

```bash
just --list --list-submodules
just check crate xmtp_common
just lint-rust
just lint-config
just lint-markdown
just format
```

**Step 2: Module lint/format commands**

```bash
just android --list
just ios lint
just ios format
just node lint
just node format
just wasm check
just wasm lint
just wasm format
```

**Step 3: Test commands (requires backend)**

```bash
just backend up
just test crate xmtp_common
just backend down
```

---

### Task 21: CI-Parity Verification in Linux Container

**No files changed — verification only. All infrastructure is temporary and removed when done.**

**Context:** CI runs on Ubuntu Linux (x86-64 via Warp runners). Verify that check, lint, and format commands work inside a Linux container with Nix, matching the CI environment. Skip test commands entirely (they need Docker/backend infrastructure that doesn't belong in a verification container).

**Step 1: Create a temporary verification script**

Create a file `/tmp/ci-verify.sh` (NOT in the repo):

```bash
#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="$(pwd)"

GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

echo "=== CI-Parity Verification in Linux Container ==="
echo ""

IMAGE_NAME="ci-verify-libxmtp:tmp"

echo "Building temporary verification image..."
docker build -t "$IMAGE_NAME" -f - /dev/null <<'DOCKERFILE'
FROM nixos/nix:latest
RUN mkdir -p /etc/nix && \
    echo "experimental-features = nix-command flakes" >> /etc/nix/nix.conf && \
    echo "accept-flake-config = true" >> /etc/nix/nix.conf
RUN nix-env -iA nixpkgs.just
WORKDIR /workspace
DOCKERFILE

echo "Image built successfully."
echo ""

run_in_container() {
  local description="$1"
  local env_vars="$2"
  local command="$3"

  printf "  %-50s " "$description"

  local env_args=""
  if [ -n "$env_vars" ]; then
    for var in $env_vars; do
      env_args="$env_args -e $var"
    done
  fi

  output=$(docker run --rm \
    -v "${REPO_DIR}:/workspace:ro" \
    -w /workspace \
    $env_args \
    "$IMAGE_NAME" \
    bash -c "$command" 2>&1) && rc=0 || rc=$?

  if [ $rc -eq 0 ]; then
    echo -e "${GREEN}PASS${NC}"
  else
    echo -e "${RED}FAIL${NC}"
    echo "    Last 5 lines of output:"
    echo "$output" | tail -5 | sed 's/^/    /'
  fi
}

echo "--- Smoke test ---"
echo ""
run_in_container "just --list" "" "just --list --list-submodules"

echo ""
echo "--- Check commands ---"
echo ""
run_in_container "just check (workspace)"          "NIX_DEVSHELL=rust"    "just check"
run_in_container "just check crate xmtp_common"    "NIX_DEVSHELL=rust"    "just check crate xmtp_common"
run_in_container "just wasm check"                 "NIX_DEVSHELL=wasm"    "just wasm check"
run_in_container "just android check"              "NIX_DEVSHELL=android" "just android check"

echo ""
echo "--- Lint commands ---"
echo ""
run_in_container "just lint-rust"                  "NIX_DEVSHELL=rust"    "just lint-rust"
run_in_container "just lint-config"                "NIX_DEVSHELL=rust"    "just lint-config"
run_in_container "just lint-markdown"              "NIX_DEVSHELL=rust"    "just lint-markdown"
run_in_container "just wasm lint"                  "NIX_DEVSHELL=wasm"    "just wasm lint"
run_in_container "just node lint"                  "NIX_DEVSHELL=js"      "just node lint"
run_in_container "just android lint"               "NIX_DEVSHELL=android" "just android lint"

echo ""
echo "--- Format commands (dry-run / check mode) ---"
echo ""
run_in_container "just lint-shell (format check)"  "NIX_DEVSHELL=rust"    "just lint-shell"
run_in_container "just node lint (prettier check)" "NIX_DEVSHELL=js"      "just node lint"
run_in_container "just wasm lint (prettier check)" "NIX_DEVSHELL=wasm"    "just wasm lint"

echo ""
echo "=== Cleanup ==="
docker rmi "$IMAGE_NAME" 2>/dev/null && echo "Removed temporary image." || true
echo ""
echo "=== Verification complete ==="
```

**Step 2: Run the verification script**

```bash
chmod +x /tmp/ci-verify.sh
/tmp/ci-verify.sh
```

**Expected results:** All commands should **PASS**.

**Step 3: Investigate any failures**

If any command fails, investigate:
- Is a tool missing from the Nix shell?
- Is there a path issue with module working directories?
- Does `set shell` concatenation work in the container?

Fix issues in the justfiles and re-run until all commands pass.

**Step 4: Clean up**

```bash
rm /tmp/ci-verify.sh
docker rmi ci-verify-libxmtp:tmp 2>/dev/null || true
```

---

### Task 22: Final Review

**Step 1: Verify all changes are committed**

```bash
git status
git log --oneline main..HEAD
```

All changes should be committed on branch `02-23-add_more_checks_to_justfile`.
