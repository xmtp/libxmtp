---
prefix: STORE
status: draft
---
# Client storage

An SDK keeps each client's database at a location the app names or at the platform default.

## Scope

In scope: the database file name, default directories, the identity needed to build a client from a stored database, storage lifecycle, and client diagnostics.

Out of scope: database contents, encryption, and locations the app names outside the default.

## Terms

| Term | Meaning |
| --- | --- |
| Storage label | A value the app supplies to distinguish client databases in one directory. |
| Default location | The directory the SDK selects when the app does not name a location. |

## 1. Database location

An SDK keeps each client's database in one database file. When the app does not name a location, the SDK uses its platform's default directory below. Each platform's default is chosen for that platform; it is not always storage that is private to the app (STORE-004 uses the Node.js process working directory). `StorageLocation.Default` means this location. The defaults are not tied to earlier SDK releases, which used other directories.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| STORE-001 | Database file name | When an SDK opens a database at the default location or in a directory the app names, it MUST name the file `xmtp-<label>-<inbox id>.db3` when the app supplies a storage label and `xmtp-<inbox id>.db3` when it does not. | Two clients or two labels in one directory would otherwise open each other's database. |
| STORE-002 | Default directory on Apple platforms | Where an SDK runs on iOS or macOS and the app asks for the default location, the SDK MUST place the file in the `xmtp` directory inside a folder named by the app's bundle identifier in the app's Application Support directory. | Documents is visible to the user and can be moved or deleted from the Files app, and a non-sandboxed macOS app shares Application Support with every other app of the user; the bundle identifier is the system's stable name for the app. |
| STORE-003 | Default directory on Android | Where an SDK runs on Android and the app asks for the default location, the SDK MUST place the file in the `xmtp_db` directory inside the app's internal files directory. | External or shared storage is readable by other apps. |
| STORE-004 | Default directory on Node.js | Where an SDK runs on Node.js and the app asks for the default location, the SDK MUST place the file in the `xmtp` directory inside the process's current working directory at client creation. | Database files written straight into the working directory mix with the app's own files. |
| STORE-005 | Default directory in a browser | Where an SDK runs in a browser and the app asks for the default location, the SDK MUST place the file in the `xmtp-sdk` directory of the SDK's OPFS storage pool. | Files at the pool root collide with other files an origin keeps in OPFS. |
| STORE-006 | No default elsewhere | Where an SDK runs on a platform that STORE-002 to STORE-005 do not name, or on iOS or macOS in a process with no bundle identifier, a request for the default location MUST fail with a typed error before the SDK opens a file. | A guessed directory puts databases where the app cannot find or back them up. |
| STORE-007 | Build opens only an existing identity | When an app builds a client without a signer on a database that holds no stored identity, the SDK MUST fail with a typed error and MUST NOT register an installation. | A build on a new database would create a client with no identity the app signed for. |
| STORE-008 | Safe storage label | If a storage label contains a path separator (`/` or `\`), a colon (`:`), or a NUL character, then the SDK MUST fail with a typed error before it opens a file. | A label taken from outside the app could otherwise place the database outside the intended directory, or open another database. |

The SDK uses the storage label unchanged. On a file system that ignores letter case, labels that differ only in case name the same file.

## 2. Storage lifecycle

The storage interface reports the file in use and controls its connection and removal. An in-memory client has no database file to report or remove. The browser exposes no storage reconnect or delete operation.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| STORE-009 | Open database path | When an app reads a client's storage path, the SDK MUST return the path of the database file that client opened, or no path for in-memory storage. | A computed path that differs from the opened file can make an app back up the wrong database. |
| STORE-010 | Reconnect the same database | When an app reconnects storage after a disconnect, the SDK MUST reopen the same database file that client used before the disconnect. | Reconnecting to another file changes the client's stored identity and history. |
| STORE-011 | Delete closed storage | When an app asks to delete a client's storage, the SDK MUST refuse with a typed error while that client is open, and otherwise remove that client's database files. | Deleting a file in use can leave a live client writing to storage that no longer has a name. |
| STORE-012 | Browser storage operations | A browser SDK MUST NOT expose storage reconnect or delete operations. | Browser storage administration has a separate owner and lifecycle. |
