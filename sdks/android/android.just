export NIX_DEVSHELL := env("NIX_DEVSHELL", "android")
set shell := ["../../dev/nix-shell"]

# Verify Android SDK builds (bindings + Gradle)
check: build
  ./gradlew -p . build

# Compile the library, example, and instrumented tests.
assemble: build
  ./gradlew -p . :library:assembleDebug :library:assembleDebugAndroidTest :example:assembleDebug

# Run Spotless + Android Lint
lint: build
  ./gradlew -p . spotlessCheck --continue && ./gradlew -p . :library:lintDebug

# Format Kotlin code
format:
  ./gradlew -p . spotlessApply

# Run unit tests
test: build
  ./gradlew -p . library:testDebugUnitTest

# Run integration tests (requires emulator)
test-integration: build
  run-test-emulator && ./gradlew -p . connectedCheck --continue

# Build native bindings
build:
  ./dev/bindings
