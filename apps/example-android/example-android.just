export NIX_DEVSHELL := env("NIX_DEVSHELL", "android")
set shell := ["../../dev/nix-shell"]

root := justfile_directory()

# Under CI, Gradle calls add `--build-cache` (see sdks/android/android.just).

# The backend and anvil ports differ per worktree. `_env` loads dev/docker/.env,
# written by dev/worktree-env, so Gradle bakes this worktree's endpoints into
# BuildConfig (see library/build.gradle).
_env := ". " + root + "/dev/docker/load-env"

# Build a signed universal testing APK and print its local path.
apk:
  ./dev/apk

# Refresh the pinned Gradle downloads used by the APK derivation.
apk-update-deps:
  env -u LD_LIBRARY_PATH nix build "{{ root }}#example-android-apk.mitmCache.updateScript" --out-link .build/apk-update-deps
  cd "{{ root }}" && apps/example-android/.build/apk-update-deps

# Gradle tasks shared by the build, lint, and CI build recipes.
_build_tasks := ":example:assembleDebug :example:assembleRelease :example:assembleDebugAndroidTest :example-shared:assembleDebug :example-shared:assembleRelease"
_lint_tasks := "spotlessCheck :example:lintDebug"
_check_packages := "python3.11 -B " + root + "/sdks/android/dev/check-native-packages.py app/build/outputs/apk/debug/example-debug.apk app/build/outputs/apk/release/example-release-unsigned.apk"

# Compile both app variants, shared UI and app instrumentation under strict verification.
check *args: build
  ./gradlew -p . {{ _build_tasks }} {{ args }} ${CI:+--build-cache} --dependency-verification=strict --no-daemon
  {{ _check_packages }}

# Run unit tests, Spotless, Android Lint, and every variant build in one Gradle
# call, then check the APK packages. CI's Messenger build job.
check-all: build _test-host
  {{ _env }} && JAVA_TOOL_OPTIONS="${JAVA_TOOL_OPTIONS:-} -Djna.library.path={{ root }}/sdks/android/.build/test-host/lib" ./gradlew -p . :example:testDebugUnitTest :example-shared:testDebugUnitTest {{ _lint_tasks }} {{ _build_tasks }} --continue ${CI:+--build-cache} --dependency-verification=strict --no-daemon
  {{ _check_packages }}

# Boot a visible emulator and run the example against the existing local backend.
start: build
  {{ _env }} && NIX_ANDROID_EMULATOR_API=34 NIX_ANDROID_EMULATOR_FLAGS="-no-snapshot-save -gpu auto -skin 1080x2400 -memory 4096 -partition-size 8192" run-test-emulator -- bash ./dev/start

# Compile shared screens and Android Studio previews without native bindings.
preview-check *args:
  ./gradlew -p . :example-shared:compileDebugKotlinAndroid {{ args }} --dependency-verification=strict --no-daemon

# Run app host and shared screen unit tests with matched native bindings.
test *args: build _test-host
  {{ _env }} && JAVA_TOOL_OPTIONS="${JAVA_TOOL_OPTIONS:-} -Djna.library.path={{ root }}/sdks/android/.build/test-host/lib" ./gradlew -p . :example:testDebugUnitTest {{ args }} :example-shared:testDebugUnitTest ${CI:+--build-cache} --dependency-verification=strict --no-daemon

# Stage the current backend for the isolated app metadata catalogue.
[private]
_metadata-backend:
  env -u LD_LIBRARY_PATH nix build "{{ root }}#xmtp-backend" --out-link "{{ root }}/apps/example-android/.build/metadata-backend"

# Check real catalogue backend startup and cleanup without an emulator.
metadata-fixture-smoke: _metadata-backend
  {{ _env }} && python3.11 -B fixtures/metadata_backend.py --backend '{{ root }}/apps/example-android/.build/metadata-backend/bin/xmtp-backend' -- true

# Check owned database and process cleanup with local stubs.
metadata-fixture-test *args:
  python3.11 -B fixtures/test_metadata_backend.py {{ args }}

# Run the app instrumentation smoke with the owned catalogue backend in the
# owned emulator scope. It forwards the catalogue, backend and S3 ports.
test-integration *args: build _metadata-backend
  {{ _env }} && python3.11 -B fixtures/metadata_backend.py --backend '{{ root }}/apps/example-android/.build/metadata-backend/bin/xmtp-backend' -- run-test-emulator -- bash -euc 'adb -s "$ANDROID_SERIAL" reverse tcp:$XMTP_METADATA_BACKEND_PORT tcp:$XMTP_METADATA_BACKEND_PORT; adb -s "$ANDROID_SERIAL" reverse tcp:$XMTP_BACKEND_PORT tcp:$XMTP_BACKEND_PORT; adb -s "$ANDROID_SERIAL" reverse tcp:$XMTP_S3_PORT tcp:$XMTP_S3_PORT; python3.11 -B performance/target.py; exec ./gradlew -p . :example:connectedDebugAndroidTest -Pandroid.testInstrumentationRunnerArguments.metadataBackendOwned=true -Pandroid.testInstrumentationRunnerArguments.metadataBackendUrl="$XMTP_METADATA_BACKEND_URL" {{ args }} ${CI:+--build-cache} --dependency-verification=strict --no-daemon'

# Stage the matched SDK bindings and host library.
build:
  just --justfile {{ root }}/justfile android build

[private]
_test-host:
  env -u LD_LIBRARY_PATH nix build "{{ root }}#xmtp-sdk-libs" --out-link "{{ root }}/sdks/android/.build/test-host"

# Check app source format and Android lint.
lint: build
  ./gradlew -p . {{ _lint_tasks }} ${CI:+--build-cache} --dependency-verification=strict --no-daemon

format:
  ./gradlew -p . spotlessApply --dependency-verification=strict --no-daemon
