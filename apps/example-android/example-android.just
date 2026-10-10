export NIX_DEVSHELL := env("NIX_DEVSHELL", "android")
set shell := ["../../dev/nix-shell"]

root := justfile_directory()

# The backend and anvil ports differ per worktree. `_env` loads dev/docker/.env,
# written by dev/worktree-env, so Gradle bakes this worktree's endpoints into
# BuildConfig (see library/build.gradle).
_env := root + "/dev/worktree-env && . " + root + "/dev/docker/load-env"

# Compile both app variants, shared UI and app instrumentation under strict verification.
check *args: build
  ./gradlew -p . :example:assembleDebug :example:assembleRelease :example:assembleDebugAndroidTest :example-shared:assembleDebug :example-shared:assembleRelease {{ args }} --dependency-verification=strict --no-daemon
  python3.11 -B {{ root }}/sdks/android/dev/check-native-packages.py app/build/outputs/apk/debug/example-debug.apk app/build/outputs/apk/release/example-release-unsigned.apk

# Run app host and shared screen unit tests with matched native bindings.
test *args: build _test-host
  {{ _env }} && JAVA_TOOL_OPTIONS="${JAVA_TOOL_OPTIONS:-} -Djna.library.path={{ root }}/sdks/android/.build/test-host/lib" ./gradlew -p . :example:testDebugUnitTest {{ args }} :example-shared:testDebugUnitTest --dependency-verification=strict --no-daemon

# Check startup and teardown of the owned backend without attachment storage.
support-fixture:
  ./dev/unsupported-backend true

# Check startup and teardown of the owned S3 response gate without a device.
io-fixture:
  {{ _env }} && ./dev/attachment-io-proxy true

# Stage the current backend for the isolated app metadata catalogue.
[private]
_metadata-backend:
  env -u LD_LIBRARY_PATH nix build "{{ root }}#xmtp-backend" --out-link "{{ root }}/apps/example-android/.build/metadata-backend"

# Check real catalogue backend startup and cleanup without an emulator.
metadata-fixture-smoke: _metadata-backend
  {{ _env }} && python3.11 -B fixtures/metadata_backend.py --backend '{{ root }}/apps/example-android/.build/metadata-backend/bin/xmtp-backend' -- true

# Check actual PostgreSQL CREATE interruption and uncertain command outcomes.
metadata-fixture-create-test: _metadata-backend
  {{ _env }} && python3.11 -B fixtures/test_metadata_create.py --backend '{{ root }}/apps/example-android/.build/metadata-backend/bin/xmtp-backend'

# Check owned database and process cleanup with local stubs.
metadata-fixture-test *args:
  python3.11 -B fixtures/test_metadata_backend.py {{ args }}

