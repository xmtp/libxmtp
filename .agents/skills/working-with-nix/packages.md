# Nix Build Packages

All `nix build .#<package>` outputs defined in `flake.nix`.

## Package Reference

| Package                          | Command                                      | Description                                 |
| -------------------------------- | -------------------------------------------- | ------------------------------------------- |
| `xmtp-sdk-wasm`                  | `nix build .#xmtp-sdk-wasm`                  | Generated worker WASM   |
| `android-sdk-libs`               | `nix build .#android-sdk-libs`               | All Android targets (.so + Kotlin bindings) |
| `android-sdk-libs-fast`          | `nix build .#android-sdk-libs-fast`          | Host-matching Android target only           |
| `ios-libs`                       | `nix build .#ios-libs`                       | All iOS targets (macOS only)                |
| `ios-libs-fast`                  | `nix build .#ios-libs-fast`                  | Simulator + host macOS only                 |
| `wasm-bindgen-cli`               | `nix build .#wasm-bindgen-cli`               | WASM bindings CLI tool                      |

## Key Files

| File                        | Purpose                                                    |
| --------------------------- | ---------------------------------------------------------- |
| `nix/package/xmtp-sdk.nix`      | Generated SDK WASM build                          |
| `nix/package/android.nix`   | Android build derivation (all targets + aggregate)         |
| `nix/package/ios.nix`       | iOS build derivation (all targets + aggregate, macOS only) |
| `nix/package/xmtp-sdk-native.nix`      | Native SDK shared libraries               |
| `nix/lib/mobile-common.nix` | Shared build args for iOS/Android                          |
| `nix/lib/filesets.nix`      | Source filtering for hermetic builds                       |

## Node Build Details

Node builds use the crane two-phase pattern (same as iOS/Android):

1. `buildDepsOnly` — compile dependencies (cached per target)
2. `buildPackage` — build the SDK shared library using cached deps

Target mapping (Rust triple -> package platform name):

| Rust Target                  | Platform          |
| ---------------------------- | ------------------ |
| `x86_64-unknown-linux-gnu`   | `linux-x64-gnu`    |
| `x86_64-unknown-linux-musl`  | `linux-x64-musl`   |
| `aarch64-unknown-linux-gnu`  | `linux-arm64-gnu`  |
| `aarch64-unknown-linux-musl` | `linux-arm64-musl` |
| `aarch64-apple-darwin`       | `darwin-arm64`     |

Windows targets are excluded from Nix (built separately in CI).
