# Defaults to `default` (full env). CI sets NIX_DEVSHELL=js for leaner builds.
devshell := env("NIX_DEVSHELL", "default")

# Install JS dependencies
install:
  nix develop .#{{devshell}} --command bash -c 'yarn'

# Install JS dependencies (CI - frozen lockfile)
install-ci:
  nix develop .#{{devshell}} --command bash -c 'yarn install --immutable'

# Build Node.js NAPI bindings
check: install
  nix develop .#{{devshell}} --command bash -c '\
    rm -rf dist && \
    yarn napi build --platform --release --esm && \
    mkdir -p dist && mv index.js dist && mv index.d.ts dist && mv *.node dist'

# Check TypeScript formatting
lint: install
  nix develop .#{{devshell}} --command bash -c 'yarn prettier -c .'

# Format TypeScript files
format: install
  nix develop .#{{devshell}} --command bash -c 'yarn prettier -w .'

# Run Node.js tests (builds NAPI bindings with test features + vitest)
test: install
  nix develop .#{{devshell}} --command bash -c '\
    rm -rf dist && \
    yarn napi build --platform --esm --features test-utils && \
    mkdir -p dist && mv index.js dist && mv index.d.ts dist && mv *.node dist && \
    yarn vitest run'

# Build Node.js NAPI bindings (alias)
build: check
