# Nix Development Setup

This guide covers setting up a Nix-based development environment for libxmtp on
macOS.

## Prerequisites

- **macOS** (aarch64-darwin is the primary supported platform)
- **Docker** — required for running the local XMTP node (`just backend up`
  starts it in Docker)

## Install Determinate Nix

The easiest way to get started is the `./dev/nix-up` script, which installs
Determinate Nix, configures the XMTP binary cache, and installs direnv
interactively:

```bash
./dev/nix-up
```

To install Determinate Nix manually:

```bash
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install --determinate
```

[Determinate Nix](https://docs.determinate.systems/) is a distribution of Nix
designed for developer and CI workflows with built-in caching support.

To fully uninstall Determinate Nix, run `/nix/nix-installer uninstall`. Note that
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
repo. Run `direnv allow` to authorize it and `direnv deny` to revoke. `.envrc`
loads the environment that `dev/nix-shell` caches, so direnv and the wrapper
share one evaluation; `dev/nix-shell --refresh true` evaluates it again.

The default shell is also used by Zed's Rust language server and default agent
commands. This keeps native compiler, linker, and SDK settings consistent. Do not
set `NIX_DEVSHELL=rust` globally. Use an explicit shell for a focused or cross-target
command. See [Zed checks](agent-tools.md#zed-checks) for check scope and migration.

## Disabling direnv

If direnv's shell integration is slowing down your terminal or you want to
temporarily stop the auto-activation, toggle it for this repository:

```bash
direnv deny    # Disable direnv for this repo
direnv allow   # Re-enable direnv for this repo
```

These do **not** uninstall anything — they just toggle whether direnv activates
when you enter the repo. This is the recommended way to pause the Nix
environment without losing your cached dependencies.

## Binary Caches

The `./dev/nix-up` script configures the XMTP binary cache via
[Cachix](https://cachix.org) so builds can pull pre-built artifacts instead of
compiling from source:

- `xmtp.cachix.org` — project-specific cache (XMTP derivations, Android NDK, etc.)

The script adds `extra-substituters` and `extra-trusted-public-keys` lines.
A trusted Nix user (`nix config show trusted-users` names you) gets them in
`~/.config/nix/nix.conf`. Any other user gets them through `sudo` in
`/etc/nix/nix.custom.conf` when it exists (Determinate Nix manages
`/etc/nix/nix.conf`), else in `/etc/nix/nix.conf`, and the script then restarts
the Nix daemon. If you installed Nix without `dev/nix-up`, run it once to
configure the cache.

## Available Dev Shells

| Shell                   | Description                                                                     |
| ----------------------- | ------------------------------------------------------------------------------- |
| `default`               | Local development: the Rust core plus editor, debug, and iOS tools              |
| `rust`                  | The Rust core: one toolchain, cargo tools, Node, and pnpm (CI Rust)             |
| `android`               | The Rust core packages plus the Android SDK, NDK, and emulator                  |
| `android-ci`            | Slim Android CI shell, no Rust (Linux); `android-ci-emulator` adds the emulator |
| `js`, `js-node`, `docs` | JavaScript and docs work, no Rust                                               |
| `ios`, `wasm`           | Old names of `default` and `rust`                                               |

Enter a shell with `dev/nix-shell --shell <name> '<command>'`; it caches the
evaluated environment.

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
  CI and local builds skip redundant work. On every push to trunk,
  `.github/workflows/post-merge.yml` builds and pins the outputs in
  `nix/ci-sets.nix`. Developers, release, and deploys read only the trusted
  cache `xmtp`; pull request and merge-run jobs also read and write the
  CI-only cache `xmtp-pr`

## Compiler cache

The Rust, local, WASM, Android, and iOS shells enable pinned Kache by default.
Every Rust compile goes through it on every platform: libraries, tests,
binaries, proc macros, and the SDK `cdylib` and `staticlib`. So do the C
compiles of build scripts, because cc-rs routes them through a `RUSTC_WRAPPER`
named `kache`. Nix derivations keep their own Nix/Crane caches. Each worktree
keeps its own Cargo target directory.

The shells set four variables (`nix/lib/shell-common.nix`): `RUSTC_WRAPPER`,
`KACHE_HOST_CONFIG`, `KACHE_BUILD_SCRIPT_CACHE=0`, and
`KACHE_KEY_ENV_VARS=CI,XMTP_TEST_LOGGING`. `KACHE_HOST_CONFIG` points at
`nix/lib/kache.toml`, which holds the preferences: the store
`~/.cache/kache`, a 50 GiB limit, executable caching on, C link caching off,
and the 7-day unit cleanup. It is Kache's lowest file layer, so your own Kache
config file (`~/.config/kache/config.toml`) and any `KACHE_*` variable win.
The two settings that keep builds correct are variables, which win over every
file. Kache expands only a leading `~` in the file, so the store no longer
follows `$XDG_CACHE_HOME`; set `KACHE_CACHE_DIR` to keep a store elsewhere.

In CI, `setup-nix` keeps kache-action's store, runtime, `KACHE_CONFIG` (its
S3 remote), and 10 GiB limit, and adds `CARGO_INCREMENTAL=0`,
`KACHE_ADAPTIVE_INCREMENTAL=0`, and `KACHE_PRESERVE_INCREMENTAL=0`. A job
that does not set `kache: "true"` gets `KACHE_DISABLED=1`, so the shells'
wrapper runs rustc directly.

Run `dev/nix-shell 'kache stats --full'` to inspect hits, misses, and
scheduler wait, and `dev/nix-shell 'kache doctor'` to see which file or
variable sets each key. To opt out, use
`KACHE_DISABLED=1 dev/nix-shell 'just check'`; add
`KACHE_PRESERVE_INCREMENTAL=1` to keep incremental compilation, which the
disabled wrapper otherwise strips. Locally, when the cache's volume has less
than 10 GiB free, builds also evict cache entries (Kache's default since
1.2.0; off in CI).

One Kache daemon serves every checkout on the machine. It keeps the settings
of the shell that started it and does not watch `nix/lib/kache.toml`; after a
change there, run `dev/nix-shell 'kache daemon restart'`.

Kache's scheduler is on. It does two things (Kache 1.2.2
`docs/how-it-works/limits.mdx`). After a local and remote miss, a compile joins
a machine-wide flight for its cache key, so a second process that misses the
same key waits and then restores the first one's entry; `kache stats --full`
reports that wait as "on a shared compile". The compile then takes a permit
from a pool of one slot per CPU per cache directory (machine-wide here, because
the worktrees share `~/.cache/kache`). A slot is the machine's memory divided by
the pool, at least 512 MiB; a measured crate takes as many slots as its peak
memory needs, an unmeasured compile one, and an unmeasured link two. Under
memory pressure (on macOS, the kernel's warning level) a compile asks for the
whole pool, so compiles run one at a time. Hits and passthroughs never wait.
The switches: `KACHE_SCHEDULER_MEMORY_PRESSURE=0` turns off only the pressure
mode; `KACHE_SCHEDULER=0` turns off the scheduler (two processes that miss
the same crate still share one compile when the first stores an entry that the
second can restore, but the shared-compile wait is no longer measured). Change
a switch only when `kache stats --full` shows permit waits while
cores are idle; bare `kache stats` shows no scheduler wait.

Locally, Kache's adaptive incremental mode (on by default) gives a workspace
crate whose sources changed isolated incremental state, also when it is a
dependency such as `crates/xmtp_mls` under an `xmtp_sdk` build. On the owner
Mac a one-line `xmtp_mls` edit rebuilt the SDK host library in about 6 s from
the second edit on (19 s for the first), against 31 s per edit with
`KACHE_ADAPTIVE_INCREMENTAL=0`, so neither `-p` lists nor
`KACHE_PRESERVE_INCREMENTAL=1` are needed. On 2026-10-15, after two edits, a
one-line `xmtp_mls` edit rebuilt `cargo check -p xmtp_mls --all-targets` in
6 s and `cargo nextest run -p xmtp_mls --no-run` in 17 s; with
`CARGO_INCREMENTAL=0` they took 25 s and 55 s.

Kache clears Cargo's shared `<profile>/incremental` after each compile it
wraps; its own compiles keep their state in `incremental.kache-auto/`. A
compile outside Kache on the same target writes `incremental/` and can lose
its state mid-build ("failed to move dependency graph"), so run editors
inside the dev shell: Zed's settings start rust-analyzer through
`dev/nix-shell`, and in VS Code install the recommended direnv extension,
which loads `.envrc` before rust-analyzer starts. Kache drops an adaptive
unit only when a cache hit replaces it, so `incremental.kache-auto/` keeps
the units of old feature sets, profiles, and toolchains (4.2 GiB after one
session on 2026-10-15). `dev/clean-incremental` removes Kache units and
crate directories that no build used for 3 days. It holds Cargo's locks
while it renames them, so it skips a profile with a running build.
`dev/nix-shell` runs it in the background for its worktree at most once a
day; `just clean-incremental [days]` runs it for every worktree. The Kache
daemon removes other build units no build used for 7 days
(`auto_clean_unused_units_days` in `nix/lib/kache.toml`; Kache's default is
30).

Kache remaps the source paths of what it compiles to `/kache/workspace`, so
a debugger needs a source map. VS Code maps it in `.vscode/settings.json`;
in command-line lldb, run
`settings set target.source-map /kache/workspace "$PWD"` from the checkout.

`KACHE_MAX_SIZE` limits the store's private blob bytes on each machine; blobs
that a target directory still holds (hardlinked or cloned) are not freed until
that target goes. `kache clean --stale 14d --dry-run`, then `--yes`, removes
target directories that no build used for 14 days, which frees their bytes.

CI cache writes are limited to trusted pushes on main. Other
contexts can restore the cache. Keep credentials out of source and derivations.
Compiler cache hits do not qualify a whole-workflow time or cost target.
