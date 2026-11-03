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

The script installs single-user Nix if missing, enables flakes, configures the
public `xmtp.cachix.org` cache and signing key, and downloads/builds the `default`,
`rust`, and `js` shell closures. `dev/nix-shell` keeps them GC-rooted. The resulting
snapshot includes `/nix/store`, Nix configuration, and cached shell environments,
so new sessions do not repeat the installation or shell downloads. No Cachix
token or runtime secret is needed. Rerunning the script reuses the store and does
not duplicate configuration. Use the same OS user for setup and sessions.

Wait for the build to become **Ready**, then launch a new instance from it. Verify:

```bash
dev/nix-shell 'rustc --version; node --version; just --version'
dev/nix-shell 'just check crate xmtp_common'
```

Always use `dev/nix-shell` for repository commands. It finds Nix even in non-login
shells. A new worktree can regenerate the small environment cache from the store
without downloading unchanged shell dependencies.

This preloads development tools, not Cargo build artifacts, pnpm dependencies,
Android SDKs, or every artifact published to Cachix. Changed Nix inputs and other
shells can still require downloads. Rebuild the Environment after changes to
`flake.lock`, shell definitions, or this setup script; existing instances do not
receive snapshot updates.

Backend tests still require a working Docker daemon and
`dev/nix-shell 'just backend up'` in each running instance. Setup does not start
services: processes are not a reliable part of a filesystem snapshot. iOS work
requires macOS and Xcode and cannot run in this Linux environment.