# Run app instrumentation with owned S3, catalogue and emulator scopes.
test-integration *args: build _metadata-backend
  {{ _env }} && ./dev/attachment-io-proxy ./dev/unsupported-backend python3.11 -B fixtures/metadata_backend.py --backend '{{ root }}/apps/example-android/.build/metadata-backend/bin/xmtp-backend' -- run-test-emulator -- bash -euc 'adb -s "$ANDROID_SERIAL" reverse tcp:$XMTP_METADATA_BACKEND_PORT tcp:$XMTP_METADATA_BACKEND_PORT; adb -s "$ANDROID_SERIAL" reverse tcp:$XMTP_BACKEND_PORT tcp:$XMTP_BACKEND_PORT; adb -s "$ANDROID_SERIAL" reverse tcp:$XMTP_S3_PORT tcp:$XMTP_ANDROID_S3_GATE_PORT; adb -s "$ANDROID_SERIAL" reverse tcp:$XMTP_ANDROID_S3_GATE_PORT tcp:$XMTP_ANDROID_S3_GATE_PORT; adb -s "$ANDROID_SERIAL" reverse tcp:$XMTP_TOXIPROXY_PORT tcp:$XMTP_TOXIPROXY_PORT; adb -s "$ANDROID_SERIAL" reverse tcp:$XMTP_TOXIPROXY_API_PORT tcp:$XMTP_TOXIPROXY_API_PORT; adb -s "$ANDROID_SERIAL" reverse tcp:$XMTP_ANDROID_UNSUPPORTED_BACKEND_PORT tcp:$XMTP_ANDROID_UNSUPPORTED_BACKEND_PORT; test_status=0; ./gradlew -p . :example:connectedDebugAndroidTest -Pandroid.injected.androidTest.leaveApksInstalledAfterRun=true -Pandroid.testInstrumentationRunnerArguments.metadataBackendUrl="$XMTP_METADATA_BACKEND_URL" -Pandroid.testInstrumentationRunnerArguments.s3GateApi="http://127.0.0.1:$XMTP_ANDROID_S3_GATE_PORT/_xmtp_attachment_test/" -Pandroid.testInstrumentationRunnerArguments.unsupportedBackendUrl="$XMTP_ANDROID_UNSUPPORTED_BACKEND_URL" -Pandroid.testInstrumentationRunnerArguments.toxicBackendUrl="$XMTP_BACKEND_TOXIC_URL" -Pandroid.testInstrumentationRunnerArguments.toxiproxyApi="$XMTP_TOXIPROXY_API" {{ args }} --dependency-verification=strict --no-daemon || test_status=$?; export_status=0; python3.11 -B fixtures/export_screenshots.py --serial "$ANDROID_SERIAL" --output "{{ root }}/apps/example-android/app/build/screenshots" || export_status=$?; if [ "$test_status" -ne 0 ]; then exit "$test_status"; fi; exit "$export_status"'

# Prove compiled release URL admission and packaged policy with temporary test signing.
test-release-integration *args: build
  {{ _env }} && run-test-emulator -- bash -euc 'adb -s "$ANDROID_SERIAL" reverse tcp:$XMTP_BACKEND_PORT tcp:$XMTP_BACKEND_PORT; test_status=0; ./gradlew -p . :example:connectedReleaseAndroidTest -PxmtpExampleReleaseTests=true -Pandroid.testInstrumentationRunnerArguments.class=org.xmtp.android.example.messenger.ReleaseTransportInstrumentedTest {{ args }} --dependency-verification=strict --no-daemon || test_status=$?; exit "$test_status"'

# Check the fixed performance report validator without a device.
performance-check:
  python3.11 -B -m unittest discover -s performance -p 'test_*.py' -v

# Run the complete workload and cache failure control in the owned Linux API 34 scope.
performance output="app/build/performance": build _metadata-backend
  {{ _env }} && NIX_ANDROID_EMULATOR_API=34 NIX_ANDROID_EMULATOR_FLAGS="-no-snapshot -no-window -gpu swiftshader_indirect -noaudio -cores 4 -memory 4096 -partition-size 16384" python3.11 -B fixtures/metadata_backend.py --backend '{{ root }}/apps/example-android/.build/metadata-backend/bin/xmtp-backend' -- run-test-emulator -- bash -euc 'adb -s "$ANDROID_SERIAL" reverse tcp:$XMTP_METADATA_BACKEND_PORT tcp:$XMTP_METADATA_BACKEND_PORT; exec python3.11 -B performance/run.py --output "{{output}}" --red-control'

# Stage the matched SDK bindings and host library.
build:
  just --justfile {{ root }}/justfile android build

[private]
_test-host:
  env -u LD_LIBRARY_PATH nix build "{{ root }}#xmtp-sdk-libs" --out-link "{{ root }}/sdks/android/.build/test-host"

# Check app source format and Android lint.
lint: build
  ./gradlew -p . spotlessCheck :example:lintDebug --dependency-verification=strict --no-daemon

format:
  ./gradlew -p . spotlessApply --dependency-verification=strict --no-daemon
