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
lint:
    bash "{{ root }}/crates/xmtp_sdk/dev/lint-generated"

# Check every tracked or unignored SDK source file, including conformance files.
check-file-sizes:
    cd "{{ root }}" && crates/xmtp_sdk/dev/check-file-sizes

# Import the staged WASM package and initialize its UniFFI player in Node.
wasm-init:
    bash "{{ root }}/crates/xmtp_sdk/dev/check-wasm-init"

check-isolation base="":
    cd "{{ root }}" && crates/xmtp_sdk/dev/check-isolation {{ base }}

[private]
_browser-pure-fixture target:
    if [ "{{ target }}" = browser ]; then \
      cd "{{ root }}" && NIX_DEVSHELL=rust dev/nix-shell 'bash crates/xmtp_sdk/dev/prepare-pure-codec-fixture'; \
    fi

[private]
_browser-panic-fixture target:
    if [ "{{ target }}" = browser ]; then \
      cd "{{ root }}" && NIX_DEVSHELL=rust dev/nix-shell 'bash crates/xmtp_sdk/dev/prepare-bridge-panic-fixture'; \
    fi

conformance target: (_browser-pure-fixture target) (_browser-panic-fixture target)
    {{ _env }} && case "{{ target }}" in \
      swift|kotlin|node|browser) bash "{{ root }}/crates/xmtp_sdk/dev/conformance-{{ target }}" ;; \
      bridge) cd "{{ root }}" && \
        bash crates/xmtp_sdk/dev/check-wasm-init && \
        NODE_OPTIONS=--preserve-symlinks sdks/browser/node_modules/.bin/vitest run --config crates/xmtp_sdk/conformance/browser/vitest.config.mts --browser.enabled=false && \
        NODE_OPTIONS="--preserve-symlinks --expose-gc" sdks/node/node_modules/.bin/tsx crates/xmtp_sdk/conformance/browser/bridge.real.mts && \
        NODE_OPTIONS="--preserve-symlinks --expose-gc" sdks/node/node_modules/.bin/tsx crates/xmtp_sdk/conformance/browser/release.gc.mts ;; \
      *) echo "Expected swift, kotlin, node, browser, or bridge" >&2; exit 2 ;; \
    esac

# Repeat held callback and constructor adoption probes on a native host.
# browser-transport checks the real worker transport without a generated SDK.
callback-lifetime target:
    {{ _env }} && cd "{{ root }}" && case "{{ target }}" in \
      swift|kotlin|node) SDK_CALLBACK_LIFETIME=1 SDK_CALLBACK_SOURCE_SHA="$(git rev-parse HEAD)" bash "crates/xmtp_sdk/dev/conformance-{{ target }}" ;; \
      browser-transport) bash crates/xmtp_sdk/dev/callback-lifetime-browser ;; \
      *) echo "Expected swift, kotlin, node, or browser-transport" >&2; exit 2 ;; \
    esac

conformance-bridge:
    bash "{{ root }}/crates/xmtp_sdk/dev/run-bridge-conformance"

# Run the focused real-worker OPFS proof against the staged SDK.
conformance-storage: wasm-init
    {{ _env }} && cd "{{ root }}" && NODE_OPTIONS= sdks/node/node_modules/.bin/tsx crates/xmtp_sdk/conformance/browser/storage.chromium.mts

# Real package worker ownership and final cleanup in Chromium.
conformance-package: wasm-init
    {{ _env }} && cd "{{ root }}" && NODE_OPTIONS= sdks/node/node_modules/.bin/tsx crates/xmtp_sdk/conformance/browser/package.chromium.mts

conformance-bridge-unit *args: wasm-init
    cd "{{ root }}" && NODE_OPTIONS=--preserve-symlinks sdks/browser/node_modules/.bin/vitest run --config crates/xmtp_sdk/conformance/browser/vitest.config.mts --browser.enabled=false {{ args }}

