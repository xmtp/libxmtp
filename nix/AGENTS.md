# Nix

Read `.claude/skills/working-with-nix/SKILL.md` before changing derivations.
Check an affected output with `nix build --no-link .#<output>`; run `just lint-config`.

## Build isolation

- Cargo resolves every workspace member before applying package selection. Crane's `mkDummySrc` input needs each member's target sources, not just its manifest. Keep dependency-cache inputs narrow after dummy generation.
- Local protobuf generation needs schemas and the real proto build script in both dependency-only and final sources, plus build-platform `protoc` when cross-compiling. After changing source filters, verify that a schema edit changes the dependency derivation and an unrelated Rust source edit does not.
- Backend builds restore only the shared dependency sources over the dummy workspace. Include migrations and `apps/backend/.sqlx` metadata. Build with `SQLX_OFFLINE=true`; no database is allowed during the build.
- The Rust shell provides sqlx-cli. Keep its version aligned with the backend SQLx dependency.
