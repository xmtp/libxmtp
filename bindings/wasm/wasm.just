# Run WASM unit tests
test:
  nix run nixpkgs#omnix ci run .#wasm

# Run WASM integration tests (builds wasm + vitest in browser)
test-integration:
  nix develop .#wasm --command yarn build:test && vitest run

# Build WASM bindings
build:
  nix build .#wasm-bindings
