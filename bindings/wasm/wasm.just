export NIX_DEVSHELL := env("NIX_DEVSHELL", "wasm")

set shell := ["../../dev/nix-shell"]

nix_system := arch() + "-" + if os() == "macos" { "darwin" } else { "linux" }

install:
    yarn

# Install JS dependencies
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

# Packages that support the WASM target

wasm_packages := "-p xmtp_mls -p xmtp_cryptography -p xmtp_common -p xmtp_api -p xmtp_id -p xmtp_db -p xmtp_api_d14n -p xmtp_content_types"

# WASM bindings Rust tests (v3 + d14n)
test: test-v3 test-d14n

# WASM bindings Rust tests (v3)
test-v3:
    XMTP_TEST_LOGGING=false RUST_LOG=off cargo nextest run --locked --profile ci --cargo-profile wasm-test --target wasm32-unknown-unknown \
      {{ wasm_packages }}

# WASM bindings Rust tests (d14n)
test-d14n:
    XMTP_TEST_LOGGING=false RUST_LOG=off cargo nextest run --locked --profile ci-d14n --cargo-profile wasm-test --target wasm32-unknown-unknown \
      --features d14n \
      {{ wasm_packages }}

# nix derivation for wasm v3 tests (no local incremental builds, but uses cachix)
nix-test-v3:
    nix run nixpkgs#nix-output-monitor build .#nextest.{{ nix_system }}.wasm-v3

# nix derivation for wasm d14n tests (no local incremental builds, but uses cachix)
nix-test-d14n:
    nix run nixpkgs#nix-output-monitor build .#nextest.{{ nix_system }}.wasm-d14n

# nix derivation for v3 and d14n tests
nix-test: nix-test-v3 nix-test-d14n

# CI version of wasm tests (no local incremental compilation, but uses cachix)
test-ci:
    nix build -L .#nextest.{{ nix_system }}.wasm-v3
    nix build -L .#nextest.{{ nix_system }}.wasm-d14n

# WASM JS integration tests
[script("bash")]
test-integration: _build-test install
    set -euo pipefail
    cd {{ source_directory() }}
    nix develop .#js --command bash -euc 'yarn tsc --noEmit && yarn vitest run'

# Build WASM bindings
build:
    nix build .#wasm-bindings

[private]
_build-test:
    nix build .#wasm-bindings-test
    mkdir -p {{ justfile_directory() }}/bindings/wasm/dist
    cp -r result/dist/* {{ justfile_directory() }}/bindings/wasm/dist
