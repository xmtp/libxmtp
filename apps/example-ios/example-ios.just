export NIX_DEVSHELL := env("NIX_DEVSHELL", "ios")
export XMTP_BACKEND_URL := env("XMTP_BACKEND_URL", "")
set shell := ["../../dev/nix-shell"]
root := justfile_directory()
_env := root + "/dev/worktree-env && . " + root + "/dev/docker/load-env"

# Build the app and its test runner from this checkout.
build:
    ./dev/build

# Build and launch on an installed iOS 26 or later simulator.
start *args:
    ./dev/start {{ args }}

# Run focused host tests with the current SDK bindings.
test *args:
    {{ root }}/sdks/ios/dev/bindings
    cd Packages/ExampleKit && swift test {{ args }}

# Run the journeys against this checkout's backend.
test-ui destination="platform=iOS Simulator,name=iPhone 17": build
    {{ _env }} && TEST_RUNNER_XMTP_BACKEND_URL="$XMTP_BACKEND_URL" env -u LD xcodebuild test-without-building -project ExampleApp.xcodeproj -scheme ExampleApp -configuration Debug -destination '{{ destination }}' -derivedDataPath .build/DerivedData

# Check source style and architecture constraints.
lint:
    python3.11 dev/test-lint.py
    swiftlint --strict
    swiftformat --lint ExampleApp ExampleAppUITests Packages/ExampleKit

format:
    swiftformat ExampleApp ExampleAppUITests Packages/ExampleKit
