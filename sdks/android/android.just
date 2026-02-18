# Run Android unit tests
test:
  nix develop .#android --command ./gradlew -p . library:testDebug

# Run integration tests with android emulator
test-integration:
  nix develop .#android --command run-test-emulator
  nix develop .#android --command ./gradlew -p . connectedCheck --continue

# Build Android native bindings (.so + Kotlin)
build:
  ./dev/bindings
