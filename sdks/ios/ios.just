export NIX_DEVSHELL := env("NIX_DEVSHELL", "default")
set shell := ["../../dev/nix-shell"]

# Verify iOS SDK builds (bindings + Swift)
# Package.swift is at repo root, so cd there for swift commands
check:
  ./dev/bindings && cd "$(git rev-parse --show-toplevel)" && swift build

# Run SwiftLint + SwiftFormat check
lint:
  swiftlint && swiftformat --lint .

# Format Swift code
format:
  swiftformat .

# Run Swift tests
test:
  ./dev/bindings && cd "$(git rev-parse --show-toplevel)" && swift test --parallel

# Build iOS native bindings (xcframework)
build:
  ./dev/bindings
