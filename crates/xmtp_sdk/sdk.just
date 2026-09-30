export NIX_DEVSHELL := env("NIX_DEVSHELL", "default")
set shell := ["../../dev/nix-shell"]

root := justfile_directory()
_env := root + "/dev/worktree-env && . " + root + "/dev/docker/load-env"

# Build both libraries and write four bindings trees inside the Nix shell.
generate:
    bash "{{ root }}/crates/xmtp_sdk/dev/generate"

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

manifest-check:
    cd "{{ root }}" && python3.11 dev/sdk/inventory.py --check

bench:
    {{ _env }} && bash "{{ root }}/crates/xmtp_sdk/dev/bench"
