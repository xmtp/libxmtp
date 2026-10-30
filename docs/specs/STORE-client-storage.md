---
prefix: STORE
status: draft
---
# Client storage

An SDK keeps each client's database and attachments at a location the app names or at the platform default, by the storage location layouts of ATCH section 5.

## Scope

In scope: the default directories, how the default location and a directory the app names map to the storage location layouts, the database file name in a data directory, the storage label, the identity needed to build a client from a stored database, storage lifecycle, and the database encryption choice: an absent key, a supplied key, and creating and opening a database under a key.

Out of scope: the storage location layouts themselves and the attachments directory (ATCH section 5), database contents, encryption algorithms, and how an app creates and stores its database encryption key.

## Terms

| Term | Meaning |
| --- | --- |
| Storage label | A value the app supplies to keep separate client stores under one root directory. |
| Default location | The directory the SDK selects when the app does not name a location. |
| Root directory | The default location, or the directory the app names. It is the data directory of a `data_dir` storage location (ATCH-040), or holds that data directory when the app supplies a storage label. |
| Salt file | A file that an SDK keeps beside a database file it created under a database encryption key. The SDK needs it, with the key, to read that database, so an app that copies or backs up the database file copies the salt file with it. |

## 1. Database location

An SDK keeps each client's database in one database file, and stores the database and the attachments by the storage location layouts of ATCH section 5. The root directory is the data directory of a `data_dir` layout (ATCH-040). When the app supplies a storage label, the data directory is the directory named by the label inside the root directory. The database and the attachments of one inbox on one deployment are then in one inbox directory, `{data_dir}/{deployment}/{inbox_id}/` (ATCH-040). An explicit location (ATCH-080) names the database file and the attachments directory itself, and the SDK adds no label, deployment, or inbox component to either path.

