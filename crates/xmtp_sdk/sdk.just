export NIX_DEVSHELL := env("NIX_DEVSHELL", "default")
set shell := ["../../dev/nix-shell"]

root := justfile_directory()

# Build both libraries and write four bindings trees inside the Nix shell.
generate:
    bash "{{ root }}/crates/xmtp_sdk/dev/generate"

check-clean-generate:
    bash "{{ root }}/crates/xmtp_sdk/dev/check-generate-cleans-stale"

# Check generated names and the TypeScript bridge we maintain.
lint:
    bash "{{ root }}/crates/xmtp_sdk/dev/lint-generated"

# Import the staged WASM package and initialize its UniFFI player in Node.
wasm-init:
    bash "{{ root }}/crates/xmtp_sdk/dev/check-wasm-init"

check-isolation:
    cd "{{ root }}" && crates/xmtp_sdk/dev/check-isolation

conformance target:
    echo "Conformance for {{ target }} is added by Task 9"

manifest-check:
    echo "Manifest check is added when the SDK manifest exists"

bench:
    echo "SDK benchmark is added with the conformance suite"
