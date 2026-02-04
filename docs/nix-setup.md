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

To uninstall Nix and direnv, run `./dev/nix-down`.

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

## Enable Cachix

The flake's `nixConfig` block declares two binary caches so builds can pull
pre-built artifacts:

- `xmtp.cachix.org` — project-specific cache
- `nix-community.cachix.org` — community cache (fenix Rust toolchain, etc.)

### Accept when prompted

When you first run `nix develop`, Nix asks whether to trust the extra
substituters. Accept, or pass `--accept-flake-config`:

```bash
nix develop --accept-flake-config
```

### Permanent configuration

To avoid the prompt on every invocation, add the caches to
`~/.config/nix/nix.conf`:

```ini
extra-trusted-substituters = https://xmtp.cachix.org https://nix-community.cachix.org
extra-trusted-public-keys = xmtp.cachix.org-1:nFPFrqLQ9kjYQKiWL7gKq6llcNEeaV4iI+Ka1F+Tmq0= nix-community.cachix.org-1:mB9FSh9qf2dCimDSUo8Zy7bkq5CX+/rkCWyvRCYg3Fs=
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

# Enter a shell and accept flake config in one step
nix develop --accept-flake-config

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
