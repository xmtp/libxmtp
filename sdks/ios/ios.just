export NIX_DEVSHELL := env("NIX_DEVSHELL", "ios")

set shell := ["../../dev/nix-shell"]

root := justfile_directory()

# The backend port differs per worktree. `_env` loads dev/docker/.env, written
# by dev/worktree-env, so the suites reach this worktree's own backend.
_env := root + "/dev/worktree-env && set -a && . " + root + "/dev/docker/.env && set +a"

# Verify iOS SDK builds (bindings + Swift)

# Package.swift is at repo root, so cd there for swift commands
check:
    ./dev/bindings && cd "$(git rev-parse --show-toplevel)" && swift build

# Run SwiftLint + SwiftFormat check
lint:
    swiftlint && swiftformat --lint .

# Format Swift code
format:
    swiftformat .

# Run Swift tests on macOS. XMTP_BACKEND_URL selects the backend.
test:
    {{ _env }} && ./dev/bindings && cd "$(git rev-parse --show-toplevel)" && swift test --parallel --num-workers=2 -q

# Build iOS native bindings (xcframework)
build:
    ./dev/bindings

# Run the suite on an installed iOS simulator.
test-simulator destination="platform=iOS Simulator,name=iPhone 17":
    {{ _env }} && ./dev/bindings && cd "$(git rev-parse --show-toplevel)" && TEST_RUNNER_XMTP_BACKEND_URL="$XMTP_BACKEND_URL" xcodebuild test -scheme XMTPiOS -destination '{{ destination }}'

# Generate a static DocC site. The output path is relative to the repository root.
docs output="apps/docs/generated/reference/swift":
    ./dev/bindings && ./script/generate-docs.sh "{{ output }}"
