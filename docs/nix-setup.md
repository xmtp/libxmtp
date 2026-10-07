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

## Shared build cache (Kache)

Each Git worktree keeps its own `target/` directory. Kache 1.0.0 can reuse
compiler output across worktrees. It does not replace `target/` or remove old
build output. Keep each worktree's Cargo build lock and target directory.

The `rust`, `default`, `wasm`, `android`, and `ios` Nix shells enable Kache when
they start. They source `dev/kache-env` and select the pinned Kache binary as
`RUSTC_WRAPPER`. You do not need to source the helper yourself.

```bash
dev/nix-shell 'just check crate xmtp_common'
dev/nix-shell 'just cache-stats'
```

### Local incremental builds

The helper keeps local compiler flags, features, profiles, and any explicit
`CARGO_INCREMENTAL` value. Cargo uses its profile defaults when that value is
unset. The WASM test profile and `just wasm check` still disable incremental
builds to limit the size of WASM build data.

On Linux, Kache can cache executable output. On Darwin, the wrapper caches only
`rlib` and `lib` compiles. It passes binaries, dynamic libraries, proc macros,
tests, static libraries, and unknown compile forms to rustc without caching
them. Compiler arguments stay unchanged. The helper disables build-script caching and C/C++ link caching.
It adds `CI` and `XMTP_TEST_LOGGING` to the cache key because our proc macros read
those values without rustc environment dependency records.
Different source, compilers, flags, features, profiles, and keyed environment
values can produce different cache entries. Use `dev/nix-shell 'just cache-stats'`
to measure reuse. Do not assume that a cache hit proves a workspace speedup.

To start a fresh shell without Kache, set `XMTP_KACHE=0` before shell entry:

```bash
XMTP_KACHE=0 dev/nix-shell 'just check crate xmtp_common'
```

To disable the wrapper in an active shell, run `unset RUSTC_WRAPPER`. Set
`XMTP_KACHE=0` as well if later shell entries must keep it disabled. Existing
build output stays in place.

### Cache size

The helper defaults to `${XDG_CACHE_HOME:-$HOME/.cache}/kache` with a 10 GiB
limit. Set `KACHE_CACHE_DIR` or `KACHE_MAX_SIZE` before shell entry to use a
different directory or limit. The helper keeps nonempty explicit values.
This store is shared across worktrees. Its limit does not limit any worktree's
`target/` directory.

```bash
dev/nix-shell 'just cache-stats'
dev/nix-shell 'just disk'
dev/nix-shell 'just clean-incremental'
dev/nix-shell 'just clean-incremental 30'
dev/nix-shell 'just clean-incremental --minutes 30'
```

The cleanup commands remove incremental directories unused for 14 days by
default, or for the selected number of days or minutes. During a long run with
several worktrees, you can run a disk guard in a background shell:

```bash
while dev/nix-shell 'just clean-incremental --minutes 30' && df -h .; do sleep 600; done
```

### CI and releases

Direct Cargo CI jobs use the official Kache action through the default
`kache: true` input of `.github/actions/setup-nix`. The action keeps its own
store and runtime. The compiler wrapper comes from the immutable Nix Kache
package, so exact source and compiler checks can bind it. CI sets `CARGO_INCREMENTAL=0`,
`KACHE_ADAPTIVE_INCREMENTAL=0`, and `KACHE_PRESERVE_INCREMENTAL=0`. These CI
settings do not change local profile defaults.

CI can restore cached output in all contexts. It saves output only on branch
pushes to `main` or `self-hosted`. Pull requests, tags, manual runs, and other
branches cannot save output. Kache uses the GitHub Actions cache. It does not
need S3 or new secrets.

`nix build` derivations, releases, `.#validation`, and `.#nextest` use Nix and
Crane caching. They do not use the outer shell's Kache wrapper.
