# xmtp_db

Client storage: Diesel + SQLCipher, no PostgreSQL or SQLx.

```sh
dev/nix-shell 'just test crate xmtp_db'
dev/nix-shell 'cargo update-schema' # after migrations; never hand-edit schema_gen.rs
```

- Migrations are append-only and must sort after existing ones by both directory
  name and digits-only version **string**. Cover baseline upgrades; never delete
  old client data on initialization failure.
- Use per-table `Query*` traits, the `DbQuery` aggregate, and existing model
  macros. Transactions belong to `XmtpMlsStorageProvider`, not `DbConnection`;
  handle `TransactionOutcome` explicitly.
- Secret columns use `StoredSecret`; key-store encoding goes through
  `sql_key_store::codec`. Never rename stored fields/variants, add fields without
  serde defaults, or edit frozen golden rows. Do not log contexts or value bytes.
- Persist state effects and durable progress/rejections atomically. Default
  delivery has one owner; check it before handing data to the app. Preserve
  prepared bytes when matching late publish replies.
- Browser storage must use guarded lifecycle APIs, not raw `get_sqlite` or file
  copies. A required worker restart must retain the OPFS Web Lock until termination.
- Before migration, codec, transaction, durable-stream, or OPFS work, read the relevant
  section of [storage development](DEVELOPMENT.md) and the owning spec.
