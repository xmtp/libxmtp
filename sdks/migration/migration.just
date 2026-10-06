set shell := ["../../dev/nix-shell"]
root := justfile_directory()

# Build the standalone Rust module and its generated bindings.
build target="all":
    cd "{{ root }}" && bash sdks/migration/dev/build "{{ target }}"

stage target:
    cd "{{ root }}" && node sdks/migration/dev/stage-package.mjs "{{ target }}"

mobile-stage target *args:
    cd "{{ root }}" && python3 sdks/migration/dev/stage-mobile.py "{{ target }}" {{ args }}

test-node:
    cd "{{ root }}" && node sdks/migration/conformance/node.mjs

test-browser:
    cd "{{ root }}" && dev/agent-run cargo build --locked -p xmtp_legacy_migration --example fixture_host --target wasm32-unknown-unknown
    cd "{{ root }}" && wasm-bindgen --target web --out-dir target/migration-fixture-host target/wasm32-unknown-unknown/debug/examples/fixture_host.wasm
    cd "{{ root }}" && node sdks/migration/conformance/browser.mjs

test-swift:
    cd "{{ root }}" && NIX_DEVSHELL=ios dev/nix-shell 'swift run --package-path sdks/migration/conformance/swift MigrationConformance "$PWD/crates/xmtp_legacy_migration/fixtures/encrypted.db3"'

test-kotlin:
    cd "{{ root }}" && NIX_DEVSHELL=android dev/nix-shell 'sdks/android/gradlew -p sdks/migration/conformance/kotlin run --no-daemon'

android-build:
    cd "{{ root }}" && NIX_DEVSHELL=android dev/nix-shell 'sdks/android/gradlew -p target/migration-packages/android assembleRelease --no-daemon'

# Run after the current Browser SDK has been built by the docs or SDK workflow.
test-browser-import:
    cd "{{ root }}" && node sdks/migration/conformance/browser.mjs --current-sdk
