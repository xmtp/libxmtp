mod android 'sdks/android/android.just'
mod ios 'sdks/ios/ios.just'
mod node 'bindings/node/node.just'
mod wasm 'bindings/wasm/wasm.just'

export NIX_DEVSHELL := env("NIX_DEVSHELL", "default")
set shell := ["./dev/nix-shell"]

# CI overrides to "cargo llvm-cov nextest --no-fail-fast --no-report" for coverage
cargo_test := env("CARGO_TEST_CMD", "cargo nextest run")

[script("bash")]
default:
  just --list --list-submodules

# --- CHECK ---

# `just check`, `just check crate xmtp_mls`, `just check crate xmtp_mls xmtp_db`
[script("bash")]
check target="workspace" *args="":
  just _check-{{target}} {{args}}

[private]
_check-workspace:
  cargo check --locked --workspace --exclude bindings_wasm

[private]
_check-crate +crates:
  args=""; for c in {{crates}}; do args="$args -p $c"; done; \
  cargo check --locked $args

# --- LINT ---

lint: lint-rust lint-config

lint-rust:
  cargo clippy --locked --workspace \
    --all-features --all-targets --no-deps --exclude bindings_wasm -- -Dwarnings
  cargo fmt --check
  cargo hakari generate --diff
  cargo hakari manage-deps --dry-run

# Config linting: TOML, Nix, shell scripts
lint-config: lint-toml lint-nix lint-treefmt

lint-toml:
  taplo format --check --diff
  taplo check

lint-nix:
  nixfmt --check nix/ flake.nix

[script("bash")]
lint-treefmt:
  nix fmt -- --fail-on-change

lint-markdown:
  markdownlint "**/*.md" --disable MD001 MD013

# --- FORMAT ---

[script("bash")]
format:
  set -euo pipefail
  nix fmt
  just android format
  just ios format
  just node format
  just wasm format

# --- TEST ---

# `just test`, `just test v3`, `just test d14n`, `just test crate xmtp_mls`
[script("bash")]
test target="all" *args="":
  just _test-{{target}} {{args}}

[private]
_test-all: (_test-v3) (_test-d14n)

[private]
_test-v3:
  {{cargo_test}} \
    --profile ci --workspace --exclude bindings_wasm

[private]
_test-d14n:
  {{cargo_test}} \
    --features d14n --profile ci-d14n \
    -E 'package(xmtp_mls)' -E 'rdeps(xmtp_mls)' \
    --workspace --exclude bindings_wasm

[private]
_test-crate +crates:
  args=""; for c in {{crates}}; do args="$args -p $c"; done; \
  {{cargo_test}} $args

# --- BACKEND ---

# `just backend up`, `just backend down`
[script("bash")]
backend command="up":
  just _backend-{{command}}

[private]
[script("bash")]
_backend-up:
  set -euo pipefail
  nix build .#validation-service-image
  dev/docker/up

[private]
[script("bash")]
_backend-down:
  dev/docker/down
