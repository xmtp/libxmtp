---
name: working-with-nix
description: Use when working with Nix flakes, selecting devShells, debugging environment issues, or understanding Nix infrastructure - covers shell selection, environment detection, common commands, and project-specific constraints like pinned Rust versions
---

# Working with Nix in libxmtp

## Quick Shell Selection

| Task                                   | Shell                 | Entry                                 |
| -------------------------------------- | --------------------- | ------------------------------------- |
| Local development, agents, iOS         | default (`ios` alias) | `dev/nix-shell '...'`                 |
| CI Rust jobs, focused Rust work        | rust (`wasm` alias)   | `dev/nix-shell --shell rust '...'`    |
| Local Android work                     | android               | `dev/nix-shell --shell android '...'` |
| Android CI build jobs, Android release | android-ci (Linux)    | `NIX_DEVSHELL: android-ci`            |
| Android CI emulator jobs               | android-ci-emulator   | `NIX_DEVSHELL: android-ci-emulator`   |
| JavaScript and docs CI                 | js, js-node, docs     | `dev/nix-shell --shell js '...'`      |

## Environment Detection

For agent commands, use `dev/nix-shell 'just <recipe>'`. The project Codex hook
adds this wrapper when trusted. The wrapper caches each evaluated shell
environment by the files that the shells read, so a source edit needs no new
evaluation; `dev/nix-shell --refresh ...` evaluates again. Nested calls reuse
the same environment only when the worktree, shell, and key match. Do not use
the variables below as proof that a requested shell is current. See
`docs/agent-tools.md`.

```bash
echo $XMTP_NIX_ENV     # "yes" if in any Rust-based Nix shell
echo $XMTP_DEV_SHELL   # rust, local, android, android-ci, or android-ci-emulator
```

## Critical Constraints

### Rust Version is Pinned — Do Not Change

Rust is pinned via `flake.nix` → `rust-manifest` (1.97.1 as of 2026-07; check with `nix develop .#rust -c rustc --version`). All shells use `xmtp.mkToolchain`. Never modify without project-wide coordination.

### Playwright Versions Must Match EXACTLY

The `playwright` pin in `package.json` (`=X.Y.Z`) must equal nixpkgs' playwright version. The js shell provides browsers via `PLAYWRIGHT_BROWSERS_PATH` (`nix/js.nix`), and npm playwright looks them up by its own per-version browser revisions — any skew fails WASM tests with `browserType.launch: Executable doesn't exist at .../chromium_headless_shell-NNNN/...`.

After every nixpkgs (flake.lock) bump:

```bash
nix eval --raw .#devShells.x86_64-linux.js.PLAYWRIGHT_VERSION
# If it differs from the package.json pin: update the pin, then
# pnpm install
```

### iOS Shell is macOS Only

The `.#ios` shell only works on Darwin. Requires Xcode 16+ for Swift 6.1.

### Untracked Files are Invisible to Flakes

```bash
git add <new-file>  # Then rebuild — flakes only see git-tracked files
```

### Cachix

Verify the binary cache is active: `nix config show | grep substituters` — should include `https://xmtp.cachix.org`.

## Shell Architecture

`rust` is the core: one Rust toolchain (host, wasm32, the Android ABIs, and
on Darwin the Apple slices), the cargo, proto, and lint tools, Node, pnpm,
Python 3.11, and `just`. `default` layers on it (`inputsFrom`) with editor,
debug, and Swift tools and JDK 21; `android` takes its core packages and adds
the Android SDK, NDK, emulator, JDK 17, Kotlin, and ktlint. All three merge
`shellCommon.rustShellEnv`, so a switch between them changes no compiler
variable, and on Darwin all three use Xcode clang with the 11.0 macOS floor.

```text
shell-common.nix   → rustToolchain, rustCorePackages, rustShellEnv, rustShellHook
  ├── rust.nix       → the core (also `wasm`)
  ├── local.nix      → default = rust + local tools (also `ios`)
  ├── android.nix    → core packages + Android SDK and NDK
  └── android-ci.nix → slim CI shells: SDK, NDK, JDK 21; no Rust (Linux only)
js.nix, js-node.nix, docs.nix → JavaScript and docs (no Rust)
```