# Compile separate Swift and Kotlin consumers against staged public products.
# Run `just sdk generate` first.
public-consumer:
    bash "{{ root }}/crates/xmtp_sdk/dev/public-consumer"

# Compile or run an independent codec package through the Node/browser ESM roots.
codec-author target="types":
    {{ _env }} && bash "{{ root }}/crates/xmtp_sdk/dev/codec-author" {{ target }}

# Run `just sdk generate` first: the root check reads the generated roots.
manifest-check:
    cd "{{ root }}" && just sdk-manifest-check
    cd "{{ root }}" && python3.11 crates/xmtp_sdk/dev/check-manifest-roots.py

bench:
    {{ _env }} && bash "{{ root }}/crates/xmtp_sdk/dev/bench"

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

check-package-scripts:
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/test-native-nix-gate.py
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/test-sdk-artifacts.py
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/test-sdk-packaging.py
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/test-sdk-staging.py
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/test-sdk-source-package.py
    cd "{{ root }}" && node crates/xmtp_sdk/dev/test-package-smoke-path.mjs
    cd "{{ root }}" && node crates/xmtp_sdk/dev/test-credential-record.mjs
    cd "{{ root }}" && node crates/xmtp_sdk/dev/test-callback-rejection.mjs
    cd "{{ root }}" && node --test crates/xmtp_sdk/dev/test-stamp-package.mjs
    cd "{{ root }}" && node crates/xmtp_sdk/dev/test-pure-wasm.mjs
    cd "{{ root }}" && node crates/xmtp_sdk/dev/test-generated-assets.mjs

# Use the matching ios or android Nix shell. This does not switch a shipped SDK.
mobile-build target:
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/mobile-package.py build "{{ target }}"

mobile-stage target:
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/mobile-package.py stage "{{ target }}"

# Compare complete installed release packages with the approved paired gates.
cutover-bench target config output:
    {{ _env }} && cd "{{ root }}" && python3 crates/xmtp_sdk/benchmarks/runner.py {{ quote(target) }} {{ quote(config) }} {{ quote(output) }}

cutover-bench-check:
    cd "{{ root }}" && python3 -m unittest discover -s crates/xmtp_sdk/benchmarks -p 'test_*.py'

# Use the ios Nix shell. The product is an installed public Swift package.
cutover-bench-ios-prepare config output:
    cd "{{ root }}" && python3 crates/xmtp_sdk/benchmarks/hosts/ios/prepare.py prepare "{{ config }}" "{{ output }}"

cutover-bench-ios-controls config output:
    cd "{{ root }}" && python3 crates/xmtp_sdk/benchmarks/ios_controls.py "{{ config }}" "{{ output }}"

cutover-bench-ios-build output udid derived:
    cd "{{ root }}" && python3 crates/xmtp_sdk/benchmarks/hosts/ios/prepare.py build "{{ output }}" "{{ udid }}" "{{ derived }}"

cutover-bench-baselines output:
    cd "{{ root }}" && python3 crates/xmtp_sdk/benchmarks/baselines.py "{{ output }}"

# Tiny NDK probes verify compiler, linker, archiver, and ELF ABI selection.
check-android-toolchain:
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/check-android-toolchain.py

# Run live-content regressions in Node, Chromium, and compiled mobile helpers.
cutover-bench-stream-check output browser_tools:
    cd "{{ root }}" && python3 crates/xmtp_sdk/benchmarks/stream_controls.py "{{ output }}" --browser-tools "{{ browser_tools }}"

# Check selected-host counter injection with generated bindings and no Rust build.
check-conformance-targets generated="target/sdk-generated":
    cd "{{ root }}" && python3 crates/xmtp_sdk/dev/test-conformance-targets.py "{{ generated }}"

# Check nonthrowing, pending and READY reader cancellation through real native calls.
caller-cancellation-swift:
    {{ _env }} && SDK_SWIFT_CALLER_CANCELLATION=1 bash "{{ root }}/crates/xmtp_sdk/dev/conformance-swift"
