# Standalone migration packages

These packages use the independent `xmtp_legacy_migration` UniFFI module.
Node and Agent use `@xmtp/migration`. Browser uses `@xmtp/browser-migration`.
Swift imports `XmtpMigration`. Kotlin imports `uniffi.xmtp_migration`.

Run commands from the repository root inside `dev/nix-shell`.
Generated bindings and binaries stay under `target/migration-packages`.
Do not add the migration API to the main client facade.

The browser worker owns the legacy OPFS pool only for one export. It must end
before the destination SDK starts. Preserve exact source names, including a
leading slash. Use the shared storage pool lock name from generated config.

## Package commands

Use `just migration build` for native and browser bindings. Use
`just migration stage node` and `just migration stage browser` to stage npm packages.
Then run `just migration test-node` and `just migration test-browser`.

Use `just migration mobile-stage ios` for the host, iOS device, and iOS simulator
Swift package. `--host-only` stages only a host test package; do not publish it.
Use `just migration test-swift` to compile and run the Swift consumer.

Use `just migration mobile-stage android` for all Android ABIs. `--fast` selects
the host emulator ABI. `just migration android-build` builds the AAR. Run
`just migration test-kotlin` for the independent host JVM consumer. A JVM proof
does not prove that an Android device can load the AAR.

Use `dev/nix-shell 'just migration test-browser-import'` after building the current Browser SDK. It checks immediate import after converter cleanup.
