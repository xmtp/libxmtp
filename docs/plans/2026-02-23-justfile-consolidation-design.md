# Justfile & Dev Script Consolidation Design

## Context

Development commands are scattered across 3 layers: justfiles (partial), `dev/` shell scripts, and `package.json` scripts. CI workflows call different layers inconsistently — only `test-wasm.yml` uses justfiles, while everything else calls dev scripts or raw cargo/yarn commands directly. Many dev scripts don't use Nix. This makes it impossible to guarantee that running a command locally produces the same result as CI.

**Goal:** Every check/lint/format/test command runs through justfiles, every justfile recipe runs through Nix, and every CI workflow calls `just`. Dev scripts that are replaced get deleted.

## Design Decisions

- `check` for iOS/Android = full SDK build (bindings + native build)
- `lint` = umbrella with sub-recipes: lint-rust, lint-config (toml+nix+shell), lint-markdown
- `format` = root formats everything (treefmt + prettier + swiftformat + spotless)
- Keep infra + specialized dev scripts, remove replaced ones
- Backend management via justfile recipe
- **Nix pattern: Explicit `nix develop` per recipe** — both root and module justfiles use `nix develop .#{{devshell}} --command` per recipe. `set shell` cannot use `env()` (const context restriction in just), so all recipes wrap commands explicitly.
- **Configurable devShell:** `NIX_DEVSHELL` env var defaults to `default` (full dev environment). CI overrides with leaner shells (`rust`, `android`, `ios`, `js`). WASM defaults to `wasm` (exception — needs emscripten/wasm-specific env vars).
- **CI coverage:** env-var override pattern — `CARGO_TEST_CMD` defaults to `cargo nextest run`, CI overrides to `cargo llvm-cov nextest --no-fail-fast --no-report`
- **WASM test-integration shell mismatch:** use explicit `nix develop .#js` for that one recipe (needs Playwright from js shell)
- **WASM lint/check in wasm module only:** root justfile doesn't deal with WASM at all

## Recipe Matrix

| Command | Workspace (root) | Android | iOS | Node | WASM |
|---------|------------------|---------|-----|------|------|
| **check** | `cargo check --workspace` | bindings + gradlew build | bindings + swift build | yarn build | cargo check --target wasm32 |
| **lint** | lint-rust + lint-config + lint-markdown | spotlessCheck + lintDebug | swiftlint + swiftformat --lint | prettier check | clippy wasm32 + fmt check + prettier check |
| **format** | treefmt + all module formatters | spotlessApply | swiftformat | prettier | prettier |
| **test** | nextest v3 + d14n | gradlew testDebug | swift test | yarn test | cargo test wasm32 (v3+d14n) |
| **test v3** | nextest v3 only | — | — | — | — |
| **test d14n** | nextest d14n only | — | — | — | — |
| **test-integration** | — | connectedCheck | — | — | vitest browser |

**Env var overrides:**
- `NIX_DEVSHELL` — defaults to `default`, CI sets `rust`/`android`/`ios`/`js`/`wasm`
- `CARGO_TEST_CMD` — defaults to `cargo nextest run`, CI sets `cargo llvm-cov nextest --no-fail-fast --no-report`

## Justfile Specifications

### Root `justfile`

```just
mod android 'sdks/android/android.just'
mod ios 'sdks/ios/ios.just'
mod node 'bindings/node/node.just'
mod wasm 'bindings/wasm/wasm.just'

devshell := env("NIX_DEVSHELL", "default")
cargo_test := env("CARGO_TEST_CMD", "cargo nextest run")

default:
  just --list --list-submodules

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

### `sdks/android/android.just`

Uses explicit `nix develop` per recipe (not `set shell` — `env()` can't be used in const context).

```just
devshell := env("NIX_DEVSHELL", "default")

check:
  nix develop .#{{devshell}} --command bash -c './dev/bindings && ./gradlew -p . build'
lint:
  nix develop .#{{devshell}} --command bash -c './gradlew -p . spotlessCheck --continue && ./gradlew -p . :library:lintDebug'
format:
  nix develop .#{{devshell}} --command bash -c './gradlew -p . spotlessApply'
test:
  nix develop .#{{devshell}} --command bash -c './dev/bindings && ./gradlew -p . library:testDebug'
test-integration:
  nix develop .#{{devshell}} --command bash -c 'run-test-emulator && ./gradlew -p . connectedCheck --continue'
build:
  nix develop .#{{devshell}} --command bash -c './dev/bindings'
```

### `sdks/ios/ios.just`

```just
devshell := env("NIX_DEVSHELL", "default")

check:
  nix develop .#{{devshell}} --command bash -c './dev/bindings && swift build'
lint:
  nix develop .#{{devshell}} --command bash -c 'swiftlint && swiftformat --lint .'
format:
  nix develop .#{{devshell}} --command bash -c 'swiftformat .'
test:
  nix develop .#{{devshell}} --command bash -c './dev/bindings && swift test --parallel'
build:
  nix develop .#{{devshell}} --command bash -c './dev/bindings'
