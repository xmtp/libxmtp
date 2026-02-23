# Nix Build Packages

All `nix build .#<package>` outputs defined in `flake.nix`.

## Package Reference

| Package | Command | Description |
|---------|---------|-------------|
| `wasm-bindings` | `nix build .#wasm-bindings` | WASM compiled bindings (wasm-pack output) |
| `android-libs` | `nix build .#android-libs` | All Android targets (.so + Kotlin bindings) |
| `android-libs-fast` | `nix build .#android-libs-fast` | Host-matching Android target only |
| `ios-libs` | `nix build .#ios-libs` | All iOS targets (macOS only) |
| `ios-libs-fast` | `nix build .#ios-libs-fast` | Simulator + host macOS only |
| `node-bindings-linux-x64-gnu` | `nix build .#node-bindings-linux-x64-gnu` | Per-target .node binary |
| `node-bindings-linux-x64-musl` | `nix build .#node-bindings-linux-x64-musl` | Per-target .node binary |
| `node-bindings-linux-arm64-gnu` | `nix build .#node-bindings-linux-arm64-gnu` | Per-target .node binary |
| `node-bindings-linux-arm64-musl` | `nix build .#node-bindings-linux-arm64-musl` | Per-target .node binary |
| `node-bindings-darwin-x64` | `nix build .#node-bindings-darwin-x64` | Per-target .node binary |
| `node-bindings-darwin-arm64` | `nix build .#node-bindings-darwin-arm64` | Per-target .node binary |
| `node-bindings-fast` | `nix build .#node-bindings-fast` | Host-matching .node only |
| `node-bindings-js` | `nix build .#node-bindings-js` | Generated index.js + index.d.ts |
| `ffi-uniffi-bindgen` | `nix build .#ffi-uniffi-bindgen` | FFI code generator |
| `wasm-bindgen-cli` | `nix build .#wasm-bindgen-cli` | WASM bindings CLI tool |

## Key Files

| File | Purpose |
|------|---------|
| `nix/package/wasm.nix` | WASM build derivation + dev shell |
| `nix/package/android.nix` | Android build derivation (all targets + aggregate) |
| `nix/package/ios.nix` | iOS build derivation (all targets + aggregate, macOS only) |
| `nix/package/node.nix` | Node.js per-target builds + JS/TS generation |
| `nix/lib/mobile-common.nix` | Shared build args for iOS/Android |
| `nix/lib/filesets.nix` | Source filtering for hermetic builds |

## Node Build Details

Node builds use the crane two-phase pattern (same as iOS/Android):
1. `buildDepsOnly` — compile dependencies (cached per target)
2. `buildPackage` — build the `.node` file using cached deps

Target mapping (Rust triple -> NAPI platform name):

| Rust Target | NAPI Name |
|-------------|-----------|
| `x86_64-unknown-linux-gnu` | `linux-x64-gnu` |
| `x86_64-unknown-linux-musl` | `linux-x64-musl` |
| `aarch64-unknown-linux-gnu` | `linux-arm64-gnu` |
| `aarch64-unknown-linux-musl` | `linux-arm64-musl` |
| `x86_64-apple-darwin` | `darwin-x64` |
| `aarch64-apple-darwin` | `darwin-arm64` |

Windows targets are excluded from Nix (built separately in CI).

The `node-bindings-js` package uses `__noChroot = true` (requires network for `yarn install`). Must run on macOS in CI since Linux enforces `sandbox=true`.
