# Defaults to `default` (full env). CI sets NIX_DEVSHELL=android for leaner builds.
devshell := env("NIX_DEVSHELL", "default")

# Verify Android SDK builds (bindings + Gradle)
check:
  nix develop .#{{devshell}} --command bash -c './dev/bindings && ./gradlew -p . build'

# Run Spotless + Android Lint
lint:
  nix develop .#{{devshell}} --command bash -c './gradlew -p . spotlessCheck --continue && ./gradlew -p . :library:lintDebug'

# Format Kotlin code
format:
  nix develop .#{{devshell}} --command bash -c './gradlew -p . spotlessApply'

# Run unit tests
test:
  nix develop .#{{devshell}} --command bash -c './dev/bindings && ./gradlew -p . library:testDebug'

# Run integration tests (requires emulator)
test-integration: build
  nix develop .#{{devshell}} --command bash -c 'run-test-emulator && ./gradlew -p . connectedCheck --continue'

# Build native bindings
build:
  nix develop .#{{devshell}} --command bash -c './dev/bindings'
