export NIX_DEVSHELL := env("NIX_DEVSHELL", "default")
set shell := ["../../dev/nix-shell"]

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

# Packages that don't support the WASM target
wasm_exclude := "--exclude xmtpv3 --exclude bindings_node --exclude xmtp_cli --exclude xdbg --exclude mls_validation_service --exclude xmtp_api_grpc --exclude xmtp-db-tools"

# Run WASM unit tests (v3 + d14n)
test: test-v3 test-d14n

# Run WASM v3 unit tests
test-v3:
    XMTP_TEST_LOGGING=false RUST_LOG=off cargo test --locked --target wasm32-unknown-unknown \
      --workspace {{wasm_exclude}}

# Run WASM d14n unit tests
test-d14n:
    XMTP_TEST_LOGGING=false RUST_LOG=off cargo test --locked --target wasm32-unknown-unknown \
      --features d14n --no-fail-fast \
      --workspace {{wasm_exclude}}

# Run WASM integration tests (vitest in browser — needs .#js shell for Playwright)
[script("bash")]
test-integration: _build-test install
    set -euo pipefail
    cd {{source_directory()}}
    nix develop .#js --command bash -euc 'yarn tsc --noEmit && yarn vitest run'

# Build WASM bindings (via Nix)
[script("bash")]
build:
    nix build .#wasm-bindings

[private]
_build-test:
  nix build .#wasm-bindings-test
  mkdir -p {{ justfile_directory() }}/bindings/wasm/dist
  cp -r result/dist/* {{ justfile_directory() }}/bindings/wasm/dist
