# MLS test fixtures

## Server configuration

`src/server_configuration.rs` resolves the deployment's published configuration
before any identity work and hands the client a `ServerConfigurationHandle`.
The snapshot is fixed for the client's life; the hourly worker in
`server_configuration/worker.rs` only rewrites the stored row and blocks the connection on a
mismatch or a raised minimum version.

- Read it with `client.server_configuration()`. Read a deployment's without a
  client or database with `server_configuration::fetch_server_configuration`.
- `handle.check()?` checks the permanent blocked connection for local entry
  points. `server_configuration/preflight.rs` also guards every client-owned
  API dispatch and incoming factory. After an offline build at a moved URL,
  it fetches, validates, and stores configuration before the first request.
  Refresh and deferred admission share one per-client lock. Do not hold a DB
  query across that fetch or put the client hook in the shared wire registry.
  Ready calls use their existing lifetime admission; a refresh must not cancel
  an already admitted ready call when close starts.
- A preflight failure must reach the current reader or request immediately.
  Preserve its typed cause and retryability for a new operation. Do not cache
  it as a rejected target request: the target was never sent.
- A test that needs a specific snapshot passes `tester!(alix, config_provider: …)`
  with a `xmtp_configuration::StaticConfigProvider`. That short-circuits the
  fetch, the store, the refresh, and the identifier binding, so no backend has
  to publish the value. A test that needs the real wire path uses
  `EphemeralBackend::start(toml)` instead.
- Drive one refresh in a test with
  `server_configuration::worker::ConfigurationWorker::new(context).tick().await`.

## Ephemeral test backends

Native `xmtp_mls` tests can use `EphemeralBackend::start(toml)` and
`tester!(alix, backend: &backend)` with optional `auth: callback`.
This helper is available only under `cfg(test)`, not `xmtp_mls/test-utils`.
Import backend fixtures through `xmtp_backend/test-utils`.

Nextest runs each test in its own process, so tests cannot share an ephemeral
backend. Each such test pays for a database create, a migration, and a listener bind.

Run `dev/check-ephemeral-backend` after changes to fixture dependency boundaries
or the test recipe environment. The Rust workspace CI job runs this check.
