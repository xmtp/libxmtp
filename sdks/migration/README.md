# Standalone migration packages

These packages expose `prepareMigrationArchive` independently from the current SDK. They contain stock generated UniFFI bindings plus the platform adapter needed to load them.

| Platform       | Package                   | Key type     | Count type |
| -------------- | ------------------------- | ------------ | ---------- |
| Node and Agent | `@xmtp/migration`         | `Uint8Array` | `bigint`   |
| Browser        | `@xmtp/browser-migration` | `Uint8Array` | `bigint`   |
| Swift          | `XmtpMigration`           | `Data`       | `UInt64`   |
| Android        | `org.xmtp:migration`      | `ByteArray`  | `ULong`    |

All commands run from the repository root through Nix:

```sh
dev/nix-shell 'just migration build all'
dev/nix-shell 'just migration stage node'
dev/nix-shell 'just migration stage browser'
dev/nix-shell 'just migration test-node'
dev/nix-shell 'just migration test-browser'
dev/nix-shell 'just migration mobile-stage ios'
dev/nix-shell 'just migration mobile-stage android'
dev/nix-shell 'just migration android-build'
dev/nix-shell 'just migration test-swift'
dev/nix-shell 'just migration test-kotlin'
```

Generated bindings are under `target/migration-generated`. Staged packages are under `target/migration-packages`. JavaScript package staging also links each source package's `dist` to its staged output for workspace type checks. Build the native and WASM libraries before running package tasks or documentation checks. Generated files are not committed.

Node staging copies the native library for the build host. Build and stage on each supported release host before publishing that host's package. It does not claim that one host binary works on other operating systems or architectures.

Swift staging builds the macOS host, iOS device, and iOS simulator libraries, then creates an XCFramework. `--host-only` stages only the macOS library for local binding checks; it is not an iOS release. Android staging builds all four JNI ABIs; `--fast` stages only the host-selected emulator ABI for local checks.

The browser requires a secure origin, OPFS, module workers, and Web Locks. Close all clients that use the SDK storage pool. Use the exact legacy database name. The returned promise includes worker cleanup. Call `readMigrationArchive(report.archivePath)` and pass its bytes to the existing SDK `importFromBytes` operation. Open the destination client after conversion finishes.

The conformance programs call the real generated packages with legacy fixtures. Node, Swift, and Kotlin use the encrypted WAL fixture. Chromium uses a real OPFS source, checks nanosecond precision and source preservation, and injects an OPFS write failure to check output preservation.
