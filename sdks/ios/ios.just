export NIX_DEVSHELL := env("NIX_DEVSHELL", "ios")

# CI points the suite at a deployed backend. Re-export it so it reaches the
# recipe shell; dev/docker/load-env then keeps it ahead of the local default.
export XMTP_BACKEND_URL := env("XMTP_BACKEND_URL", "")

set shell := ["../../dev/nix-shell"]

root := justfile_directory()

# The backend port differs per worktree. `_env` loads dev/docker/.env, written
# by dev/worktree-env, so the suites reach this worktree's own backend.
_env := root + "/dev/worktree-env && . " + root + "/dev/docker/load-env"

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
