mod android 'sdks/android/android.just'
mod ios 'sdks/ios/ios.just'
mod node 'bindings/node/node.just'
mod wasm 'bindings/wasm/wasm.just'

default:
  just --list --list-submodules

format:
  nix fmt

# test v3, test d14n
test target="all":
  @just _test-{{target}}

[private]
_test-all: (_test-v3) (_test-d14n)

[private]
_test-v3:
  nix develop .#rust --command cargo nextest run --profile ci --workspace --exclude bindings_wasm

[private]
_test-d14n:
  nix develop .#rust --command cargo nextest run --features d14n --profile ci-d14n \
    -E 'package(xmtp_mls)' -E 'rdeps(xmtp_mls)' \
    --workspace --exclude bindings_wasm

lint: lint-rust lint-rust-wasm
  nix develop .#rust --command ./dev/lint-shellcheck

lint-rust:
  nix develop .#rust --command cargo clippy --workspace --all-features --all-targets --no-deps --exclude bindings_wasm -- -Dwarnings

lint-rust-wasm:
  nix develop .#rust --command cargo clippy --workspace \
    --manifest-path ./bindings/wasm/Cargo.toml \
    --all-features --all-targets --no-deps -- -Dwarnings
