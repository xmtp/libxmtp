# Nix

Check an affected output with `nix build --no-link .#<output>`; run `dev/nix-shell 'just lint-config'`.

On macOS, keep the compiler, linker, and SDK from the same toolchain. The local
and iOS shells select Xcode through `ios-env.nix` and use its native linker.
Do not hard-code an Xcode version or use Nix's linker with the system SDK.
The focused Rust shell and Android host builds use the Nix toolchain.
They must replace native compiler and linker overrides from an outer local or
iOS shell. Otherwise, Xcode's linker uses Nix's SDK without its library search paths.
Run `dev/nix-shell 'bash dev/check-apple-toolchain'` in the local shell and
`NIX_DEVSHELL=rust dev/nix-shell 'bash dev/check-apple-toolchain'` for the Nix SDK.
Also check a shell transition with
`dev/nix-shell 'dev/nix-shell --shell rust "bash dev/check-apple-toolchain"'`.
Also check the Android shell transition with
`dev/nix-shell 'dev/nix-shell --shell android "bash dev/check-apple-toolchain"'`.
The check links `iconv`; a basic libc link does not detect mixed toolchains.

## Build isolation

- Cargo resolves every workspace member before applying package selection. Crane's `mkDummySrc` input needs each member's target sources, not just its manifest. Keep dependency-cache inputs narrow after dummy generation.
- Local protobuf generation needs schemas and the real proto build script in both dependency-only and final sources, plus build-platform `protoc` when cross-compiling. After changing source filters, verify that a schema edit changes the dependency derivation and an unrelated Rust source edit does not.
- Backend builds restore only the shared dependency sources over the dummy workspace. Include migrations, SQL query files, and `apps/backend/.sqlx` metadata. Add each new shared dependency to the restored crate list. Build with `SQLX_OFFLINE=true`; no database is allowed during the build.
- The Rust shell provides sqlx-cli. Keep its version aligned with the backend SQLx dependency.
- SDK compiler sources use a dummy workspace and restore the selected local
  dependency graph. Keep embedded SDK data and bindgen templates in the real
  compiler source. Provenance wrappers keep the complete source identity.

`backend-ci` packages disposable PostgreSQL, VersityGW, and the native backend.
It supports one command on an isolated macOS runner. It does not add services
to development shells. Run `dev/nix-shell 'just backend ci COMMAND'`.

## Generated SDK preparation

`xmtp-sdk-generated` includes the native, worker, and pure roots with matched
contract records. Select `xmtp-sdk-generated-swift`, `xmtp-sdk-generated-kotlin`,
or `xmtp-sdk-generated-node` to build one native language without WASM.
`xmtp-sdk-generated-browser` includes matched worker and pure roots.
Each selected product keeps the aggregate's root names. Node and Browser
include their required runtime links. Android and Apple preparation use the
selected Kotlin and Swift products. Check these build closures with
`dev/nix-shell 'just sdk check-generated-nix'`.
`xmtp-sdk-pure-wasm` is a separate artifact and shares the
worker dependency cache. The generated library outputs are
`xmtp-sdk-node-<platform>` and `xmtp-sdk-android-<abi>`. Darwin adds `xmtp-sdk-ios-device` and
`xmtp-sdk-ios-simulator`. Public SDK packaging stages these matched generated
artifacts. Building and staging do not publish a package.

Run `dev/nix-shell 'python3 -B nix/check-sdk-products.py'` to build selected
Kotlin, Android fast, and Swift on Darwin. It checks generated files, complete
receipts, source identities, and the selected Android ELF library.
Run `dev/nix-shell 'python3 -B nix/test-sdk-windows-staging.py'` to check the
Windows Node job with cached generated roots. This fixture checks cleanup and
receipt creation. It does not compile a Windows SDK.

`android-sdk-libs` combines generated Kotlin, runtime and Android sources,
the contract record, and all four `libxmtp_sdk.so` ABIs. `android-sdk-libs-fast`
selects the host emulator ABI. The SDK Gradle build reads this layout. The
mobile-stage command builds the SDK library AAR so platform helpers are included.

The Android minimum-platform CI caller sets `NIX_ANDROID_EMULATOR_API=23`.
Only Linux x86_64 includes this default x86_64 system image. Other callers use
API 34. The launcher checks the selected API and runs the existing clock sync.
Check the selector with `dev/nix-shell 'python3 nix/lib/test-android-emulator-platform.py'`.
