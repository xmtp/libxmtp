# Steps for setting up a diesel migration

### Install the CLI onto your local system (one-time)

```
cargo install diesel_cli --no-default-features --features sqlite
```

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

This updates the generated `schema.rs` file. You can now update `models.rs` to reference it and consume your new model in the rest of the crate.
