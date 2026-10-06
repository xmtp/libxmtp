---
layout: "@astrojs/starlight/components/StarlightPage.astro"
title: Migrate legacy SDK data
description: Draft guide for moving legacy XMTP SDK history and consent to the self-hosted SDK through a local archive.
sidebar:
  hidden: true
pagefind: false
prev: false
next: false
head:
  - tag: meta
    attrs:
      name: robots
      content: noindex
---

:::caution[Draft API]
The migration package is not implemented. This page describes the intended workflow.
Migration API names and package names are proposed. TypeScript design examples are
shown as text until the package can supply real types. The existing archive import
examples use checked SDK source.
:::

Use `prepareMigrationArchive(args)` to create a local archive from a database made by the last legacy major version of your SDK. You do not need to install intervening SDK releases. The package applies the required legacy database migrations.

The archive contains eligible groups, DMs, application messages, and consent. Message IDs and content bytes stay unchanged. Disappearing messages are always excluded. There is no option to include them.

The archive does not contain installation keys, MLS secrets, or attachment files. Setup and group activation use the existing self-hosted SDK workflows. Imported conversations are history; importing them does not grant authority to send or decrypt later group traffic.

## 1. Close the legacy SDK and prepare the files

Close the legacy client and its database connection before calling the migration function. The function fails if it cannot get the required database access. A SQLite lock check cannot prove that an idle client is closed; closing the client remains the caller's responsibility.

Keep the database and its sidecars together, including the SQLCipher salt file if present. The function includes committed WAL data when it prepares the working copy.

Provide the database path, its encryption key when required, a new 32-byte archive key, and the output path. Store the archive key in your app's key storage before export. You need it again for import.

The proposed implementation migrates a temporary copy and leaves the source unchanged. This is a safety default, not a public option. The final choice between a copy and in-place migration remains open.

## 2. Create the archive

The proposed standalone packages expose `prepareMigrationArchive(args)`. The call owns source access, the working copy, legacy migrations, SQL extraction, metadata decoding, and the archive write. No existing SDK client or network connection is required.

The Rust interface uses owned, UniFFI-compatible values:

```rust
#[derive(uniffi::Record)]
pub struct PrepareMigrationArchiveArgs {
    pub database_path: String,
    pub database_key: Option<Vec<u8>>,
    pub archive_key: Vec<u8>,
    pub output_path: String,
}

#[derive(uniffi::Record)]
pub struct MigrationReport {
    pub archive_path: String,
    pub group_count: u64,
    pub message_count: u64,
    pub consent_count: u64,
}
```

The proposed exported signature is below. The body is omitted.

```rust
#[uniffi::export]
pub async fn prepare_migration_archive(
    args: PrepareMigrationArchiveArgs,
) -> Result<MigrationReport, MigrationError>;
```

Bindings expose this as `prepareMigrationArchive(args)`. The Rust function runs blocking database and file work off the async worker. `MigrationError` is a public UniFFI error with stable variants for invalid input, a busy source, an unsupported schema, migration failure, record-read failure, and output failure. Metadata decode failures use the fallback below.

The function always exports both standard categories without a time window and always excludes disappearing messages. It has no selection, in-place, schema-version, or resume option. `database_key: None` means the source is unencrypted. Sidecars use the legacy naming rules.

**Node and agent SDK example.** `@xmtp/migration` is a proposed standalone package name. The app supplies `databaseKey` as `Uint8Array | undefined` and `archiveKey` as a 32-byte `Uint8Array`.

```text
import { prepareMigrationArchive } from "@xmtp/migration";

const archive = await prepareMigrationArchive({
  databasePath: "/data/legacy/xmtp.db3",
  databaseKey,
  archiveKey,
  outputPath: "/data/migration/history.xmtp",
});
```

**Swift example.** Import the proposed generated migration module. Paths and keys come from the app.

```swift
let archive = try await prepareMigrationArchive(
    args: PrepareMigrationArchiveArgs(
        databasePath: databasePath,
        databaseKey: databaseKey,
        archiveKey: archiveKey,
        outputPath: outputPath
    )
)
```

**Kotlin example.** Call the proposed generated function from a coroutine.

