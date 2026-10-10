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
  just android check-packages

# Verify JNI stripping in the built debug and release AARs and example APKs.
check-packages:
  python3 -B ./dev/check-native-packages.py

# Compile the library, commonMain UI, example, and instrumented tests.
assemble: build
  ./gradlew -p . :library:assembleDebug :library:assembleDebugAndroidTest :example-shared:assembleDebug :example:assembleDebug :example:assembleDebugAndroidTest --dependency-verification=strict --no-daemon

# Compile both app variants, shared UI and app instrumentation under strict verification.
example-check: build
  ./gradlew -p . :example:assembleDebug :example:assembleRelease :example:assembleDebugAndroidTest :example-shared:assembleDebug :example-shared:assembleRelease --dependency-verification=strict --no-daemon

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

# Run app host and shared screen unit tests with matched native bindings.
example-test *args: build _test-host
  {{ _env }} && JAVA_TOOL_OPTIONS="${JAVA_TOOL_OPTIONS:-} -Djna.library.path={{ root }}/sdks/android/.build/test-host/lib" ./gradlew -p . :example:testDebugUnitTest {{ args }} :example-shared:testDebugUnitTest --dependency-verification=strict --no-daemon

# Run app instrumentation inside the emulator launcher's owned scope.
example-test-integration *args: build
  {{ _env }} && run-test-emulator -- bash -euc 'adb -s "$ANDROID_SERIAL" reverse tcp:$XMTP_BACKEND_PORT tcp:$XMTP_BACKEND_PORT; adb -s "$ANDROID_SERIAL" reverse tcp:$XMTP_S3_PORT tcp:$XMTP_S3_PORT; test_status=0; ./gradlew -p . :example:connectedDebugAndroidTest {{ args }} --dependency-verification=strict --no-daemon || test_status=$?; mkdir -p "{{ root }}/sdks/android/example/build/screenshots"; if adb -s "$ANDROID_SERIAL" shell test -d /sdcard/Pictures/XmtpMessengerProof; then adb -s "$ANDROID_SERIAL" pull /sdcard/Pictures/XmtpMessengerProof/. "{{ root }}/sdks/android/example/build/screenshots"; fi; exit "$test_status"'

# Prove compiled release URL admission and packaged policy with temporary test signing.
example-test-release-integration *args: build
  {{ _env }} && run-test-emulator -- bash -euc 'adb -s "$ANDROID_SERIAL" reverse tcp:$XMTP_BACKEND_PORT tcp:$XMTP_BACKEND_PORT; test_status=0; ./gradlew -p . :example:connectedReleaseAndroidTest -PxmtpExampleReleaseTests=true -Pandroid.testInstrumentationRunnerArguments.class=org.xmtp.android.example.messenger.ReleaseTransportInstrumentedTest {{ args }} --dependency-verification=strict --no-daemon || test_status=$?; exit "$test_status"'

# Run integration tests (requires emulator)
test-integration: build
  {{ _env }} && run-test-emulator -- bash -euc 'adb -s "$ANDROID_SERIAL" reverse tcp:${XMTP_S3_PORT:-9067} tcp:${XMTP_S3_PORT:-9067}; exec ./gradlew -p . :library:connectedCheck --continue'

# Check release JNI, public clients, timestamps, and cancel/replay on API 23.
test-min-sdk:
  {{ _env }} && ./dev/bindings --release && NIX_ANDROID_EMULATOR_API=23 run-test-emulator -- ./gradlew -p . :library:assembleRelease :library:connectedDebugAndroidTest -Pandroid.testInstrumentationRunnerArguments.class=uniffi.xmtp_sdk.AndroidPackageTest\#generatedTimestampsLoadAndConvertWithDesugaring,uniffi.xmtp_sdk.AndroidPackageTest\#contextFactoryPreservesExplicitAndInMemoryStorage,uniffi.xmtp_sdk.AndroidPackageTest\#cancelledCollectorReplaysTheUnacknowledgedMessage --dependency-verification strict --no-daemon

# Check that the negative consumers in library/src/test/negative do not compile.
check-consumers: build
  ./dev/check-consumers

# Generate the public Kotlin API reference.
docs: build
  ./gradlew -p . :library:dokkaGeneratePublicationHtml

# Generate the reference from current release Kotlin sources without JNI.
docs-generated:
  env -u LD_LIBRARY_PATH nix build "{{ root }}#xmtp-sdk-generated-kotlin" --out-link "{{ root }}/sdks/android/.build/docs-generated"
  generated="{{ root }}/sdks/android/.build/docs-generated"; \
    empty_jni="$(mktemp -d)"; \
    trap 'rm -rf "$empty_jni"' EXIT; \
    test -s "$generated/kotlin/sdk-contract.json"; \
    XMTP_SDK_GENERATED_DIR="$generated" XMTP_SDK_ANDROID_JNI_DIR="$empty_jni" \
      ./gradlew -p . :library:dokkaGeneratePublicationHtml --dependency-verification=strict --no-daemon

# Build native bindings
build:
  ./dev/bindings
