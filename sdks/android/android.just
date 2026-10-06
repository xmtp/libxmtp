export NIX_DEVSHELL := env("NIX_DEVSHELL", "android")
set shell := ["../../dev/nix-shell"]

root := justfile_directory()

# The backend and anvil ports differ per worktree. `_env` loads dev/docker/.env,
# written by dev/worktree-env, so Gradle bakes this worktree's endpoints into
# BuildConfig (see library/build.gradle).
_env := root + "/dev/worktree-env && . " + root + "/dev/docker/load-env"

# Verify Android SDK builds (bindings + Gradle)
check: build _test-host
  {{ _env }} && JAVA_TOOL_OPTIONS="${JAVA_TOOL_OPTIONS:-} -Djna.library.path={{ root }}/sdks/android/.build/test-host/lib" ./gradlew -p . build --dependency-verification=strict --no-daemon --stacktrace

# Compile the library, example, and instrumented tests.
assemble: build
  ./gradlew -p . :library:assembleDebug :library:assembleDebugAndroidTest :example:assembleDebug

# Run Spotless + Android Lint
lint: build
  ./gradlew -p . spotlessCheck --continue && ./gradlew -p . :library:lintDebug

# Format Kotlin code
format:
  ./gradlew -p . spotlessApply --dependency-verification=strict --no-daemon

# Stage the matched host library for JVM native calls.
[private]
_test-host:
  env -u LD_LIBRARY_PATH nix build "{{ root }}#xmtp-sdk-libs" --out-link "{{ root }}/sdks/android/.build/test-host"

# Run unit tests with the generated SDK and its matched host library.
test: build _test-host
  {{ _env }} && JAVA_TOOL_OPTIONS="${JAVA_TOOL_OPTIONS:-} -Djna.library.path={{ root }}/sdks/android/.build/test-host/lib" ./gradlew -p . library:testDebugUnitTest --dependency-verification=strict --no-daemon


# Run unit tests against existing matched generated bindings.
test-unit *args:
  {{ _env }} && ./gradlew -p . library:testDebugUnitTest {{ args }} --dependency-verification=strict --no-daemon

# Run integration tests (requires emulator)
test-integration: build
  {{ _env }} && run-test-emulator -- bash -euc 'adb -s "$ANDROID_SERIAL" reverse tcp:${XMTP_S3_PORT:-9067} tcp:${XMTP_S3_PORT:-9067}; exec ./gradlew -p . connectedCheck --continue'

# Check release JNI, public clients, timestamps, and cancel/replay on API 23.
test-min-sdk:
  {{ _env }} && ./dev/bindings --release && NIX_ANDROID_EMULATOR_API=23 run-test-emulator -- ./gradlew -p . :library:assembleRelease :library:connectedDebugAndroidTest -Pandroid.testInstrumentationRunnerArguments.class=uniffi.xmtp_sdk.AndroidPackageTest\#generatedTimestampsLoadAndConvertWithDesugaring,uniffi.xmtp_sdk.AndroidPackageTest\#contextFactoryPreservesExplicitAndInMemoryStorage,uniffi.xmtp_sdk.AndroidPackageTest\#cancelledCollectorReplaysTheUnacknowledgedMessage --dependency-verification strict --no-daemon

# Generate the public Kotlin API reference.
docs: build
  ./gradlew -p . :library:dokkaGeneratePublicationHtml

# Build native bindings
build:
  ./dev/bindings
