export NIX_DEVSHELL := env("NIX_DEVSHELL", "default")
set shell := ["../../dev/nix-shell"]

root := justfile_directory()
generated := root + "/target/sdk-generated"
config := root + "/apps/xmtp_sdk_bindgen/uniffi-global.toml"

# Build the native and unstaged wasm libraries, then write four bindings trees.
[script("bash")]
generate:
    set -euo pipefail
    cd "{{ root }}"
    export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
    source dev/sccache-env
    dev/agent-run cargo build -p xmtp_sdk -p xmtp-sdk-bindgen
    dev/agent-run cargo build -p xmtp_sdk --target wasm32-unknown-unknown
    case "$(uname -s)" in
      Darwin) native=target/debug/libxmtp_sdk.dylib ;;
      Linux) native=target/debug/libxmtp_sdk.so ;;
      *) echo "unsupported host for SDK generation" >&2; exit 1 ;;
    esac
    mkdir -p "{{ generated }}"
    for language in swift kotlin typescript-napi; do
      target/debug/xmtp-sdk-bindgen generate --lib "$native" --language "$language" --out "{{ generated }}/$language" --config "{{ config }}"
    done
    cp "$native" "{{ generated }}/typescript-napi/"
    target/debug/xmtp-sdk-bindgen generate --lib target/wasm32-unknown-unknown/debug/xmtp_sdk.wasm --language typescript-wasm --out "{{ generated }}/typescript-wasm" --config "{{ config }}"
    cp target/wasm32-unknown-unknown/debug/xmtp_sdk.wasm "{{ generated }}/typescript-wasm/"

# Check generated names. The stock ubrn templates emit `any` and `as` casts,
# so root Oxlint and Oxfmt ignore target/sdk-generated/ until upstream fixes them.
[script("bash")]
lint:
    set -euo pipefail
    cd "{{ root }}"
    crates/xmtp_sdk/dev/check-id-spelling "{{ generated }}/swift" "{{ generated }}/kotlin" "{{ generated }}/typescript-napi" "{{ generated }}/typescript-wasm"
    node_modules/.bin/oxlint --config .oxlintrc.json apps/xmtp_sdk_bindgen/runtime/ts "{{ generated }}/typescript-napi" "{{ generated }}/typescript-wasm"
    node_modules/.bin/oxfmt --config .oxfmtrc.json --check apps/xmtp_sdk_bindgen/runtime/ts "{{ generated }}/typescript-napi" "{{ generated }}/typescript-wasm"

check-isolation:
    cd "{{ root }}" && crates/xmtp_sdk/dev/check-isolation

conformance target:
    echo "Conformance for {{ target }} is added by Task 9"

manifest-check:
    echo "Manifest check is added when the SDK manifest exists"

bench:
    echo "SDK benchmark is added with the conformance suite"
