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

# Run every slice of the Nix WASM tests. Each slice starts its own test
# services on one fixed port block and holds a machine-wide lock, so the
# slices run one after another. nix/ci-checks.nix (`wasmPartitions`) and
# .github/workflows/test-wasm.yml name the same slices.
nix-test:
    status=0; for part in 1 2 3 4; do nix build --print-build-logs --no-link ".#wasm-nextest-part-$part.{{ nix_system }}" --option sandbox relaxed || status=1; done; exit "$status"

# Run one slice of the Nix WASM tests; the first slice also builds the archive.
# `result-wasm-<part>/junit.xml` keeps the slice's JUnit report.
nix-test-part part:
    nix build --print-build-logs --out-link result-wasm-{{ part }} .#wasm-nextest-part-{{ part }}.{{ nix_system }} --option sandbox relaxed
