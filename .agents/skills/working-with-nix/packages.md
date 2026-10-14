# Nix Build Packages

All `nix build .#<package>` outputs defined in `flake.nix`.

## Package Reference

| Package                          | Command                                      | Description                                 |
| -------------------------------- | -------------------------------------------- | ------------------------------------------- |
| `xmtp-sdk-wasm`                  | `nix build .#xmtp-sdk-wasm`                  | Generated worker WASM                       |
| `android-sdk-libs`               | `nix build .#android-sdk-libs`               | All Android targets (.so + Kotlin bindings) |
| `android-sdk-libs-fast`          | `nix build .#android-sdk-libs-fast`          | Host-matching Android target only           |
| `ios-xcframeworks`               | `nix build .#ios-xcframeworks`               | Release XCFramework package (macOS only)    |
| `ios-xcframeworks-fast`          | `nix build .#ios-xcframeworks-fast`          | Simulator + host macOS only                 |
| `wasm-bindgen-cli`               | `nix build .#wasm-bindgen-cli`               | WASM bindings CLI tool                      |

## Key Files

| File                              | Purpose                                                    |
| --------------------------------- | ---------------------------------------------------------- |
| `nix/package/xmtp-sdk.nix`        | Generated SDK WASM build                                   |
| `nix/package/android.nix`         | Android build derivation (all targets + aggregate)         |
| `nix/package/ios.nix`             | iOS XCFrameworks and Swift bindings (macOS only)           |
| `nix/package/xmtp-sdk-native.nix` | Native SDK libraries; the host one is `xmtp-sdk-libs`      |
| `nix/lib/mobile-common.nix`       | Shared build args for iOS/Android                          |
| `nix/lib/filesets.nix`            | Source filtering for hermetic builds                       |

## Node Build Details

Node builds use the crane two-phase pattern (same as iOS/Android):

1. `buildDepsOnly` — compile dependencies (cached per target)
2. `buildPackage` — build the SDK shared library using cached deps

`nix/sdk-platforms.json` lists the Node platforms (package name, Rust target,
library, the Nix systems that build it, CI runners), the Android ABIs, the
Apple slices, and the glibc floor. Nix, `node-platforms.mjs`,
`record-node-platform.py`, and `build-sdk-node-platforms.yml` read it.
The host platform uses the host package set, so `xmtp-sdk-node-<host>` and
`xmtp-sdk-libs` share one library build.

Windows targets are excluded from Nix (built separately in CI).
