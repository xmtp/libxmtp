# Legacy migration

Use raw legacy SQL only on a temporary copy. Never open the source with SQLite.
The embedded migrations come from the revision in `README.md`.

## Commands

```bash
just check crate xmtp_legacy_migration
just test crate xmtp_legacy_migration
```

Keep fixtures and their provenance together. Do not regenerate a fixture with
current SDK storage initialization.
