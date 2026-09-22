# Nix Development Setup

This guide covers setting up a Nix-based development environment for libxmtp on
macOS.

## Prerequisites

- **macOS** (aarch64-darwin is the primary supported platform)
- **Docker** — required for running the local XMTP node (`dev/up` starts it in
  Docker)

## Install Determinate Nix

The easiest way to get started is the `./dev/nix-up` script, which installs
Determinate Nix and direnv interactively:

```bash
./dev/nix-up
```

To install Determinate Nix manually:

```bash
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install --determinate
```

[Determinate Nix](https://docs.determinate.systems/) is a distribution of Nix
designed for developer and CI workflows with built-in caching support.

To fully uninstall Nix and direnv, run `./dev/nix-down`. Note that
re-installing requires re-downloading all dependencies (5+ minutes depending on
connection speed). If you just want to temporarily disable direnv, see
[Disabling direnv](#disabling-direnv) below.

## Install direnv

The `./dev/nix-up` script offers to install direnv for you. If you prefer to
install it manually:

```bash
# macOS
brew install direnv
```

Then add the shell hook to your shell config:

```bash
# ~/.zshrc
eval "$(direnv hook zsh)"

# ~/.bashrc
eval "$(direnv hook bash)"
```

direnv automatically loads the default Nix dev shell when you `cd` into the
repo. Run `direnv allow` to authorize it and `direnv deny` to revoke.

The default shell is also used by Zed's Rust language server and default agent
commands. This keeps native compiler, linker, and SDK settings consistent. Do not
set `NIX_DEVSHELL=rust` globally. Use an explicit shell for a focused or cross-target
command. See [Zed checks](agent-tools.md#zed-checks) for check scope and migration.

## Disabling direnv

If direnv's shell integration is slowing down your terminal or you want to
temporarily stop the auto-activation, use the lightweight toggle scripts:

```bash
dev/direnv-down   # Disable direnv for this repo (runs direnv deny)
dev/direnv-up     # Re-enable direnv for this repo (runs direnv allow)
```

These do **not** uninstall anything — they just toggle whether direnv activates
when you enter the repo. This is the recommended way to pause the Nix
environment without losing your cached dependencies.

## Binary Caches

The `./dev/nix-up` script configures the XMTP binary cache via
[Cachix](https://cachix.org) so builds can pull pre-built artifacts instead of
compiling from source:

- `xmtp.cachix.org` — project-specific cache (XMTP derivations, Android NDK, etc.)

The script runs `cachix use xmtp` (via `nix run nixpkgs#cachix`), which
automatically chooses the right Nix config approach based on whether you are a
trusted user. If you installed Nix manually (without `dev/nix-up`), you can
configure the cache yourself:

```bash
nix run nixpkgs#cachix -- use xmtp
```

If you are not a trusted Nix user, you may need sudo:

```bash
sudo nix run nixpkgs#cachix -- use xmtp
```

## Available Dev Shells

| Shell     | Command                 | Description                                  |
| --------- | ----------------------- | -------------------------------------------- |
| `default` | `nix develop`           | General Rust development for libxmtp         |
| `android` | `nix develop .#android` | Android cross-compilation (NDK, cargo-ndk)   |
| `ios`     | `nix develop .#ios`     | iOS/Swift builds (macOS only)                |
| `js`      | `nix develop .#js`      | Node.js bindings development                 |
| `wasm`    | `nix develop .#wasm`    | WebAssembly builds (wasm-pack, wasm-bindgen) |

## Common Commands

```bash
# Enter the default dev shell
nix develop

# Enter a specific dev shell
nix develop .#android

# Let direnv manage the shell automatically
direnv allow

# Show available flake outputs
nix flake show
```

## How Nix is Used in This Repo

- **Reproducible Rust toolchain** — Rust 1.94.0 is pinned via
  [fenix](https://github.com/nix-community/fenix), ensuring every developer and
  CI runner uses the exact same compiler
- **Platform-specific cross-compilation** — dedicated dev shells provide
  pre-configured environments for Android (NDK), iOS (Xcode toolchain), and
  WebAssembly (wasm-pack/wasm-bindgen)
- **CI caching** — [Cachix](https://cachix.org) stores built Nix derivations so
  CI and local builds skip redundant work
- **Omnix for CI orchestration** — the `.envrc` integrates with
  [omnix](https://omnix.page) for CI workflow management

## Shared Build Cache (sccache)

Each git worktree keeps its own `target/` directory. A shared compilation cache
can reuse eligible dependency builds across worktrees. It does not replace
`target/` or remove old build artifacts.

`sccache` is in the `rust` and `default` dev shells. It is **off by default**.
Turn it on inside each Nix shell that needs it:

```bash
source dev/sccache-env
dev/nix-shell 'dev/agent-run cargo build -p xmtp_common'
```

The helper selects `sccache` from `PATH` as `RUSTC_WRAPPER`. Each worktree keeps
its own Cargo build lock. Do not point several worktrees at one mutable target
directory. The helper does not change compiler flags, features, or profiles.

### Local incremental builds and cache reuse

The helper unsets `CARGO_INCREMENTAL`, including an inherited `0` or `1`.
Cargo then uses the profile defaults: local dev crates use incremental builds,
and the repository disables incremental builds for non-local dependencies.
sccache passes incremental builds through without caching them. Do not export
`CARGO_INCREMENTAL=1` while this wrapper is active: sccache 0.16 rejects that
explicit override before compilation.

An isolated two-worktree test with sccache 0.16 reused the registry dependency
and passed local incremental builds through. This proves cache eligibility,
not a workspace speedup. A separate non-incremental test still missed the cache
for local crates at different worktree paths. `SCCACHE_BASEDIRS` did not remove
those misses. Different toolchains, features, profiles, and environment values
can also split cache entries. In particular, different `CARGO_TARGET_DIR` values
prevented reuse in the test. Keep the normal per-worktree `target/` path.

Linked binaries, proc macros, and many check-only calls cannot be cached.
See the [sccache Rust limits](https://github.com/mozilla/sccache/blob/v0.16.0/docs/Rust.md).
Use `just cache-stats` to measure actual reuse. To disable the helper in this shell:

```bash
unset RUSTC_WRAPPER
```

This does not restore a previous wrapper or incremental override. If you need
those settings, restore them explicitly. Existing build artifacts are not deleted.

### Cache size

The local cache evicts least-recently-used entries. The helper requests a 10 GiB
cap in `~/.cache/sccache`. Set `SCCACHE_CACHE_SIZE` or `SCCACHE_DIR` before
sourcing to use a different cap or location. Nonempty explicit values are kept.

A running sccache server keeps the settings from its startup. Sourcing the helper
does not restart it or reduce an existing 60 GiB cap. Check the actual cache
location and maximum size with `just cache-stats`. To apply changed settings,
wait until all builds that use that server have stopped, then run
`sccache --stop-server`. The next build starts a server with the new settings.
Do not stop a server that other worktrees are using. Cache storage is additional
disk use; the cap does not limit any worktree's `target/` directory.

```bash
just cache-stats            # hit rates and current size
just clean-incremental      # delete incremental/ dirs unused 14+ days
just clean-incremental 30   # ...or a different age
```

### CI and releases

`dev/sccache-env` is never sourced automatically and is for local use only.

- **Nix builds** (`nix build`, releases, `.#validation`, `.#nextest`) run in a
  sandbox and never see `RUSTC_WRAPPER`. They are unaffected.
- **CI** sets its own sccache through the `sccache` input of
  `.github/actions/setup-nix`, backed by the GitHub Actions cache. Workflows
  that build inside `nix build` set `sccache: "false"` on purpose.
