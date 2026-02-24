---
name: working-with-nix
description: Use when working with Nix flakes, selecting devShells, debugging environment issues, or understanding Nix infrastructure - covers shell selection, environment detection, common commands, and project-specific constraints like pinned Rust versions
---

# Working with Nix in libxmtp

## Quick Shell Selection

| Task | Shell | Command |
|------|-------|---------|
| Focused Rust crates/bindings work | rust | `nix develop .#rust` |
| Full local development (default) | local | `nix develop` |
| Android builds/testing | android | `nix develop .#android` |
| iOS builds/testing | ios | `nix develop .#ios` |
| JavaScript/Node.js | js | `nix develop .#js` |
| WebAssembly builds | wasm | `nix develop .#wasm` |

## Environment Detection

```bash
echo $XMTP_NIX_ENV     # "yes" if in any Rust-based Nix shell
echo $XMTP_DEV_SHELL   # "local", "android", "ios", or unset
```

## Critical Constraints

### Rust Version is Pinned — Do Not Change

**Rust 1.92.0** via `flake.nix` → `rust-manifest`. All shells use `xmtp.mkToolchain`. Never modify without project-wide coordination.

### iOS Shell is macOS Only

The `.#ios` shell only works on Darwin. Requires Xcode 16+ for Swift 6.1.

### Untracked Files are Invisible to Flakes

```bash
git add <new-file>  # Then rebuild — flakes only see git-tracked files
```

### Cachix

Verify the binary cache is active: `nix config show | grep substituters` — should include `https://xmtp.cachix.org`.

## Shell Architecture

Shells are independent — they compose from shared building blocks, not from each other:

```
shell-common.nix   → shared building blocks (rustBase, wasmEnv, tool groups)
  ├── rust.nix     → focused Rust shell (crates/bindings, lint, test)
  ├── local.nix    → full local dev (default) = all targets + debug + misc
  ├── android.nix  → Android cross-compilation
  └── ios.nix      → iOS cross-compilation (macOS only)
js.nix             → JavaScript/browser testing (no Rust)
package/wasm.nix   → WASM shell + package build
```

## Essential Commands

```bash
# Enter shells
nix develop              # Default shell
nix develop .#rust       # Focused Rust shell
nix develop .#android    # Android shell
nix develop .#ios        # iOS shell (macOS only)
nix develop .#js         # JavaScript shell
nix develop .#wasm       # WASM shell

# Build packages
nix build .#wasm-bindings                    # WASM compiled bindings
nix build .#node-bindings-fast               # Host-matching .node binary
nix build .#node-bindings-js                 # Generated JS/TS bindings
nix build .#node-bindings-linux-x64-gnu      # Per-target .node (example)
nix build .#android-libs                     # All Android targets
nix build .#ios-libs                         # All iOS targets (macOS only)

# Explore
nix flake show           # List all outputs
nix flake metadata       # Show inputs and versions

# Debug
nix develop --show-trace  # Verbose error output
```

## Platform Support

| Platform | Shells Available |
|----------|------------------|
| macOS (aarch64-darwin) | rust, default, android, ios, js, wasm |
| Linux (x86_64-linux) | rust, default, android, js, wasm |

## Key Files

| File | Purpose |
|------|---------|
| `flake.nix` | DevShell + package definitions, input pinning, cachix |
| `nix/lib/default.nix` | Overlay wiring — exposes `xmtp.*` to all Nix files |
| `nix/lib/shell-common.nix` | Shared building blocks (rustBase, wasmEnv, tool groups) |
| `nix/lib/mkToolchain.nix` | Rust version pinning logic |
| `nix/lib/filesets.nix` | Source file filtering for hermetic builds |
| `nix/lib/mobile-common.nix` | Shared mobile (iOS/Android) build args |
| `nix/lib/android-env.nix` | Android SDK config, targets, emulator script |
| `nix/lib/ios-env.nix` | iOS targets, dynamic Xcode resolution |
| `nix/lib/node-env.nix` | Node targets, NAPI name mapping, cross-compilation |
| `nix/shells/rust.nix` | Focused Rust dev shell |
| `nix/shells/local.nix` | Full local dev shell (default) |
| `nix/shells/android.nix` | Android dev shell |
| `nix/shells/ios.nix` | iOS dev shell (macOS only) |
| `nix/js.nix` | JavaScript shell |
| `nix/package/wasm.nix` | WASM shell + package build |
| `nix/package/node.nix` | Node.js per-target builds + JS/TS generation |
| `nix/package/android.nix` | Android release build derivation |
| `nix/package/ios.nix` | iOS release build derivation |
| `dev/nix-up` | Installation script for Nix and direnv |

## Further Reference

- [Shell: Default](shells/default.md) — Full local dev shell reference
- [Shell: Rust](shells/rust.md) — Focused Rust shell reference
- [Shell: Android](shells/android.md) — Android shell reference
- [Shell: iOS](shells/ios.md) — iOS shell reference
- [Shell: JavaScript](shells/js.md) — JavaScript shell reference
- [Shell: WASM](shells/wasm.md) — WASM shell reference
- [Nix Build Packages](packages.md) — All `nix build .#<package>` outputs
- [Troubleshooting](troubleshooting.md) — Common issues and solutions