## Essential Commands

```bash
# Enter shells (the wrapper caches each evaluated environment)
dev/nix-shell 'just check'                 # default shell
dev/nix-shell --shell rust 'cargo check'   # Rust core
dev/nix-shell --shell android 'just android check'
dev/nix-shell --refresh true               # evaluate again

# Build packages
nix build .#xmtp-sdk-wasm                    # WASM compiled bindings
nix build .#xmtp-sdk-node-darwin-arm64               # Host-matching .node binary
nix build .#xmtp-sdk-generated                 # Generated JS/TS bindings
nix build .#xmtp-sdk-node-linux-x64-gnu      # Per-target .node (example)
nix build .#android-sdk-libs                     # All Android targets
nix build .#ios-xcframeworks                 # iOS release XCFramework (macOS only)

# Explore
nix flake show           # List all outputs
nix flake metadata       # Show inputs and versions

# Debug
nix develop --show-trace  # Verbose error output
```

## Platform Support

| Platform               | Shells Available                                                                 |
| ---------------------- | -------------------------------------------------------------------------------- |
| macOS (aarch64-darwin) | rust, default, android, ios, js, js-node, docs, wasm                             |
| Linux (x86_64-linux)   | rust, default, android, android-ci, android-ci-emulator, js, js-node, docs, wasm |

## Key Files

| File                              | Purpose                                                        |
| --------------------------------- | -------------------------------------------------------------- |
| `flake.nix`                       | DevShell + package definitions, input pinning, cachix          |
| `nix/lib/default.nix`             | Overlay wiring — exposes `xmtp.*` to all Nix files             |
| `nix/lib/shell-common.nix`        | Shared building blocks (rustBase, wasmEnv, tool groups)        |
| `nix/lib/mkToolchain.nix`         | Rust version pinning logic                                     |
| `nix/lib/filesets.nix`            | Source file filtering for hermetic builds                      |
| `nix/lib/mobile-common.nix`       | Shared mobile (iOS/Android) build args                         |
| `nix/lib/android-env.nix`         | Android SDK config, targets, emulator script                   |
| `nix/lib/ios-env.nix`             | iOS targets, dynamic Xcode resolution                          |
| `nix/lib/node-env.nix`            | Node targets, NAPI name mapping, cross-compilation             |
| `nix/lib/packages/pnpm.nix`       | Workspace pnpm package; version comes from root `package.json` |
| `nix/shells/rust.nix`             | Rust core shell (also `wasm`)                                  |
| `nix/shells/local.nix`            | Local dev shell (`default`, also `ios`)                        |
| `nix/shells/android.nix`          | Local Android shell                                            |
| `nix/shells/android-ci.nix`       | Slim Android CI shells (Linux)                                 |
| `nix/js.nix`                      | JavaScript shell                                               |
| `nix/package/node.nix`            | Node.js per-target builds + JS/TS generation                   |
| `nix/package/xmtp-sdk-native.nix` | Native SDK release build derivation                            |
| `nix/package/ios.nix`             | iOS release build derivation                                   |
| `dev/nix-up`                      | Installation script for Nix and direnv                         |
| `.node-version`, `.nvmrc`         | Node.js version for tools outside Nix                          |

## Devcontainer Image Updates

The devcontainer Dockerfile uses the NodeSource major-version setup script.
CI does not build or publish the devcontainer image. Use
`xmtp-release set-devcontainer-image --image <image>` to update the pinned
image in `.devcontainer/devcontainer.json` after a separately approved image
release. Do not publish an image from a local version update.

## Further Reference

- [Shell: Default](shells/default.md) — Full local dev shell reference
- [Shell: Rust](shells/rust.md) — Focused Rust shell reference
- [Shell: Android](shells/android.md) — Android shells reference
- [Shell: JavaScript](shells/js.md) — JavaScript shell reference
- [Nix Build Packages](packages.md) — All `nix build .#<package>` outputs
- [Troubleshooting](troubleshooting.md) — Common issues and solutions
