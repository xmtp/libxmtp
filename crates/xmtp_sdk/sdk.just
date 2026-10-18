export NIX_DEVSHELL := env("NIX_DEVSHELL", "default")
set shell := ["../../dev/nix-shell"]

root := justfile_directory()
_env := ". " + root + "/dev/docker/load-env"

# Build selected artifacts, then render only the selected target roots.
generate targets="swift,kotlin,node,browser" *args:
    "{{ root }}/crates/xmtp_sdk/dev/generate" --targets "{{ targets }}" {{ args }}

# Source checks that need no generated tree. CI runs them before generating.
lint-source:
    cd "{{ root }}" && crates/xmtp_sdk/dev/check-file-sizes
    cd "{{ root }}" && crates/xmtp_sdk/dev/check-test-hooks --self-test
    cd "{{ root }}" && crates/xmtp_sdk/dev/check-public-names
    cd "{{ root }}" && node_modules/.bin/oxfmt --config .oxfmtrc.json --check apps/xmtp_sdk_bindgen/runtime/ts
    cd "{{ root }}" && node_modules/.bin/oxfmt --config apps/xmtp_sdk_bindgen/templates/bridge/oxfmt.json --check apps/xmtp_sdk_bindgen/templates/bridge

# Check generated names, test hooks, and the TypeScript bridge we maintain.
# Run `just sdk generate` first: it reads every generated target root.
lint-generated:
    "{{ root }}/crates/xmtp_sdk/dev/lint-generated"

lint: lint-source lint-generated

# Check every tracked or unignored SDK source file.
check-file-sizes:
    cd "{{ root }}" && crates/xmtp_sdk/dev/check-file-sizes

# Import the staged WASM package and initialize its UniFFI player in Node.
wasm-init:
    "{{ root }}/crates/xmtp_sdk/dev/check-wasm-init"

# Bridge runtime unit tests in apps/xmtp_sdk_bindgen/runtime-tests/ts and the
# generated conformance.gen.test.ts against the staged SDK. The generated test
# also runs the real WASM worker proofs bridge.real.mts and
# bridge.values.real.mts against this worktree's backend. The tests load the
# generated WASM and pure trees, which link the @ubjs packages in their
# node_modules. Extra arguments go to Vitest. Run `just sdk generate` first.
test-bridge *args:
    {{ _env }} && cd "{{ root }}" && NODE_OPTIONS=--preserve-symlinks sdks/browser/node_modules/.bin/vitest run --config apps/xmtp_sdk_bindgen/runtime-tests/ts/vitest.config.mts --browser.enabled=false {{ args }}

# Browser platform proofs in sdks/browser/test/platform: real WASM workers,
# Chromium storage, package, logging and bridge proofs, and the Vitest
# Playwright suite. Copies the Nix-built panic and pure codec fixtures.
# `mode` is `all` or `migration` (only the legacy migration proofs).
test-browser mode="all":
    "{{ root }}/crates/xmtp_sdk/dev/run-browser-platform-tests" {{ quote(mode) }}

# Run only the real-worker and Chromium log queue proofs.
test-browser-logging: wasm-init
    cd "{{ root }}" && crates/xmtp_sdk/dev/copy-browser-fixtures
    cd "{{ root }}" && NIX_DEVSHELL=js "{{ root }}/dev/nix-shell" 'NODE_OPTIONS=--preserve-symlinks sdks/node/node_modules/.bin/tsx sdks/browser/test/platform/bridge.log.real.mts'
    cd "{{ root }}" && NIX_DEVSHELL=js "{{ root }}/dev/nix-shell" 'NODE_OPTIONS= sdks/node/node_modules/.bin/tsx sdks/browser/test/platform/logging.chromium.mts'

# Run only the real-worker OPFS proof against the staged SDK.
test-browser-storage: wasm-init
    {{ _env }} && cd "{{ root }}" && NODE_OPTIONS= sdks/node/node_modules/.bin/tsx sdks/browser/test/platform/storage.chromium.mts

# Run only the package worker ownership and final cleanup proof in Chromium.
test-browser-package: wasm-init
    {{ _env }} && cd "{{ root }}" && NODE_OPTIONS= sdks/node/node_modules/.bin/tsx sdks/browser/test/platform/package.chromium.mts

# Run `just sdk generate` first: the root check reads the generated roots.
# Build once. Later render calls reuse the recorded artifact hashes.
build targets="swift,kotlin,node,browser" *args:
    cd "{{ root }}" && python3.11 crates/xmtp_sdk/dev/sdk-artifacts.py build --targets "{{ targets }}" {{ args }}

render targets="swift,kotlin,node,browser" *args:
    cd "{{ root }}" && python3.11 crates/xmtp_sdk/dev/sdk-artifacts.py render --targets "{{ targets }}" {{ args }}

stage target:
    cd "{{ root }}" && node crates/xmtp_sdk/dev/stage-package.mjs "{{ target }}"

# Stage the public product, then pack, install, and smoke it (node or browser).
package-smoke target="node":
    cd "{{ root }}" && node crates/xmtp_sdk/dev/stage-package.mjs {{ quote(target) }}
    cd "{{ root }}" && node crates/xmtp_sdk/dev/package-smoke.mjs {{ quote(target) }}

# Check the real generation and mobile consumer build closures.
check-generated-nix:
    cd "{{ root }}" && python3.11 -B crates/xmtp_sdk/dev/check-generated-nix.py

# Every dev/test_*.py and dev/test-*.mjs, found by discovery. With no path,
# `node --test` applies its default patterns; the grep fails when it ran none.
check-package-scripts:
    cd "{{ root }}" && python3.11 -B -m unittest discover -s crates/xmtp_sdk/dev -p 'test_*.py'
    cd "{{ root }}/crates/xmtp_sdk/dev" && set -o pipefail && node --test --test-reporter=spec | tee /dev/stderr | grep 'tests [1-9]' >/dev/null

# Stage the iOS package or the Android AAR from the Nix release libraries
# (target/sdk-packages/<target>). Android reads the Kotlin tree in
# target/sdk-generated: run `XMTP_SDK_BUILD=nix just sdk generate kotlin` first.
mobile-stage target:
    cd "{{ root }}" && python3.11 crates/xmtp_sdk/dev/mobile-package.py stage "{{ target }}"

# Measure the staged package on one host (node, browser, swift or kotlin).
# Writes results.json under target/sdk-bench/. See benchmarks/README.md.
bench host *args:
    {{ _env }} && cd "{{ root }}" && python3.11 crates/xmtp_sdk/benchmarks/bench.py {{ quote(host) }} {{ args }}

# Check the benchmark runners without a backend, device or SDK build.
bench-check:
    cd "{{ root }}" && python3.11 crates/xmtp_sdk/benchmarks/bench.py check
    cd "{{ root }}" && node --test crates/xmtp_sdk/benchmarks/test_workload.mjs
