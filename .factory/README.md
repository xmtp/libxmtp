# Droid cloud environment

Use a [Factory Environment](https://docs.factory.com/environments/create), not
the deprecated Cloud Templates. In Settings → Environments, create an Environment
with this repository and a Linux source (Ubuntu with root or passwordless sudo).
Set this repository's **repository setup script** to:

```bash
bash .factory/setup.sh
```

Leave the global setup script empty. It runs from the home directory, whereas
the repository setup script runs from this checkout. The script must be committed
and available in the revision cloned by the Environment builder.

The script installs daemon-backed Nix with dedicated build users, enables flakes,
keeps build sandboxing enabled without an unsandboxed fallback, configures the
public `xmtp.cachix.org` cache and signing key, and downloads/builds the `default`,
`rust`, and `js` shell closures. `dev/nix-shell` keeps them GC-rooted. It then runs
`dev/nix-shell 'just check'` (`cargo check --locked` for the default workspace
members), warming Cargo's dependency downloads, the checkout's `target` directory,
and the compiler cache. A failed check fails the Environment build.
The resulting snapshot includes `/nix/store`, Nix configuration, cached shell
environments, and Cargo caches, so new sessions can reuse the prepared tools and
build state. No Cachix token or runtime secret is needed. Rerunning the script
reuses the store and does not duplicate configuration. Use the same OS user for
setup and sessions.

The root-owned daemon startup helper is installed at
`/usr/local/sbin/libxmtp-nix-daemon`. Setup starts it, and `dev/nix-shell` ensures
it is running after snapshot restoration before connecting with `NIX_REMOTE=daemon`.
This works without systemd and requires root or passwordless sudo in new sessions
as well as during setup. The daemon receives only system configuration, not the
session's environment or user Nix configuration. The Linux source must permit
Nix's build sandbox namespaces; unsupported kernels fail rather than silently
disable sandboxing. Rebuild old single-user snapshots from a fresh Linux source.

Wait for the build to become **Ready**, then launch a new instance from it. Verify:

```bash
dev/nix-shell 'rustc --version; node --version; just --version'
dev/nix-shell 'just check crate xmtp_common'
```

Always use `dev/nix-shell` for repository commands. It finds Nix even in non-login
shells. A new worktree can regenerate the small environment cache from the store
without downloading unchanged shell dependencies.

This preloads development tools and Cargo check artifacts, not test/release
artifacts, pnpm dependencies, Android SDKs, or every artifact published to Cachix.
New worktrees have separate target directories; they share Cargo downloads and
the compiler cache, not the original checkout's target directory. Changed source,
features, targets, or Nix inputs may still require compilation or downloads.
Rebuild the Environment after changes to `Cargo.lock`, `flake.lock`, shell
definitions, or this setup script; existing instances do not receive snapshot
updates. The first build takes longer because it checks the Rust workspace.

Backend tests still require a working Docker daemon and
`dev/nix-shell 'just backend up'` in each running instance. Setup does not start
backend services: processes are not a reliable part of a filesystem snapshot.
iOS work requires macOS and Xcode and cannot run in this Linux environment.
