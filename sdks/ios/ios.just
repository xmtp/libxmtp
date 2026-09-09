export NIX_DEVSHELL := env("NIX_DEVSHELL", "ios")

set shell := ["../../dev/nix-shell"]

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
    ./dev/bindings && cd "$(git rev-parse --show-toplevel)" && swift test --parallel --num-workers=2 -q

# Build iOS native bindings (xcframework)
build:
    ./dev/bindings

# Run the suite on an installed iOS simulator.
test-simulator destination="platform=iOS Simulator,name=iPhone 17":
    ./dev/bindings && cd "$(git rev-parse --show-toplevel)" && TEST_RUNNER_XMTP_BACKEND_URL="${XMTP_BACKEND_URL:-http://localhost:5050}" xcodebuild test -scheme XMTPiOS -destination '{{ destination }}'

# Generate a static DocC site. The output path is relative to the repository root.
docs output="apps/docs/generated/reference/swift":
    ./dev/bindings && ./script/generate-docs.sh "{{ output }}"
