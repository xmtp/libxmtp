export NIX_DEVSHELL := env("NIX_DEVSHELL", "android")
set shell := ["../../dev/nix-shell"]

root := justfile_directory() / ".." / ".."

# The backend and anvil ports differ per worktree. `_env` loads dev/docker/.env,
# written by dev/worktree-env, so Gradle bakes this worktree's endpoints into
# BuildConfig (see library/build.gradle).
_env := root + "/dev/worktree-env && set -a && . " + root + "/dev/docker/.env && set +a"

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
[script("bash")]
test: build
  {{ _env }}
  ./gradlew -p . library:testDebugUnitTest

# Run integration tests (requires emulator)
[script("bash")]
test-integration: build
  {{ _env }}
  run-test-emulator && ./gradlew -p . connectedCheck --continue

# Generate the public Kotlin API reference.
docs: build
  ./gradlew -p . :library:dokkaGeneratePublicationHtml

# Build native bindings
build:
  ./dev/bindings
