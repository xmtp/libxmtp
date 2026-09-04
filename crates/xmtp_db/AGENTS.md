# xmtp_db

Storage. Diesel over encrypted SQLite (SQLCipher).

## Commands

```bash
just check crate xmtp_db
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_db
just test v3 -p xmtp_db --ignore-default-filter test_it_stores_group   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp_db -E 'test(/encrypted_store::group::/)'"   # one module
dev/nix-shell 'cargo update-schema'      # regen schema.rs after a migration
```

## Gotchas

- Migrations: `crates/xmtp_db/migrations/`. Add one, then refresh `schema.rs` with the `update-schema` command above.
- `test_db_migrates` is flaky (sqlcipher). CI retries it 3x.
- `update-schema` emits `id -> Nullable<Integer>` for `d14n_migration_cutover`. Keep it `Integer` or `xmtp_db` stops compiling. Check the diff.
