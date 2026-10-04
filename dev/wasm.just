export NIX_DEVSHELL := env("NIX_DEVSHELL", "wasm")
set shell := ["./dev/nix-shell"]
set working-directory := ".."

nix_system := arch() + "-" + if os() == "macos" { "darwin" } else { "linux" }

wasm_packages := "-p xmtp_mls -p xmtp_cryptography -p xmtp_common -p xmtp_api -p xmtp_id -p xmtp_db -p xmtp_api_backend -p xmtp_content_types -p xmtp_attachments -p xmtp_sdk"

# Check the generated SDK and its WASM core dependencies.
check:
    dev/agent-run cargo check --locked --target wasm32-unknown-unknown -p xmtp_sdk

lint:
    dev/agent-run cargo clippy --locked --target wasm32-unknown-unknown -p xmtp_sdk --all-targets --no-deps -- -Dwarnings

test *args="":
    XMTP_TEST_LOGGING=false RUST_LOG=off dev/agent-run cargo nextest run --locked --profile ci --cargo-profile wasm-test --target wasm32-unknown-unknown {{ wasm_packages }} {{ args }}

nix-test:
    nix build --print-build-logs --no-link .#wasm-nextest.{{ nix_system }} --option sandbox relaxed

test-ci: nix-test
    dev/ci/warm-deps .#wasm-nextest.{{ nix_system }}
