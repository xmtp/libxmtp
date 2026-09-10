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

Each git worktree keeps its own `target/` directory. Without a shared cache, N
worktrees compile the same dependencies N times, and `target/` grows to tens of
gigabytes per worktree.

`sccache` is in the `rust` and `default` dev shells. It is **off by default**.
Turn it on per shell:

```bash
source dev/sccache-env
cargo build
```

Every worktree keeps its own cargo lock, so builds in different worktrees still
run in parallel at full width. They share one compilation cache, so work that
one worktree already did is copied instead of recompiled.

### Trade-off: incremental compilation

sccache cannot cache incremental compilation, so `dev/sccache-env` sets
`CARGO_INCREMENTAL=0`. Choose per task:

- **Many worktrees, fresh branches, agent-driven work** — use sccache.
  Incremental has nothing to reuse in a fresh worktree.
- **Hand-iterating on one crate in one worktree** — prefer incremental.
  Do not source the script, or run:

  ```bash
  unset RUSTC_WRAPPER
  export CARGO_INCREMENTAL=1
  ```

### Cache size

The cache is bounded and evicts least-recently-used entries. Default is 60 GiB;
override with `SCCACHE_CACHE_SIZE` before sourcing.

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
