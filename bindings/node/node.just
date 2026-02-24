# Defaults to `default` (full env). CI sets NIX_DEVSHELL=js for leaner builds.
devshell := env("NIX_DEVSHELL", "default")

# Install JS dependencies
install:
  nix develop .#{{devshell}} --command bash -c 'yarn'

# Install JS dependencies (CI - frozen lockfile)
install-ci:
  nix develop .#{{devshell}} --command bash -c 'yarn install --immutable'

# Build NAPI bindings and assemble dist (needs Rust — always uses default shell)
[private]
_prepare-dist *extra_flags="":
  nix develop .#default --command bash -c '\
    rm -rf dist && \
    yarn napi build --platform --esm {{extra_flags}} && \
    mkdir -p dist && mv index.js dist && mv index.d.ts dist && mv *.node dist'

# Build Node.js NAPI bindings
check: install (_prepare-dist "--release")

# Check TypeScript formatting
lint: install
  nix develop .#{{devshell}} --command bash -c 'yarn prettier -c .'

# Format TypeScript files
format: install
  nix develop .#{{devshell}} --command bash -c 'yarn prettier -w .'

# Run Node.js tests (builds NAPI bindings with test features + vitest)
test: install (_prepare-dist "--features test-utils")
  nix develop .#{{devshell}} --command bash -c 'yarn vitest run'

# Build Node.js NAPI bindings (alias)
build: check
