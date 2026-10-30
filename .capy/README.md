# Capy environment

`.capy/setup.json` owns this repository's Capy Setup on `main`. Machine size
and snapshot activation remain project settings, not fields in this file.

- Initialize configures the public XMTP Nix cache without credentials, warms
  the default development shell, fetches locked Rust dependencies, and runs
  `just install-js` with the frozen pnpm workspace lockfile. It runs Clippy for
  every `xmtp_mls` target to prime Cargo's debug artifacts and Kache, then
  builds the library test binary and runs the backend-free
  `storage_location::tests::deployment_dirs_do_not_collide` test. Both debug
  and test artifacts stay in the snapshot.
- Update-after-checkout hashes manifests, lockfiles, package-manager settings,
  patches, runtime pins, Nix shell inputs, and the Setup files. It repeats the
  dependency install only when those inputs change or `node_modules` is missing.
- Startup runs `just backend db-up`, which waits for PostgreSQL and its replica
  to become healthy. Their test data is ephemeral. The full backend and
  observability stack are on demand through `backend-up`; `test` starts the
  full stack before running the workspace tests.

Every language command runs through `dev/nix-shell`; the pinned Nix toolchain
takes precedence over the VM's host runtimes. Setup installs upstream Nix
2.31.5, matching Linux CI, from its checksum-verified official archive. The VM's
bundled Determinate Nix handles `extra-allowed-impure-host-deps` differently
and fails the repository's cache configuration test. The interactive `dev/nix-up`
bootstrap is not used because it prompts for direnv and assumes a system Nix
daemon when configuring an untrusted user's cache. Capy's VM uses single-user
Nix, so initialize configures the same public cache in the user's Nix settings
without restarting a daemon or changing sandbox settings.

No project secrets are required. Mobile toolchains and generated SDK bindings
are built on demand with their existing Just recipes, not on every boot.

The project's nightly snapshot automation runs at 03:00 America/Vancouver.
It rebuilds the active `main` Setup and leaves the previous image current if
the build fails. It will not activate anything until this file is merged and
the first tested snapshot is enabled. The project uses the Ultra machine size
(32 GB RAM, 64 GB disk); disk size cannot be selected separately from RAM.
