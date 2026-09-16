# xmtp_mls

Core client. Groups, messages, sync, streams.

## Commands

```bash
just check crate xmtp_mls
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_mls                # needs `just backend up` (anvil)
dev/nix-shell "cargo nextest run -p xmtp_mls --profile ci -E 'test(test_valid_deletion_by_sender)'"
dev/nix-shell "cargo nextest run --profile ci -p xmtp_mls -E 'test(/messages::/)'"   # one module
```

## Gotchas

- Needs `just backend up`.
- Tests use one backend client. Native callback streams use backend bidi streams.
- Use the `ci` nextest profile for the backend suite. `--ignore-default-filter` includes tests excluded by the default filter.
- Set `XMTP_BACKEND_URL=http://127.0.0.1:5050` if localhost selects IPv6.
- Proxy tests share one backend proxy. Run them in a separate nextest invocation
  with `--test-threads 1`; do not run another proxy test process at the same time.
- Reproduce bidi fuzz failures with the logged `XMTP_BIDI_FUZZ_SEED`. Keep the
  default round count and use `--retries 0` so a new seed cannot hide a failure.

## Conventions

- Message ids: `src/utils/mod.rs:36 id::calculate_message_id(group_id, bytes, idempotency_key)` and `:61 calculate_message_id_for_intent(intent)` (`src/groups/mls_sync.rs:41`). Never re-derive the `group_id \t key \t payload` hash. If `apps/backend` needs it without `xmtp_mls`, move the function to a shared crate.
- Test clients: `tester!(alix)` / `tester!(bo, from: alix)` / `tester!(alix2, snapshot: snap)` (`src/utils/test/tester_utils.rs:822`, builder `:406 TesterBuilder`). Any `TesterBuilder` method works as a `key: value` or bare `key` argument. `src/utils/test/mod.rs` adds `ClientBuilder::temp_store()`, `.dev()`, `.local()`. All of it sits behind `cfg(test)` or the `test-utils` feature (`src/utils/mod.rs:8`); `apps/backend` and crates below `xmtp_mls` need their own fixture.
- A module may hold several related error enums: `src/groups/error.rs` defines `GroupError:93`, `DeleteMessageError:471`, `MetadataPermissionsError:507`, `DmValidationError:560`. Derive `ErrorCode` only when the code must be stable across the FFI boundary (`:92` derives it; `:470` does not). `RetryableError` is implemented by hand, delegating to inner errors (`:588`).

## Server configuration (spec 006)

`src/server_configuration.rs` resolves the deployment's published configuration
before any identity work and hands the client a `ServerConfigurationHandle`.
The snapshot is fixed for the client's life; the hourly worker in
`server_configuration/worker.rs` only rewrites the stored row and latches on a
mismatch or a raised minimum version.

- Read it with `client.server_configuration()`. Read a deployment's without a
  client or database with `server_configuration::fetch_server_configuration`.
- `handle.check()?` is the latch gate. It is already on the client-level,
  group-sync, and publish paths; do not add a second one.
- A test that needs a specific snapshot passes `tester!(alix, config_provider: …)`
  with a `xmtp_configuration::StaticConfigProvider`. That short-circuits the
  fetch, the store, the refresh, and the identifier binding, so no backend has
  to publish the value. A test that needs the real wire path uses
  `EphemeralBackend::start(toml)` instead.
- Drive one refresh in a test with
  `server_configuration::worker::ConfigurationWorker::new(context).tick().await`.

## Ephemeral test backends

`just test` needs `just backend up db replica` and the shared backend services.
It sets `SQLX_OFFLINE=true` for compilation and `DATABASE_URL` for test runs.
The database URL defaults to this worktree's database; `just backend status`
prints it. The main checkout uses `postgres://xmtp:xmtp@localhost:55432/xmtp_backend`.
Native `xmtp_mls` tests can use `EphemeralBackend::start(toml)` and
`tester!(alix, backend: &backend)` with optional `auth: callback`.
This helper is available only under `cfg(test)`, not `xmtp_mls/test-utils`.
The backend exports its fixtures through `xmtp_backend/test-utils`. Its own
`cargo test` still uses the existing dev-dependencies.

Use the shared backend on port 5050 by default. Use an ephemeral backend only
when a test needs a specific configuration. Nextest runs each test in its own
process, so tests cannot share an ephemeral backend. Each such test pays for
a database create, a migration, and a listener bind.
