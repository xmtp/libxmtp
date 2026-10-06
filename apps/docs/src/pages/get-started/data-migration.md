---
layout: "@astrojs/starlight/components/StarlightPage.astro"
title: Migrate legacy SDK data
description: Move legacy XMTP SDK history and consent to the self-hosted SDK through a local archive.
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

Use `prepareMigrationArchive(args)` to convert a closed legacy database to a standard XMTP archive. Then import that archive with the current SDK. The standalone converter needs no SDK client or network connection.

The converter copies the source before it applies legacy migrations. The source database and its sidecars stay unchanged. You do not need to install intervening SDK releases.

## Supported legacy data

The converter accepts known migration prefixes from the last legacy SDK majors: Node 6, Browser 7, Agent 2, iOS 4, and Android 4. The Agent SDK uses Node storage. Fixtures cover the Node 6.1.0, Browser 7.1.0, iOS 4.10.0 and 4.11.0, and Android 4.10.0 and 4.11.0 schema endpoints. Unknown migration histories fail with `UnsupportedSchema`.

The archive contains groups, DMs, application messages, and consent. Message IDs, content bytes, and nanosecond timestamps stay unchanged. Virtual sync conversations, one-shot conversations, and groups already marked as restored are excluded.

**Messages with a stored expiry deadline are always excluded.** This includes deadlines in the past and future. A null deadline does not exclude a message. Some older schemas and previously imported histories have no recorded deadline. Those messages are included, even when their original disappearing-message status is unknown. The converter does not infer expiry from publish state, push flags, or timestamps. A conversation can remain in the archive when it has no eligible messages.

The archive does not contain installation keys, MLS secrets, or attachment files. Imported conversations are history. Import does not grant authority to send or decrypt later group traffic.

## 1. Close the legacy SDK

Close the legacy client and every connection to its database. Native lock checks can detect some active writers. POSIX checks do not report locks held by the calling process. No lock check can prove that an idle client is closed. Closing the client remains the caller's responsibility.

For native storage, keep the database and its sidecars together. This includes the WAL and SQLCipher salt file when present. The working copy includes committed WAL data. Supply the original 32-byte database key for encrypted storage. Omit `databaseKey` for unencrypted storage.

Create a separate random 32-byte archive key. Save it in the app's key storage before export. Use the same key for import. Choose an output path that is separate from the source and its sidecars.

## 2. Create and import the archive

### Node and Agent

Use the standalone `@xmtp/migration` package. Keys are `Uint8Array` values. Buffer views are accepted without including bytes outside the view. Report counts are `bigint` values.

```ts source="legacy-migration-node.ts" region="imports"

```

The app supplies a destination client for the correct inbox, file paths, and keys:

```ts source="legacy-migration-node.ts" region="migrate"

```

The Agent SDK uses the same Node converter and archive import workflow.

### Browser

Use `@xmtp/browser-migration` on a secure origin with OPFS and Web Locks support. Close the legacy client and the current SDK before conversion. The migration worker owns the SDK storage pool until conversion ends. The promise settles after worker termination and storage release.

Use the exact legacy database name, including its leading slash if present. `outputPath` is a logical archive name in the separate `xmtp-migration-archives` OPFS directory. Browser legacy storage is unencrypted; omit `databaseKey`.

```ts source="legacy-migration-browser.ts" region="imports"

```

Open the destination client after conversion completes. `readMigrationArchive` returns the completed file as bytes for the existing importer:

```ts source="legacy-migration-browser.ts" region="migrate"

```

### Swift

Add the standalone `XmtpMigration` Swift package and import its generated module. The app supplies `String` paths and `Data` keys. Report counts are `UInt64` values.

```swift
import XmtpMigration

let archive = try await prepareMigrationArchive(
    args: PrepareMigrationArchiveArgs(
        databasePath: databasePath,
        databaseKey: databaseKey,
        archiveKey: archiveKey,
        outputPath: outputPath
    )
)
```

Pass `archive.archivePath` and the archive key to the current Swift SDK archive restore workflow.

### Kotlin

Add the standalone `org.xmtp:migration` Android library. The app supplies `String` paths and `ByteArray` keys. Call the function from a coroutine. Report counts are `ULong` values.

```kotlin
import uniffi.xmtp_migration.PrepareMigrationArchiveArgs
import uniffi.xmtp_migration.prepareMigrationArchive

val archive = prepareMigrationArchive(
    PrepareMigrationArchiveArgs(
        databasePath = databasePath,
        databaseKey = databaseKey,
        archiveKey = archiveKey,
        outputPath = outputPath,
    )
)
```

Pass `archive.archivePath` and the archive key to the current Kotlin SDK archive restore workflow.

## Optional metadata and failures

The converter uses the pinned legacy MLS decoder for conversation metadata. If an optional field cannot be decoded, the field is absent. Other decoded fields remain. The converter does not invent creator IDs, attributes, or admin lists. Required record failures still stop conversion.

`MigrationError` has six stable variants: `InvalidInput`, `SourceBusy`, `UnsupportedSchema`, `Migration`, `RecordRead`, and `Output`. Use the typed variant to handle a failure. Do not match error message text.

A failed conversion does not replace an existing completed archive. On native targets, cancellation before publication also preserves the old output. A successful report names the completed file and gives the emitted group, message, and consent counts. It contains no keys.

**Inbox validation is not implemented.** Archive metadata has no owner inbox ID. The app must select the correct destination inbox. This migration does not change the archive wire format or add an owner check.

## 3. Re-create missing conversations

**TODO:** Define SDK methods to re-create missing DMs and groups on the new network. These methods are outside this converter. Re-creation can fail until the required members move to the new network. Retry later when the methods become available.

## 4. Check the result and retry

Check known conversations, message contents, and consent settings. Missing legacy metadata can appear as unknown. Existing messages remain unchanged, live groups keep their state, and consent uses the existing merge rule.

Export counts need not equal the increase in destination counts. The destination can already contain records. Keep the source until you accept the result.

| Result                         | Next action                                                                 |
| ------------------------------ | --------------------------------------------------------------------------- |
| Source is busy                 | Close all clients that use the source, then retry.                          |
| Invalid input                  | Check paths, key lengths, the database key, and sidecars.                   |
| Unsupported schema             | Check that the source uses a supported legacy migration history.            |
| Migration or record read fails | Correct the cause, then run the converter again.                            |
| Output fails                   | Check free space and permissions, then retry. Do not import partial output. |
| Import fails partway through   | Correct the cause, then retry the same completed archive and key.           |

Import retry is idempotent for the same archive. Completed records remain after a partial import. No migration-specific resume option is needed.
