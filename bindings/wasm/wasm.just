# Run WASM unit tests
test:
  nix develop .#wasm --command ../../dev/test/wasm-ci --v3
  nix develop .#wasm --command ../../dev/test/wasm-ci --d14n

# Run WASM integration tests (builds wasm + vitest in browser)
test-integration:
  nix develop .#js --command bash -c 'yarn && yarn typecheck && yarn build:test && yarn run vitest run'
# need a nix build .#wasm-bindings-test to be usable with integration tests

# Build WASM bindings
build:
  nix build .#wasm-bindings

lint:
  nix develop .#rust --command \
    cargo clippy --target wasm32-unknown-unknown \
    --manifest-path "./Cargo.toml" \
    --all-features --no-deps -- -Dwarnings

