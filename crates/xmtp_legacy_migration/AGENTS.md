# Legacy migration

Use raw legacy SQL only on a temporary copy. Never open the source with SQLite.
The embedded migrations come from the revision in `README.md`.

## Commands

```bash
dev/nix-shell 'just check crate xmtp_legacy_migration'
dev/nix-shell 'just test crate xmtp_legacy_migration'
```

Keep fixtures and their provenance together. Do not regenerate a fixture with
current SDK storage initialization.

Eligible stored messages have a fixed 64 MiB budget across all retained variable
fields. Check SQL byte lengths before loading rows into Rust. Keep the exact
limit inclusive. An oversized row fails the whole preparation with RecordRead;
do not skip it. Preserve the SQL check when fields are added to MessageRow.
