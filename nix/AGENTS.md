# Nix

Read `.agents/skills/working-with-nix/SKILL.md` before changing derivations.
Check an affected output with `nix build --no-link .#<output>`; run `just lint-config`.

On macOS, keep the compiler, linker, and SDK from the same toolchain. The local
and iOS shells select Xcode through `ios-env.nix` and use its native linker.
Do not hard-code an Xcode version or use Nix's linker with the system SDK.
The focused Rust shell uses the Nix toolchain and does not need system Xcode.
It must replace native compiler and linker overrides from an outer local or iOS
shell. Otherwise, Xcode's linker uses Nix's SDK without its library search paths.
Run `dev/nix-shell 'bash dev/check-apple-toolchain'` in the local shell and
`NIX_DEVSHELL=rust dev/nix-shell 'bash dev/check-apple-toolchain'` for the Nix SDK.
Also check a shell transition with
`dev/nix-shell 'dev/nix-shell --shell rust "bash dev/check-apple-toolchain"'`.
The check links `iconv`; a basic libc link does not detect mixed toolchains.

## Build isolation

- Cargo resolves every workspace member before applying package selection. Crane's `mkDummySrc` input needs each member's target sources, not just its manifest. Keep dependency-cache inputs narrow after dummy generation.
- Local protobuf generation needs schemas and the real proto build script in both dependency-only and final sources, plus build-platform `protoc` when cross-compiling. After changing source filters, verify that a schema edit changes the dependency derivation and an unrelated Rust source edit does not.
- Backend builds restore only the shared dependency sources over the dummy workspace. Include migrations, SQL query files, and `apps/backend/.sqlx` metadata. Add each new shared dependency to the restored crate list. Build with `SQLX_OFFLINE=true`; no database is allowed during the build.
- The Rust shell provides sqlx-cli. Keep its version aligned with the backend SQLx dependency.
