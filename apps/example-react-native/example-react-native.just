# The canonical `react-native` shell requires /Applications/Xcode_27.0.app;
# `react-native-local` uses the selected Xcode. On Linux they are the same.
export NIX_DEVSHELL := env("NIX_DEVSHELL", "react-native-local")
set shell := ["../../dev/nix-shell"]

# Link and verify the native SDK package (Cachix substitute; no Rust build).
sdk:
    ./dev/sdk native

# Link the JS-only SDK product, enough for typecheck, lint and unit tests.
sdk-js:
    ./dev/sdk js

# Install JS dependencies from the lockfile and, on macOS, the pods.
install:
    ./dev/install

# Typecheck, lint, check formatting and run the unit tests.
check:
    ./dev/check

# Format the app's JS, TS and JSON sources.
format:
    ./node_modules/.bin/oxfmt --config ../../.oxfmtrc.json --write .

# Build the simulator app. CONFIGURATION=Release embeds the JS bundle.
build-ios:
    ./dev/build-ios

# Start Metro on XMTP_RN_METRO_PORT (default 8081).
metro *args:
    ./dev/metro {{ args }}

# Build and launch on XMTP_RN_SIMULATOR; append --headless to keep Simulator.app closed.
start-ios *args:
    ./dev/start-ios {{ args }}

# Build the APK. CONFIGURATION=Release embeds the JS bundle.
build-android:
    ./dev/build-android

# Build and run on a scoped emulator; append --headless or --screenshot FILE.
start-android *args:
    ./dev/start-android {{ args }}
