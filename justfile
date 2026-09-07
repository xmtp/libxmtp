mod android 'sdks/android/android.just'
mod ios 'sdks/ios/ios.just'
mod node 'bindings/node/node.just'
mod wasm 'bindings/wasm/wasm.just'
mod js 'sdks/js/js.just'

export NIX_DEVSHELL := env("NIX_DEVSHELL", "default")

set shell := ["./dev/nix-shell"]

nix_system := arch() + "-" + if os() == "macos" { "darwin" } else { "linux" }

# CI overrides to "cargo llvm-cov nextest --no-fail-fast --no-report" for coverage

cargo_test := env("CARGO_TEST_CMD", "cargo nextest run")

[script("bash")]
default:
    just --list --list-submodules

# --- CHECK ---

# `just check`, `just check crate xmtp_mls`, `just check crate xmtp_mls xmtp_db`
[script("bash")]
check target="workspace" *args="":
    just _check-{{ target }} {{ args }}

[private]
_check-workspace:
    cargo check --locked

[private]
_check-crate +crates:
    args=""; for c in {{ crates }}; do args="$args -p $c"; done; \
    cargo check --locked $args

# --- LINT ---

lint: lint-rust lint-config lint-markdown lint-proto

lint-proto:
    buf lint proto

lint-rust:
    cargo clippy --locked --all-features --all-targets --no-deps -- -Dwarnings
    cargo fmt --check
    cargo hakari generate --diff
    cargo hakari manage-deps --dry-run

# Config linting: TOML, Nix, shell scripts
lint-config: lint-treefmt

lint-toml:
    taplo format --check --diff
    taplo check

[script("bash")]
lint-treefmt:
    nix fmt -- --fail-on-change

# Exclude the generated error glossary and release changelogs.
lint-markdown:
    markdownlint "**/*.md" --ignore "**/CLAUDE.md" --ignore "**/node_modules/**" --ignore "target/**" --ignore "docs/error_glossary.md" --ignore "sdks/js/*/CHANGELOG.md" --disable MD001 MD013

# --- FORMAT ---

[script("bash")]
format:
    nix fmt
    just android format
    just ios format
    just node format
    just wasm format

# --- TEST ---

# run the nix derivation for v3/d14n tests. no local incremental compilation but does use global cachix.
nix-test:
    nix run nixpkgs#nix-output-monitor build .#nextest.{{ nix_system }}.v3
    nix run nixpkgs#nix-output-monitor build .#nextest.{{ nix_system }}.d14n

# `just test`, `just test v3`, `just test d14n`, `just test crate xmtp_mls`
[script("bash")]
test target="all" *args="":
    just _test-{{ target }} {{ args }}

[private]
_test-all *args="": (_test-v3 args) (_test-d14n args)

[private]
_test-v3 *args="":
    {{ cargo_test }} --profile ci {{ args }}

[private]
_test-d14n *args="":
    {{ cargo_test }} \
      --features d14n --profile ci-d14n \
      -E 'package(xmtp_mls)' -E 'rdeps(xmtp_mls)' {{ args }}

[private]
_test-crate +crates:
    args=""; for c in {{ crates }}; do args="$args -p $c"; done; \
    {{ cargo_test }} $args

# Verify the shared validation crate without workspace feature unification.
check-validation:
    dev/check-validation check

# Run the shared validation crate tests on native and wasm Node.
test-validation:
    dev/check-validation test

validation: check-validation test-validation

# --- BACKEND ---

# Build the self-hosted binary without starting the test services.
build-backend:
    nix build .#xmtp-backend

test-backend *args="":
    DATABASE_URL="${DATABASE_URL:-postgres://xmtp:xmtp@localhost:55432/xmtp_backend}" RUST_TEST_THREADS="${RUST_TEST_THREADS:-4}" cargo test --locked -p xmtp_backend {{ args }}

backend-db-up:
    docker compose -f dev/backend/compose.yml up --detach --wait

backend-db-down:
    docker compose -f dev/backend/compose.yml down

[script("bash")]
backend-schema:
    set -euo pipefail
    schema_tmp="$(mktemp docs/schemas/backend-v1.XXXXXX)"
    trap 'rm -f "$schema_tmp"' EXIT
    cargo run --locked --quiet -p xmtp_backend --example config_schema > "$schema_tmp"
    mv "$schema_tmp" docs/schemas/backend-v1.json

backend-sql-prepare:
    DATABASE_URL="${DATABASE_URL:-postgres://xmtp:xmtp@localhost:55432/xmtp_backend}" cargo sqlx migrate run --source apps/backend/migrations
    DATABASE_URL="${DATABASE_URL:-postgres://xmtp:xmtp@localhost:55432/xmtp_backend}" cargo sqlx prepare --workspace -- --package xmtp_backend --all-targets

backend-sql-check:
    DATABASE_URL="${DATABASE_URL:-postgres://xmtp:xmtp@localhost:55432/xmtp_backend}" cargo sqlx migrate run --source apps/backend/migrations
    DATABASE_URL="${DATABASE_URL:-postgres://xmtp:xmtp@localhost:55432/xmtp_backend}" cargo sqlx prepare --check --workspace -- --package xmtp_backend --all-targets

backend-image arch="x86_64":
    nix build .#backend-image-{{ arch }}-unknown-linux-musl

# `just backend up`, `just backend down`
[script("bash")]
backend command="up":
    just _backend-{{ command }}

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
