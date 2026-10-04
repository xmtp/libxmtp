export NIX_DEVSHELL := env("NIX_DEVSHELL", "android")
set shell := ["../../dev/nix-shell"]

root := justfile_directory()

# The backend and anvil ports differ per worktree. `_env` loads dev/docker/.env,
# written by dev/worktree-env, so Gradle bakes this worktree's endpoints into
# BuildConfig (see library/build.gradle).
_env := root + "/dev/worktree-env && . " + root + "/dev/docker/load-env"

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
  ./gradlew -p . spotlessApply --dependency-verification=strict --no-daemon

# Run unit tests
test: build
  {{ _env }} && ./gradlew -p . library:testDebugUnitTest


# Run unit tests against existing matched generated bindings.
test-unit *args:
  {{ _env }} && ./gradlew -p . library:testDebugUnitTest {{ args }} --dependency-verification=strict --no-daemon

# Run integration tests (requires emulator)
test-integration: build
  {{ _env }} && run-test-emulator && adb wait-for-device && adb reverse tcp:${XMTP_S3_PORT:-9067} tcp:${XMTP_S3_PORT:-9067} && ./gradlew -p . connectedCheck --continue

# Check release JNI and timestamp desugaring on the supported API 23 runner.
test-min-sdk:
  ./dev/bindings --release && NIX_ANDROID_EMULATOR_API=23 run-test-emulator && ./gradlew -p . :library:assembleRelease :library:connectedDebugAndroidTest -Pandroid.testInstrumentationRunnerArguments.class=uniffi.xmtp_sdk.AndroidPackageTest\#generatedTimestampsLoadAndConvertWithDesugaring --dependency-verification strict --no-daemon

# Generate the public Kotlin API reference.
docs: build
  ./gradlew -p . :library:dokkaGeneratePublicationHtml

# Build native bindings
build:
  ./dev/bindings
