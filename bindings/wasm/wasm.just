
# Run WASM integration tests (builds wasm + vitest in browser)
test-integration:
  nix develop .#js --command bash -c 'yarn && yarn typecheck && yarn playwright install && nix build .#wasm-bindings-test && ln -s result/dist {{ justfile_directory()}}/bindings/wasm/dist && yarn run vitest run'
# need a nix build .#wasm-bindings-test to be usable with integration tests



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

(??)# Run WASM integration tests (builds wasm + vitest in browser)
(??)test-integration:
(??)  nix develop .#js --command bash -c 'yarn && yarn typecheck && yarn build:test && yarn run vitest run'
(??)# need a nix build .#wasm-bindings-test to be usable with integration tests

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
