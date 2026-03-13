export NIX_DEVSHELL := env("NIX_DEVSHELL", "wasm")

set shell := ["../../dev/nix-shell"]

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

wasm_packages := "-p xmtp_mls -p xmtp_cryptography -p xmtp_common -p xmtp_api -p xmtp_id -p xmtp_db -p xmtp_api_d14n"

# Run WASM bindings Rust tests (v3 + d14n)
test: test-v3 test-d14n

# Run WASM bindings Rust tests (v3) with per-test retry on failure
[script("bash")]
test-v3:
    #!/usr/bin/env bash
    set -euo pipefail
    source "{{ justfile_directory() }}/../../dev/wasm-retry"
    wasm_test_with_retry "" "{{ wasm_packages }}"

# Run WASM bindings Rust tests (d14n) with per-test retry on failure
[script("bash")]
test-d14n:
    #!/usr/bin/env bash
    set -euo pipefail
    source "{{ justfile_directory() }}/../../dev/wasm-retry"
    wasm_test_with_retry "--features d14n" "{{ wasm_packages }}"

# Run WASM JS integration tests
[script("bash")]
test-integration: _build-test install
    set -euo pipefail
    cd {{ source_directory() }}
    nix develop .#js --command bash -euc 'yarn tsc --noEmit && yarn vitest run'

# Build WASM bindings
[script("bash")]
build:
    nix build .#wasm-bindings

[private]
_build-test:
    nix build .#wasm-bindings-test
    mkdir -p {{ justfile_directory() }}/bindings/wasm/dist
    cp -r result/dist/* {{ justfile_directory() }}/bindings/wasm/dist
