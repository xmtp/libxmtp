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

| Shell     | Command                | Description                                       |
| --------- | ---------------------- | ------------------------------------------------- |
| `default` | `nix develop`          | General Rust development for libxmtp               |
| `android` | `nix develop .#android`| Android cross-compilation (NDK, cargo-ndk)         |
| `ios`     | `nix develop .#ios`    | iOS/Swift builds (macOS only)                      |
| `js`      | `nix develop .#js`     | Node.js bindings development                       |
| `wasm`    | `nix develop .#wasm`   | WebAssembly builds (wasm-pack, wasm-bindgen)       |

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

- **Reproducible Rust toolchain** — Rust 1.92.0 is pinned via
  [fenix](https://github.com/nix-community/fenix), ensuring every developer and
  CI runner uses the exact same compiler
- **Platform-specific cross-compilation** — dedicated dev shells provide
  pre-configured environments for Android (NDK), iOS (Xcode toolchain), and
  WebAssembly (wasm-pack/wasm-bindgen)
- **CI caching** — [Cachix](https://cachix.org) stores built Nix derivations so
  CI and local builds skip redundant work
- **Omnix for CI orchestration** — the `.envrc` integrates with
  [omnix](https://omnix.page) for CI workflow management
