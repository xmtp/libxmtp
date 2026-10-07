---
prefix: MIG
status: draft
---
# Legacy database migration

Legacy conversion prepares conversation history and consent for archive import.
It reads closed local storage. It does not require a network connection or a
client installation on the destination network.

## Scope

In scope: supported legacy storage, source preservation, the app input and
result, optional metadata recovery, message eligibility, and completed output.

Out of scope: archive framing and import merge rules (ARCH); conversation
creation and activation (JOIN, DMS); attachment file transfer (ATCH); and an
archive owner inbox check. The app selects the destination inbox.

| Related | Relation |
| --- | --- |
| ARCH | Owns the archive format, record ordering, exclusions, and import behavior. |
| META | Owns metadata meaning and disappearing-message policy. |
| CONS | Owns consent meaning and conflict resolution. |

## Terms

| Term | Meaning |
| --- | --- |
| Converter | The process that reads legacy storage and prepares an archive. |
| Source | The closed legacy database and its sidecars. |
| Working copy | Temporary storage that holds the source data during conversion. |
| Publication | The atomic change that makes a completed archive available through the output path. |
| Supported range | The source SDK versions listed by the converter package release. |

## 1. Source access

The app closes the legacy SDK before conversion. A lock check can detect
conflicting access. It cannot prove that an idle client is closed. Committed
write-ahead log data is part of the source. The working copy permits schema
changes without changing the source.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| MIG-001 | Independent legacy conversion | When source storage is in the package's published compatibility range, the converter MUST prepare an archive without requiring a legacy or destination client. | An app must not need an active legacy service to read its history. |
| MIG-002 | Source bytes remain unchanged | When archive preparation succeeds, fails, or is cancelled, the converter MUST leave source database, WAL, and salt bytes unchanged; if conflicting source access is detected, it MUST fail before migration. | The app needs its source after a failed conversion. |

## 2. App input and completed output

The app supplies the source path, the database key when encrypted, the archive
key, and an output path. Paths identify local storage. The browser uses its
local storage names. The database key, when supplied, has 32 bytes. ARCH owns
the archive key length and format.

```webidl
dictionary PrepareMigrationArchiveArgs {
  required DOMString database_path;
  sequence<octet>? database_key;
  required sequence<octet> archive_key;
  required DOMString output_path;
};

dictionary MigrationReport {
  required DOMString archive_path;
  required unsigned long long group_count;
  required unsigned long long message_count;
  required unsigned long long consent_count;
};
```

The report counts emitted records. Import can add fewer records because the
destination can already contain history. The output is complete at publication.
A cancellation after publication does not undo the completed operation. A retry
starts a new conversion from the source.

Native publication replaces the final file. Browser publication commits an
IndexedDB record that names a completed private OPFS object. A per-output lock
coordinates readers, publication, and removal of unused objects. A page or worker
termination is a crash. On the next access, recovery removes private objects
that have no published record before it returns the completed archive.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| MIG-003 | Owned input and result | When an app requests archive preparation, the converter MUST accept the owned values in `PrepareMigrationArchiveArgs` and return `MigrationReport`, or a typed input, busy, schema, migration, record, or output error. It MUST reject a database key whose length is not 32 bytes. | An app needs stable error categories and record counts. |
| MIG-004 | Complete atomic output | When preparation fails or is cancelled before publication, the converter MUST preserve any completed output and remove its incomplete output. On success, it MUST publish only a complete archive under ARCH-001. If page or worker termination interrupts preparation, unpublished objects MUST remain unavailable through the output path and MUST be removed on the next storage access. | A partial file cannot replace an existing archive. |

## 3. Legacy record conversion

Conversion selects both standard archive categories with no time bounds. The
archive eligibility and secret exclusions remain defined by ARCH. Ordinary SDK
export retains its strict metadata failure behavior. Legacy conversion can
recover required history even when optional metadata is unavailable. Existing
protobuf absence and scalar unknown values represent unavailable metadata.

Some legacy imports lost expiry information. Their null-expiry history can
migrate. Conversion cannot prove that this history was always non-disappearing.
A null value added by a legacy schema migration also remains eligible. A
conversation can remain in the archive when no message qualifies.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| MIG-005 | Optional metadata degrades independently | When optional legacy metadata cannot be decoded, the converter MUST retain the group and eligible messages and omit only unavailable metadata; unreadable required records MUST fail preparation. | A damaged optional field must not remove readable history. |
| MIG-006 | Exclude recorded message expiry | When preparing an archive, the converter MUST exclude every otherwise eligible message whose stored `expire_at_ns` value is non-null. A null value, including one added by a legacy schema migration, MUST NOT cause exclusion by itself. | Keep available history while excluding every recorded disappearing deadline. |
| MIG-007 | Bounded stored-message reads | Before loading the retained variable fields of each otherwise eligible stored-message row, the converter MUST validate their combined byte length in SQL against a fixed 64 MiB budget. A row at the limit MUST remain supported. A row over the limit MUST fail the whole preparation with a typed record error, preserve source and previous output, and leave no incomplete output. | A large stored message must not cause an unbounded allocation or silent loss of history. |

The retained variable fields are the message ID, group ID, content bytes,
sender installation ID, sender inbox ID, authority ID, and optional reference
ID. Count TEXT as bytes, not characters. An absent optional field contributes
zero bytes. This is a per-row limit, not a database or archive size limit.
Conversion does not skip an oversized eligible message.
