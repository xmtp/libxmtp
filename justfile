mod android 'sdks/android/android.just'
mod ios 'sdks/ios/ios.just'
mod node 'bindings/node/node.just'
mod wasm 'bindings/wasm/wasm.just'

default:
  just --list --list-submodules

format:
  nix fmt

# `just test v3` , `just test d14n`, `just test`
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

lint: lint-rust
  nix develop .#rust --command ./dev/lint-shellcheck
  just wasm::lint

lint-rust:
  nix develop .#rust --command cargo clippy --workspace --all-features --all-targets --no-deps --exclude bindings_wasm -- -Dwarnings

# `just backend up`, `just backend down`
backend command="up":
  @just _backend-{{command}}

[private]
_backend-up:
  nix build .#validation-service-image
  dev/docker/up

[private]
_backend-down:
  dev/docker/down
