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
    case "{{ target }}" in \
      swift|kotlin|node|browser) bash "{{ root }}/crates/xmtp_sdk/dev/conformance-{{ target }}" ;; \
      *) echo "Expected swift, kotlin, node, or browser" >&2; exit 2 ;; \
    esac

manifest-check:
    echo "Manifest check is added when the SDK manifest exists"

bench:
    bash "{{ root }}/crates/xmtp_sdk/dev/bench"
