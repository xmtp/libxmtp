# Defaults to `default` (full env). CI sets NIX_DEVSHELL=ios for leaner builds.
# Darwin only — will fail on Linux (no .#ios nix shell)
devshell := env("NIX_DEVSHELL", "default")

# Verify iOS SDK builds (bindings + Swift)
# Package.swift is at repo root, so cd there for swift commands
check:
  nix develop .#{{devshell}} --command bash -c './dev/bindings && cd "$(git rev-parse --show-toplevel)" && swift build'

# Run SwiftLint + SwiftFormat check
lint:
  nix develop .#{{devshell}} --command bash -c 'swiftlint && swiftformat --lint .'

# Format Swift code
format:
  nix develop .#{{devshell}} --command bash -c 'swiftformat .'

# Run Swift tests
test:
  nix develop .#{{devshell}} --command bash -c './dev/bindings && cd "$(git rev-parse --show-toplevel)" && swift test --parallel'

# Build iOS native bindings (xcframework)
build:
  nix develop .#{{devshell}} --command bash -c './dev/bindings'
