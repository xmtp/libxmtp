# Run Node.js tests (builds NAPI bindings + vitest)
test:
  nix develop .#js --command yarn test

# Build Node.js NAPI bindings
build:
  nix develop .#js --command yarn build
