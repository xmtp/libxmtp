export NIX_DEVSHELL := env("NIX_DEVSHELL", "ios")

# CI points the suite at a deployed backend. Re-export it so it reaches the
# recipe shell; dev/docker/load-env then keeps it ahead of the local default.
export XMTP_BACKEND_URL := env("XMTP_BACKEND_URL", "")

set shell := ["../../dev/nix-shell"]

root := justfile_directory()

# The backend port differs per worktree. `_env` loads dev/docker/.env, written
# by dev/worktree-env, so the suites reach this worktree's own backend.
_env := ". " + root + "/dev/docker/load-env"

# Build the generated SDK and Apple package

# Package.swift is at repo root, so cd there for swift commands
check:
    ./dev/bindings && cd "$(git rev-parse --show-toplevel)" && swift build

# Run SwiftLint + SwiftFormat check
lint:
    swiftlint && swiftformat --lint .

# Format Swift code
format:
    swiftformat .

# Lifecycle checks: reader teardown and the listener gate. `test-lifecycle`
# runs only them, and `test skip-lifecycle` runs every other test.
_lifecycle_tests := "'XmtpSdkTests\\.(ReaderTeardownTests|ListenerGateTests)'"

# Run Swift tests on macOS. XMTP_BACKEND_URL selects the backend.
# `skip-lifecycle` leaves out the tests that `test-lifecycle` runs.
test mode="all" *args="":
    {{ _env }} && ./dev/bindings && cd "$(git rev-parse --show-toplevel)" && swift test --parallel -q {{ if mode == "skip-lifecycle" { "--skip " + _lifecycle_tests } else if mode == "all" { "" } else { error("mode must be all or skip-lifecycle") } }} {{ args }}

# The macOS unit tests against a cargo debug host library and its Swift render,
# with no Nix release build: the inner loop after a Rust edit. The simulator,
# the examples, and CI use the Nix release products (`just ios test`).
test-host *args="":
    {{ _env }} && cd "$(git rev-parse --show-toplevel)" && crates/xmtp_sdk/dev/host-package swift && swift test --parallel -q {{ args }}

# Run only the lifecycle checks on macOS. XMTP_BACKEND_URL selects the backend.
test-lifecycle:
    {{ _env }} && ./dev/bindings && cd "$(git rev-parse --show-toplevel)" && swift test --filter {{ _lifecycle_tests }}

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