```kotlin
val archive = prepareMigrationArchive(
    PrepareMigrationArchiveArgs(
        databasePath = databasePath,
        databaseKey = databaseKey,
        archiveKey = archiveKey,
        outputPath = outputPath,
    )
)
```

The browser package exposes the same logical arguments. Its paths refer to files in the SDK's browser storage, not native operating-system paths. The browser adapter must use the existing worker and OPFS integration. It must define how an app reads the completed local archive for byte-based import. Package setup and that adapter example are implementation work.

Wait for the call to succeed. `archivePath` names the completed local file. The report counts emitted records and contains no keys. A failed write does not publish a completed new archive.

## What happens when metadata cannot be decoded

The converter uses a pinned legacy decoder for metadata stored in MLS state. If metadata cannot be decoded, it keeps the group and its eligible messages and leaves the affected optional metadata null. It retains metadata that can be decoded. It does not invent creator IDs, attributes, or admin lists.

In protobuf, null means an absent optional message or field. Existing scalar fields that cannot represent null use their documented unknown value. Do not create a new wire encoding for null.

This fallback applies to optional legacy metadata. A database read failure, a missing required group identity, or an unencodable required record still fails export.

The converter excludes messages with a disappearing deadline. If supported legacy data cannot establish that a message is non-disappearing, it excludes that message too. Missing or undecodable expiry data must not turn a disappearing message into permanent history. A conversation can remain in the archive when none of its messages qualify.

## 3. Import through the existing SDK workflow

Use the destination client for the correct inbox. Pass the completed file and archive key to the normal archive restore operation.

The Node SDK already supports file-based import. In this checked example, use `archive.archivePath` in place of `/path/to/archive.xmtp`, and supply the archive key as `key`:

```ts source="backups-node.ts" region="import"

```

An optional preview uses the existing method:

```ts source="backups-node.ts" region="metadata"

```

A preview reads metadata without applying records. It does not validate the complete archive.

The browser SDK already supports byte-based import. Once the browser storage adapter supplies the completed local file as `data: Uint8Array`, use the archive key as `key` in this checked example:

```ts source="backups-browser.ts" region="import"

```

The browser file-read adapter is still to be specified. The import call above is the existing SDK method. Swift and Kotlin use their existing archive restore workflows.

**Inbox validation is not implemented by these calls today.** Archive metadata has no owner inbox ID. To check the inbox inside the existing import operation, the format must carry that ID and the importer must compare it with the destination client before applying records. This is the remaining contract decision, not a check performed by `prepareMigrationArchive`.

## 4. Re-create missing conversations

After import, the new SDK will provide methods to re-create missing DMs and groups on the new network. These are separate SDK methods, not part of `prepareMigrationArchive`.

Re-creation may fail if some members of the conversation are not yet on the new network. Retry the method later as members move to the new network. The method names and signatures are still to be defined.

## 5. Check the result and retry if needed

Check known conversations, message contents, and consent settings. Unavailable legacy metadata can appear as unknown. Existing messages remain unchanged, existing groups keep live state, and consent uses the existing merge rule.

Export counts need not equal the increase in destination counts because the destination can already hold records. Keep the source until you accept the result.

| Result                                           | Next action                                                                             |
| ------------------------------------------------ | --------------------------------------------------------------------------------------- |
| Source is busy or locked                         | Close the legacy SDK and retry.                                                         |
| Source cannot be opened                          | Check the path, database key, and required sidecars.                                    |
| Unsupported schema                               | Use a package release that supports the source's last legacy SDK major.                 |
| Optional metadata cannot be decoded              | The converter uses null for the unavailable metadata and continues.                     |
| Migration, required-record read, or export fails | Correct the cause and run the same function again. Do not import partial output.        |
| Import fails partway through                     | Correct a transient cause and retry the existing import call. Completed records remain. |

For example, after correcting an import failure, retry the same Node call:

```text
await destinationClient.archives.importFromFile(
  archive.archivePath,
  archiveKey,
);
```

There is no migration resume cursor or automatic retry loop. Repeating malformed input does not repair it. Removing the archive does not undo an import.
