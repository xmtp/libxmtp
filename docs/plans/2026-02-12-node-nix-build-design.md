# Node Bindings Nix Build Migration

## Summary

Migrate the Node SDK release build from per-platform CI runners with `napi build` to Nix derivations using crane's two-phase caching, following the existing `mkIos`/`mkAndroid` patterns.

## Goals

- Per-target Nix derivations for all non-Windows targets
- Cross-compilation from minimal host architectures (1 Linux + 1 macOS runner)
- Parallel CI builds via matrix strategy on individual targets
- Separate JS/TS generation derivation
- Incremental testability at each phase

## Targets

Six Nix-managed targets:

| Rust triple | NAPI platform name | Build host |
|---|---|---|
| `x86_64-unknown-linux-gnu` | `linux-x64-gnu` | x86_64-linux (native) |
| `x86_64-unknown-linux-musl` | `linux-x64-musl` | x86_64-linux (cross) |
| `aarch64-unknown-linux-gnu` | `linux-arm64-gnu` | x86_64-linux (cross) |
| `aarch64-unknown-linux-musl` | `linux-arm64-musl` | x86_64-linux (cross) |
| `x86_64-apple-darwin` | `darwin-x64` | aarch64-darwin (cross) |
| `aarch64-apple-darwin` | `darwin-arm64` | aarch64-darwin (native) |

One non-Nix target (unchanged):

| Rust triple | NAPI platform name | Build host |
|---|---|---|
| `x86_64-pc-windows-msvc` | `win32-x64-msvc` | Windows runner |

## Architecture

### Per-target derivations

Each target follows crane's two-phase build:

1. **`buildDepsOnly`** — narrow fileset (Cargo.toml/lock/build.rs), cached per-target
2. **`buildPackage`** — full source via `forCrate ./bindings/node`, uses cached deps

The `installPhase` copies the compiled shared library and renames it:
```
target/<triple>/release/libbindings_node.{so,dylib}
  → $out/bindings_node.<napi-platform>.node
```

A `.node` file is just a renamed shared library. Node.js loads it via `process.dlopen()`.

### JS/TS generation derivation

A separate derivation that builds `bindings_node` for the native host and runs the `napi` CLI to extract platform-independent files:

- `index.js` — runtime platform-detecting loader (~200 lines)
- `index.d.ts` — TypeScript definitions auto-generated from `#[napi]` macros

The `.node` file from this build is discarded. Only the JS/TS files are kept.

### No aggregate derivation

Since the full set of targets spans two host architectures (Linux + macOS), no single-machine aggregate is possible. Individual targets are the primary outputs. Assembly happens in CI.

## Flake packages

```
node-bindings-linux-x64-gnu      → .node file
node-bindings-linux-x64-musl     → .node file
node-bindings-linux-arm64-gnu    → .node file
node-bindings-linux-arm64-musl   → .node file
node-bindings-darwin-x64         → .node file
node-bindings-darwin-arm64       → .node file
node-bindings-js                 → index.js + index.d.ts
node-bindings-fast               → host-matching target only (local dev)
```

## File structure

**New files:**
- `nix/package/node.nix` — `mkNode` function, per-target derivations, JS/TS derivation
- `nix/lib/node-env.nix` — target list, triple-to-napi mapping, cross-compilation env helpers

**Modified files:**
- `flake.nix` — import node.nix, expose packages
- `.cargo/config.toml` — musl cdylib rustflags (`-C target-feature=-crt-static`)
- `nix/lib/default.nix` — register `nodeEnv` in the xmtp overlay

### `mkNode` function

```
mkNode(targetList) → {
  targets.<triple>    # per-target .node derivation
  jsBindings          # index.js + index.d.ts derivation
}
```

## Cross-compilation

**Linux gnu (from x86_64-linux):**
- `x86_64-unknown-linux-gnu` — native, no cross toolchain
- `aarch64-unknown-linux-gnu` — nixpkgs `pkgsCross.aarch64-multiplatform` for linker + CC

**Linux musl (from x86_64-linux):**
- `x86_64-unknown-linux-musl` — nixpkgs `pkgsCross.musl64` for CC + linker
- `aarch64-unknown-linux-musl` — nixpkgs `pkgsCross.aarch64-multiplatform-musl`

**macOS (from aarch64-darwin):**
- `aarch64-apple-darwin` — native, no cross toolchain
- `x86_64-apple-darwin` — Rust handles natively (same SDK, different arch)

**Vendored C dependencies** (openssl, sqlite, zstd) need correct CC per target. Same approach as `mobile-common.nix`.

### Musl cdylib workaround

Musl targets have `crt_static_default: true` and `crt_static_allows_dylibs: false` in their Rust target spec. This means cargo rejects `cdylib` crate types when `crt-static` is enabled (the default). The fix is `-C target-feature=-crt-static` in `.cargo/config.toml`, which disables static CRT linking and enables dynamic linking for the `.node` shared libraries. The resulting binaries dynamically link musl libc, which is correct for musl environments (Alpine Linux, etc.).

Note: `CARGO_BUILD_RUSTFLAGS` env var does **not** work for this because `.cargo/config.toml` uses `target."cfg(all())"` rustflags (for `tracing_unstable`), and target-level config takes precedence over build-level config. The fix must be in `.cargo/config.toml` as `target.<musl-triple>.rustflags`.

### JS/TS generation and `__noChroot`

The `jsBindings` derivation uses `__noChroot = true` to allow `yarn install` network access (yarn Berry needs network for dependency resolution even with an offline cache). This attribute is only honored on macOS — Linux enforces `sandbox = true`. Therefore the JS/TS build **must run on a macOS runner** in CI.

The derivation also requires `NODE_EXTRA_CA_CERTS` and `SSL_CERT_FILE` pointing to Nix's `cacert` bundle, since Node.js doesn't use the system certificate store in the Nix build environment.

## CI workflow

```
build-linux (matrix: 4 targets)
  → nix build .#node-bindings-<target>
  → upload .node artifact

build-macos (matrix: 2 targets)
  → nix build .#node-bindings-<target>
  → upload .node artifact

build-js
  → nix build .#node-bindings-js
  → upload index.js + index.d.ts artifact

build-windows (unchanged, non-Nix)
  → yarn build:release --target x86_64-pc-windows-msvc
  → upload .node artifact

publish (needs all above)
  → download all artifacts into dist/
  → set version in package.json
  → npm publish --provenance
```

## What gets removed

**From package.json scripts:**
- `build` (composite pipeline) — replaced by Nix derivations for releases

**Kept for local dev/test:** `build:release`, `build:debug`, `build:test`, `build:clean`, `build:finish`, `test`, `lint`, `format`

**From CI workflow:**
- `setup-rust` action
- `setup-node` action (in build jobs; still needed in publish)
- Cross-toolchain apt installs (`crossbuild-essential-arm64`, `musl-tools`)
- Cargo config for cross-linkers
- `yarn` + `napi build` in build jobs

## Implementation phases

1. **Native host build** — `node-bindings-linux-x64-gnu` on Linux. Validates crane + napi crate, `.so` → `.node` rename, filesets.
2. **JS/TS generation** — `node-bindings-js`. Validates napi CLI in Nix.
3. **Linux cross-compilation** — `linux-arm64-gnu`, then musl variants. Validates cross toolchains + vendored deps.
4. **macOS builds** — `darwin-arm64` (native), then `darwin-x64` (cross).
5. **CI workflow** — Replace `release-node.yml`. Windows stays as-is.
6. **Cleanup** — Remove unused package.json scripts, CI toolchain setup.

Each phase is independently testable and committable.