When the app does not name a location, the SDK uses its platform's default directory below as the root directory. Each platform's default is chosen for that platform; it is not always storage that is private to the app (STORE-004 uses the Node.js process working directory). `StorageLocation.Default` means this location. The defaults are not tied to earlier SDK releases, which used other directories and file names.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| STORE-020 | Locations map to storage layouts | When an app asks for the default location or names a directory, the SDK MUST create the client with a `data_dir` storage location (ATCH-040) whose `data_dir` is the root directory, or, when the app supplies a non-empty storage label, the directory named by the label inside the root directory. When an app names a database path and an attachments directory, the SDK MUST create the client with an `explicit` storage location (ATCH-080) that holds those two paths unchanged. | An SDK that derives its own paths puts the database where another SDK does not open it, and the attachments where ATCH-044 does not find them. |
| STORE-019 | Database file in a data directory | When an SDK opens a database with a `data_dir` storage location, it MUST name the database file `{data_dir}/{deployment}/{inbox_id}/xmtp.db3`, with the components of ATCH-040. | Another SDK, or another version of one, that names the file otherwise opens an empty database with no identity. |
| STORE-002 | Default directory on Apple platforms | Where an SDK runs on iOS or macOS and the app asks for the default location, the SDK MUST use as the root directory the `xmtp` directory inside a folder named by the app's bundle identifier in the app's Application Support directory. | Documents is visible to the user and can be moved or deleted from the Files app, and a non-sandboxed macOS app shares Application Support with every other app of the user; the bundle identifier is the system's stable name for the app. |
| STORE-003 | Default directory on Android | Where an SDK runs on Android and the app asks for the default location, the SDK MUST use as the root directory the `xmtp_db` directory inside the app's internal files directory. | External or shared storage is readable by other apps. |
| STORE-004 | Default directory on Node.js | Where an SDK runs on Node.js and the app asks for the default location, the SDK MUST use as the root directory the `xmtp` directory inside the process's current working directory at client creation. | Database files written straight into the working directory mix with the app's own files. |
| STORE-005 | Default directory in a browser | Where an SDK runs in a browser and the app asks for the default location, the SDK MUST use as the root directory the `xmtp-sdk` directory of the SDK's OPFS storage pool. | Files at the pool root collide with other files an origin keeps in OPFS. |
| STORE-006 | No default elsewhere | Where an SDK runs on a platform that STORE-002 to STORE-005 do not name, or on iOS or macOS in a process with no bundle identifier, a request for the default location MUST fail with a typed error before the SDK opens a file. | A guessed directory puts databases where the app cannot find or back them up. |
| STORE-007 | Build opens only an existing identity | When an app builds a client without a signer on a database that holds no stored identity, the SDK MUST fail with a typed error and MUST NOT register an installation. | A build on a new database would create a client with no identity the app signed for. |
| STORE-021 | Safe storage label | If a storage label is `.` or `..`, or contains a path separator (`/` or `\`), a colon (`:`), or a NUL character, then the SDK MUST fail with a typed error before it opens or creates a path. | A label taken from outside the app could otherwise place the client's files outside the root directory, or open another client's database. |

The SDK uses the storage label unchanged as one directory name. On a file system that ignores letter case, labels that differ only in case name the same directory.

## 2. Storage lifecycle

The storage interface reports the file in use and controls its connection and removal. An in-memory client has no database file to report or remove. The browser exposes no storage reconnect or delete operation.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| STORE-009 | Open database path | When an app reads a client's storage path, the SDK MUST return the path of the database file that client opened, or no path for in-memory storage. | A computed path that differs from the opened file can make an app back up the wrong database. |
| STORE-010 | Reconnect the same database | When an app reconnects storage of a client it has not ended, the SDK MUST reopen the database file that client opened. | Reconnecting to another file changes the client's stored identity and history. |
| STORE-011 | In-memory storage cannot be deleted | When an app asks to delete in-memory client storage, the SDK MUST fail with a typed invalid-input error. | In-memory storage has no database file to remove. |
| STORE-017 | Close before file removal | When an app asks to delete file-backed client storage, the SDK MUST close the client before it removes the database file. | A live connection could otherwise write to a file after its name is removed. |
| STORE-018 | Storage files gone after deletion | When deletion of file-backed client storage or a browser database completes, the SDK MUST have removed the database file and every journal, shared-memory, salt, and temporary salt file belonging to it; native client storage deletion MUST also remove the attachment key directories, staged ciphertext, and unfinished attachment files in that client's attachments directory. If a required removal fails, the SDK MUST report a typed error, and a later retry MUST continue past files already absent. The SDK MUST NOT remove other files in the attachments directory. | A database-only deletion leaves plaintext and recovery data after the app reports that storage is gone. |
| STORE-026 | Synced before it leaves the device | Before a client returns a queued message's id, or sends a request that publishes a prepared envelope or a key package, it MUST have completed a sync to stable storage of the file-backed database write that recorded that work. | An operating system crash or power loss can otherwise erase the local record of work other devices already hold: the installation forks its group, loses the send, or cannot read Welcomes to the package. |

## 3. Database encryption choice

An app can use a database without an encryption key. When the key is absent,
the database is not encrypted by the SDK. This choice applies when the app
creates a database and when it opens that database again.

Creation of a database under a database encryption key lasts from the SDK's first write for a path whose database file is absent or 0 bytes long until the call that opens the database returns.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| STORE-022 | Optional database encryption key | When an app omits the database encryption key, the SDK MUST support creating a database and reopening a database created without a key, without requiring a key. | Requiring a key prevents an app from using or reopening its existing store. |
| STORE-023 | Interrupted keyed creation stays openable | If the app's process exits or is killed at any point while the SDK creates a file-backed database under a database encryption key, and the device keeps running, then when the app later opens the same path with the same key, the SDK MUST open a database at that path. This requirement does not cover a power loss or an operating system crash. | A stop that leaves files the key cannot read makes every later open of that path fail, so the app cannot create or restore its client there without deleting files by hand. |
| STORE-024 | Failed keyed open preserves the store | If an app opens with a database encryption key a path whose database file is longer than 0 bytes and the SDK cannot read that database with the key, then the SDK MUST fail with a typed error. It MUST NOT change the data the database holds, and MUST NOT create, change, or remove its salt file. | An open that rewrites or resets a store it cannot read can destroy data that the right key, or the missing salt file restored from a copy, would still recover. |
| STORE-025 | A supplied key is applied or refused | When an app supplies a database encryption key for a file-backed database, the SDK MUST either store that database encrypted under the key, so that a later open of it with a different key fails with a typed error, or fail with a typed error before it opens a database. | A key the SDK accepts and ignores leaves installation keys and decrypted messages readable from a copy of the files while the app believes they are protected. |

## Known limitations

Storage reconnect fails after the app ends the client; the app builds a new client instead.

Deletion under STORE-018 removes file names; it does not promise forensic erasure of deleted blocks, backups, or copied files. Empty storage directories and the deployment record of a data directory stay. Browser database administration removes the database file family from its storage pool; the separate attachments directory stays. An app closes every other client or process using the same database or attachments directory before native deletion, because the SDK stops only the client receiving the request.

The replacement STORE paragraph makes the surviving empty directories, deployment record, browser attachment tree and lack of forensic erasure explicit.

A browser SDK does not encrypt its database. Anyone who can read the origin's browser storage, for example from a copied browser profile, can read the stored installation keys and decrypted messages.

Older SDK releases wrote some encrypted databases without a salt file. The SDK does not convert such a database, and a keyed open of it fails (STORE-024).

A power loss shortly after an SDK first creates an encrypted database can lose the new salt file on a file system that cannot sync a directory, and on Windows, where the SDK does not sync the directory. A later keyed open of that path can then fail (STORE-024), and the app removes the files and creates its client again. Closing this window on those systems needs platform-specific file-system calls.
