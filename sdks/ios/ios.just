# Darwin only — will fail on Linux (no .#ios nix shell)

# Run Swift tests
test:
  nix develop .#ios --command ./dev/test

# Build iOS native bindings (xcframework)
build:
  ./dev/bindings
