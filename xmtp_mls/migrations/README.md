# Steps for setting up a diesel migration

### Install the CLI onto your local system (one-time)

```
cargo install diesel_cli --no-default-features --features sqlite
```

### Change directory to libxmtp/xmtp_mls/

### Create your migration SQL

In this example the migration is called `create_key_store`:

```
diesel migration generate create_key_store
```

Edit the `up.sql` and `down.sql` files created

### Generate application code

```
cargo run --bin update-schema
```

This updates the generated `schema.rs` file. You can now update the models and queries to reference it in `xmtp_mls/src/storage/encrypted_store/`.
