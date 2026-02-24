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
