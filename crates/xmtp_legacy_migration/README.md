# Legacy archive conversion

`prepare_migration_archive` converts closed legacy XMTP storage to the existing encrypted archive format. It is an internal Rust crate. The normal SDK exposes it through UniFFI. It does not create a current SDK client or use the network.

The public arguments are `database_path`, optional `database_key`, `archive_key`, and `output_path`. Keys contain 32 bytes. The report contains the completed archive path and `u64` group, message, and consent counts. `MigrationError` has stable `InvalidInput`, `SourceBusy`, `UnsupportedSchema`, `Migration`, `RecordRead`, and `Output` variants.

Native conversion checks detectable SQLite locks, copies the main database and durable sidecars, then opens only the copy. It applies the known legacy migration suffix and streams records through `xmtp_archive::exporter::ElementWriter`. It flushes and synchronizes a sibling temporary output before atomic publication. A failed operation or cancellation before publication preserves an existing output. Close the legacy client first: lock checks cannot detect an idle client or every same-process connection.

Browser conversion copies the closed OPFS source in 64 KiB chunks under the storage lifecycle guard. It applies migrations in a temporary OPFS database with a bounded SQLite cache. Cleanup removes the working data before archive publication. The next conversion removes working data left by worker termination. A dedicated worker owns the storage pool and ends before the wrapper settles. The output uses a separate OPFS directory. `readMigrationArchive` supplies bytes to the existing current SDK importer.

Groups, DMs, application messages, and consent use the normal archive records. Every stored non-null message expiry deadline is excluded. Null deadlines remain eligible, including null values added by a legacy migration. Older or previously imported history can therefore include messages whose original expiry is unknown. No timestamp, publication, or push heuristic is used.

The isolated metadata decoder reads only the pinned OpenMLS group context. It keeps independently decoded optional fields and omits malformed fields. Required record failures stop the export. Installation keys, MLS secrets, and attachment files are never archive elements.

See [fixture provenance](fixtures/README.md) for source pins and schema coverage, and [normal SDK commands](../xmtp_sdk/AGENTS.md) for build commands. The migration guide is at `apps/docs/src/pages/get-started/data-migration.md`.

Inbox ownership validation and conversation re-creation are outside this converter. The archive wire format is unchanged.
