export NIX_DEVSHELL := env("NIX_DEVSHELL", "ios")

# CI points the suite at a deployed backend. Re-export it so it reaches the
# recipe shell; dev/docker/load-env then keeps it ahead of the local default.
export XMTP_BACKEND_URL := env("XMTP_BACKEND_URL", "")

set shell := ["../../dev/nix-shell"]

root := justfile_directory()

# The backend port differs per worktree. `_env` loads dev/docker/.env, written
# by dev/worktree-env, so the suites reach this worktree's own backend.
_env := root + "/dev/worktree-env && . " + root + "/dev/docker/load-env"

# Build the generated SDK and Apple package

# Package.swift is at repo root, so cd there for swift commands
check:
    ./dev/bindings && cd "$(git rev-parse --show-toplevel)" && swift build

# Compile both retained example apps against this checkout's SDK.
check-examples:
    ./dev/bindings
    env -u LD xcodebuild build -project XMTPiOSExample/XMTPiOSExample.xcodeproj -scheme XMTPiOSExample -destination 'generic/platform=iOS Simulator' ARCHS=arm64 CODE_SIGNING_ALLOWED=NO
    env -u LD xcodebuild build -project example/example.xcodeproj -scheme example -destination 'generic/platform=iOS Simulator' ARCHS=arm64 CODE_SIGNING_ALLOWED=NO

# Run SwiftLint + SwiftFormat check
lint:
    swiftlint && swiftformat --lint .

# Format Swift code
format:
    swiftformat .

# The seam proofs: reader teardown and the listener gate. The required Swift
# job in test-sdk.yml runs them with `test-seams`. test-ios.yml runs
# `test skip-seams`, so one CI run does not run them twice on macOS.
_seam_tests := "'XmtpSdkTests\\.(ReaderTeardownTests|ListenerGateTests)'"

# Run Swift tests on macOS. XMTP_BACKEND_URL selects the backend.
# `skip-seams` leaves out the tests that `test-seams` runs.
test mode="all":
    {{ _env }} && ./dev/bindings && cd "$(git rev-parse --show-toplevel)" && swift test --parallel -q {{ if mode == "skip-seams" { "--skip " + _seam_tests } else if mode == "all" { "" } else { error("mode must be all or skip-seams") } }}

# Run only the seam proofs on macOS. XMTP_BACKEND_URL selects the backend.
test-seams:
    {{ _env }} && ./dev/bindings && cd "$(git rev-parse --show-toplevel)" && swift test --filter {{ _seam_tests }}

# Run the consumer in Tests/Consumer as a bare executable, then check that the
# negative consumers in Tests/Consumer/Negative do not compile.
check-consumer:
    ./dev/bindings && ./script/check-consumer.sh

# Build iOS native bindings (xcframework)
build:
    ./dev/bindings

# Run the suite on an installed iOS simulator.
test-simulator destination="platform=iOS Simulator,name=iPhone 17":
    {{ _env }} && ./dev/bindings && cd "$(git rev-parse --show-toplevel)" && TEST_RUNNER_XMTP_BACKEND_URL="$XMTP_BACKEND_URL" env -u LD xcodebuild test -scheme XmtpSdk -destination '{{ destination }}'

# Generate a static DocC site. The output path is relative to the repository root.
docs output="apps/docs/generated/reference/swift":
    ./dev/bindings && ./script/generate-docs.sh "{{ output }}"
