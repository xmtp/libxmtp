# Backend tests

## `apps/backend`

Same `#[xmtp_common::test(unwrap_try = true)]`. The runner is `cargo test`,
not nextest, with four threads by default to bound database connections
(`RUST_TEST_THREADS` overrides). `just test` excludes this crate.

```bash
just backend db-up                                 # once; disposable primary and replica
just backend test                                  # everything
just backend test --lib service::publish::tests    # one module; append a function name to narrow
```

Fixtures live in `apps/backend/src/test_support.rs` and are exported to other
crates under `xmtp_backend/test-utils`:

```rust
// apps/backend/src/auth/jwks/tests.rs
#[xmtp_common::test(unwrap_try = true)]
async fn startup_retries_until_keys_arrive() {
    let jwks = JwksServer::start(vec![JwksResponse::error(), JwksResponse::keys(&[key])]).await;
    let server = TestServer::new(|config| config.auth = Some(jwks.config())).await?;
    assert_eq!(jwks.requests().len(), 2);
    server.stop().await?;                           // stops, then drops the database
}
```

- `TestServer::new(|config| ..)` starts a fresh database and server on an
  OS-chosen loopback port. `TestServer::from_toml(toml)` layers a partial TOML
  over the defaults. `with_verifier(change, verifier)` injects a
  smart-contract-wallet verifier.
- `server.query()`, `publisher()`, `identity()` return gRPC clients;
  `server.publish(envelopes)` is the shortcut. Use these boundaries, not
  internal calls.
- `TestDatabase::new()` for a database without a server;
  `RunningServer::new(config)` for a second instance on the same database.
- `test_support::auth`: `TestKey::{es256, es384, eddsa, rsa}`, `mint`,
  `valid_claims`, `JwksServer::start`.
- `query_topic(topic, sequence_id)` and `topic(kind, id)` build request values.
- Log assertions: `xmtp_logging::test_logging::LogCapture`
  (see [utilities.md](utilities.md)).

Every new endpoint gets an integration test: happy path, each error, each
limit.

## `EphemeralBackend` for `xmtp_mls` tests

Use the shared stack by default. Start an ephemeral backend only when a test
needs a specific backend configuration. Nextest runs each test in its own
process, so tests cannot share one; each pays for a database, a migration, and
a listener bind. Native `cfg(test)` only; not available through
`xmtp_mls/test-utils`.

```rust
// crates/xmtp_mls/src/utils/test/backend/tests.rs
#[xmtp_common::test(unwrap_try = true)]
async fn ephemeral_applies_partial_config() {
    let backend = EphemeralBackend::start(r#"
        [limits]
        default_query_limit = 7
    "#).await?;
    tester!(alix, backend: &backend);
    let group = alix.create_group(None, None)?;
    group.send_message(b"ephemeral", SendMessageOpts::default()).await?;
}
```

`tester!(alix, backend: &backend, auth: callback)` adds an
`Arc<dyn AuthCallback>`. `backend.url()` and `backend.config()` expose the
instance. `dev/check-ephemeral-backend`, run in CI, asserts `xmtp_backend`
never reaches wasm or `test-utils` builds.
