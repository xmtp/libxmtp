export NIX_DEVSHELL := env("NIX_DEVSHELL", "default")
set shell := ["../../dev/nix-shell"]

root := justfile_directory()
_env := root + "/dev/worktree-env && . " + root + "/dev/docker/load-env"

# Build selected artifacts, then render only the selected target roots.
generate targets="swift,kotlin,node,browser":
    bash "{{ root }}/crates/xmtp_sdk/dev/generate" --targets "{{ targets }}"

check-clean-generate:
    bash "{{ root }}/crates/xmtp_sdk/dev/check-generate-cleans-stale"

# Check generated names, test hooks, and the TypeScript bridge we maintain.
# Run `just sdk generate` first: lint reads every generated target root.
lint:
    bash "{{ root }}/crates/xmtp_sdk/dev/lint-generated"

# Check every tracked or unignored SDK source file.
check-file-sizes:
    cd "{{ root }}" && crates/xmtp_sdk/dev/check-file-sizes

# Import the staged WASM package and initialize its UniFFI player in Node.
wasm-init:
    bash "{{ root }}/crates/xmtp_sdk/dev/check-wasm-init"

# Bridge runtime unit tests in apps/xmtp_sdk_bindgen/runtime-tests/ts. Extra
# arguments go to Vitest. Run `just sdk generate` first.
test-bridge *args:
    bash "{{ root }}/crates/xmtp_sdk/dev/run-bridge-tests" {{ args }}

# Browser platform proofs in sdks/browser/test/platform: real WASM workers,
# Chromium storage, package, logging and bridge proofs, and the Vitest
# Playwright suite. Builds the panic and pure codec fixtures in the Rust shell.
test-browser:
    bash "{{ root }}/crates/xmtp_sdk/dev/run-browser-platform-tests"

# Run only the real-worker OPFS proof against the staged SDK.
test-browser-storage: wasm-init
    {{ _env }} && cd "{{ root }}" && NODE_OPTIONS= sdks/node/node_modules/.bin/tsx sdks/browser/test/platform/storage.chromium.mts

# Run only the package worker ownership and final cleanup proof in Chromium.
test-browser-package: wasm-init
    {{ _env }} && cd "{{ root }}" && NODE_OPTIONS= sdks/node/node_modules/.bin/tsx sdks/browser/test/platform/package.chromium.mts

# Run `just sdk generate` first: the root check reads the generated roots.
# Build once. Later render calls reuse the recorded artifact hashes.
build targets="swift,kotlin,node,browser" *args:
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/sdk-artifacts.py build --targets "{{ targets }}" {{ args }}

render targets="swift,kotlin,node,browser" *args:
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/sdk-artifacts.py render --targets "{{ targets }}" {{ args }}

stage target:
    cd "{{ root }}" && node crates/xmtp_sdk/dev/stage-package.mjs "{{ target }}"

package-smoke target="node":
    cd "{{ root }}" && bash crates/xmtp_sdk/dev/package-smoke "{{ target }}"

# Evaluate native build inputs. This command does not compile a product.
check-native-nix:
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/check-native-nix.py
    cd "{{ root }}" && python3 -B crates/xmtp_sdk/dev/check-source-nix.py

# Check the real generation and mobile consumer build closures.
check-generated-nix:
    cd "{{ root }}" && python3 -B crates/xmtp_sdk/dev/check-generated-nix.py

check-package-scripts:
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/test-native-nix-gate.py
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/test-generated-records.py
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/test-sdk-artifacts.py
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/test-sdk-ios-openssl.py
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/test-sdk-packaging.py
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/test-sdk-native-build-context.py
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/test-sdk-staging.py
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/test-sdk-source-package.py
    cd "{{ root }}" && node crates/xmtp_sdk/dev/test-package-smoke-path.mjs
    cd "{{ root }}" && node --test crates/xmtp_sdk/dev/test-node-platforms.mjs
    cd "{{ root }}" && node --test crates/xmtp_sdk/dev/test-stamp-package.mjs
    cd "{{ root }}" && node crates/xmtp_sdk/dev/test-credential-record.mjs
    cd "{{ root }}" && node crates/xmtp_sdk/dev/test-callback-rejection.mjs
    cd "{{ root }}" && node crates/xmtp_sdk/dev/test-pure-wasm.mjs
    cd "{{ root }}" && node crates/xmtp_sdk/dev/test-generated-assets.mjs

# Use the matching ios or android Nix shell. This does not switch a shipped SDK.
mobile-build target:
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/mobile-package.py build "{{ target }}"

mobile-stage target:
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/mobile-package.py stage "{{ target }}"

# Measure the staged package on one host (node, browser, swift or kotlin).
# Writes results.json under target/sdk-bench/. See benchmarks/README.md.
bench host *args:
    {{ _env }} && cd "{{ root }}" && python3 crates/xmtp_sdk/benchmarks/bench.py {{ quote(host) }} {{ args }}

# Check the benchmark runners without a backend, device or SDK build.
bench-check:
    cd "{{ root }}" && python3 -m unittest discover -s crates/xmtp_sdk/benchmarks -p 'test_*.py'
    cd "{{ root }}" && python3 crates/xmtp_sdk/benchmarks/bench.py check
    cd "{{ root }}" && node --test crates/xmtp_sdk/benchmarks/test_workload.mjs

# Tiny NDK probes verify compiler, linker, archiver, and ELF ABI selection.
check-android-toolchain:
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/check-android-toolchain.py

# Run the normal package migration proofs after generate and stage.
conformance-migration target="node":
    cd "{{ root }}" && case "{{ target }}" in \
      node) node crates/xmtp_sdk/conformance/migration/node.mjs ;; \
      browser) dev/agent-run cargo build --locked -p xmtp_legacy_migration --example fixture_host --target wasm32-unknown-unknown && wasm-bindgen --target web --out-dir target/migration-fixture-host target/wasm32-unknown-unknown/debug/examples/fixture_host.wasm && node crates/xmtp_sdk/conformance/migration/browser.mjs --current-sdk && node crates/xmtp_sdk/conformance/migration/browser-regressions.mjs ;; \
      swift) NIX_DEVSHELL=ios dev/nix-shell 'swift run --package-path crates/xmtp_sdk/conformance/migration/swift MigrationConformance "$PWD/crates/xmtp_legacy_migration/fixtures/encrypted.db3"' ;; \
      kotlin) NIX_DEVSHELL=android dev/nix-shell 'sdks/android/gradlew -p crates/xmtp_sdk/conformance/migration/kotlin run --no-daemon' ;; \
      *) echo "Expected node, browser, swift, or kotlin" >&2; exit 2 ;; \
    esac