```

### `bindings/node/node.just`

Commands inlined from `package.json`. JS tools invoked via `yarn` for correct `node_modules` resolution.

```just
devshell := env("NIX_DEVSHELL", "default")

install:
  nix develop .#{{devshell}} --command bash -c 'yarn'
install-ci:
  nix develop .#{{devshell}} --command bash -c 'yarn install --immutable'
check: install
  nix develop .#{{devshell}} --command bash -c 'rm -rf dist && yarn napi build --platform --release --esm && mkdir -p dist && mv index.js dist && mv index.d.ts dist && mv *.node dist'
lint: install
  nix develop .#{{devshell}} --command bash -c 'yarn prettier -c .'
format: install
  nix develop .#{{devshell}} --command bash -c 'yarn prettier -w .'
test: install
  nix develop .#{{devshell}} --command bash -c 'rm -rf dist && yarn napi build --platform --esm --features test-utils && mkdir -p dist && mv index.js dist && mv index.d.ts dist && mv *.node dist && yarn vitest run'
build: check
```

### `bindings/wasm/wasm.just`

Commands inlined from `package.json` where possible. Exception: `test-integration` keeps `yarn build:test` (RUSTFLAGS quoting too complex for nested `bash -c`).

```just
devshell := env("NIX_DEVSHELL", "wasm")

install:
  nix develop .#{{devshell}} --command bash -c 'yarn'
install-ci:
  nix develop .#{{devshell}} --command bash -c 'yarn install --immutable'
check:
  nix develop .#{{devshell}} --command cargo check --locked --target wasm32-unknown-unknown --manifest-path Cargo.toml
lint: install
  nix develop .#{{devshell}} --command bash -c 'cargo clippy --locked --target wasm32-unknown-unknown --manifest-path Cargo.toml --all-features --no-deps -- -Dwarnings && cargo fmt --manifest-path Cargo.toml --check && yarn prettier -c .'
format: install
  nix develop .#{{devshell}} --command bash -c 'yarn prettier -w .'
test:
  nix develop .#{{devshell}} --command bash -c 'RUST_LOG=off cargo test --locked --release --target wasm32-unknown-unknown -p xmtp_mls -p xmtp_cryptography -p xmtp_common -p xmtp_api -p xmtp_id -p xmtp_db -p xmtp_api_d14n && RUST_LOG=off cargo test --locked --release --target wasm32-unknown-unknown --features d14n --no-fail-fast -p xmtp_mls -p xmtp_cryptography -p xmtp_common -p xmtp_api -p xmtp_id -p xmtp_db -p xmtp_api_d14n'
test-integration:
  nix develop .#js --command bash -c 'cd {{justfile_directory()}} && yarn && yarn tsc --noEmit && yarn build:test && yarn vitest run'
build:
  nix build .#wasm-bindings
```

## CI Workflow Changes

Every reusable workflow simplified to: setup nix + install just + `just <command>`. CI sets `NIX_DEVSHELL` to the appropriate lean shell.

Key changes:
- All `lint-*.yml` and `test-*.yml` switch to `just` commands
- New `lint-config.yml` replaces `lint-toml.yml` + `lint-nix.yml` (runs `just lint-config`)
- `test-workspace.yml` sets `CARGO_TEST_CMD` for coverage, adds report step after `just test`
- `lint.yml` orchestrator updated to reference `lint-config.yml`

## Dev Scripts to Remove

```
dev/fmt, dev/lint-shellcheck, dev/lint-markdown, dev/lint-rust
dev/check-wasm, dev/check-android, dev/check-swift
dev/test/v3, dev/test/d14n, dev/test/wasm, dev/test/wasm-ci, dev/test/wasm-nextest
sdks/ios/dev/test, sdks/ios/dev/fmt, sdks/ios/dev/lint, sdks/ios/dev/build
```

## Dev Scripts to Keep

```
dev/up, dev/down, dev/docker/*, dev/nix-*, dev/direnv-*
dev/gen_protos.sh, dev/bench, dev/flamegraph, dev/llvm-cov, dev/docs
dev/release-swift, dev/xdbg, dev/build_validation_service*
dev/test/coverage, dev/test/libs, dev/test/wasm-interactive
dev/test/big_group*.sh, dev/test/diff-coverage, dev/test/browser-sdk
sdks/android/dev/.setup, sdks/android/dev/bindings, sdks/android/dev/up
sdks/ios/dev/.setup, sdks/ios/dev/bindings, sdks/ios/dev/fly/*
```

## Verification Notes

1. **`set shell` with `env()` does NOT work** — `env()` is a function call, not a const expression. `set shell` requires const context in just 1.46.0. Resolved by using explicit `nix develop .#{{devshell}} --command` per recipe instead.
2. WASM `test-integration` uses explicit `nix develop .#js` override — works correctly.
3. CI Nix caching: all workflows switched from `setup-rust` to `setup-nix`.
4. Foundry/cargo-hakari availability in `.#rust` — provided by Nix shell.
5. Module working directories work correctly with explicit `nix develop` per recipe.
6. Android lint requires compiled bindings — CI workflow runs `just android build` before `just android lint`.
