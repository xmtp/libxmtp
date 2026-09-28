# xmtp_api

Backend API wrapper. It owns retries, request limits, paging, and result maps.

```bash
just check crate xmtp_api
just test crate xmtp_api
```

The real RPC test needs the worktree backend and PostgreSQL.
Use `just backend up` for the Docker stack and its reduced query row limit.
Use `just backend db-up`, `just backend build`, and `just backend run` to run
outside Docker. Mock tests use `MockBackendClient`.
Run one test with `just test workspace -p xmtp_api read_topic_boundaries`.

Keep each commit and its proposals in one `PublishUnit`. The unit retains
canonical bytes and cannot be split. Request limits come from the snapshot the
deployment published (spec 006 CFG-064, CFG-065): `ApiClientWrapper::limits()`,
installed at client build by `set_configuration`. Build every unit with
`PublishUnit::new_within` / `single_within` against those limits. The
`new` / `single` constructors fall back to `LimitsConfiguration::default()`,
which is the compiled `xmtp_configuration::BACKEND_DEFAULT_MAX_*` set, and exist
only for a caller that holds no snapshot.

`query_all` advances a separate cursor for each topic and reads until
`has_more` is false. Key-package results include `None` for missing keys.
Inbox results keep input order and duplicates. Match gRPC codes and structured
publish details; do not inspect error message text.

Keep auth errors typed. `dyn_err` maps `ApiClientError::Auth` to `ApiError::Auth`
before it erases transport errors, so bindings keep the public auth code.

## Client request admission

`ApiClientWrapper::api_client` is a guarded adapter. A built MLS client binds
one shared preflight hook before workers start. Keep every RPC and stream open
behind that hook. Do not expose its raw transport outside test helpers.
`ConfigurationFetch` is the restricted, credential-free fetch capability used
by the hook, so a deferred configuration fetch cannot call itself.

`ApiError::Preflight` retains the typed cause. A preflight failure ends the
current logical request even when its cause is retryable. A new operation can
retry. Preserve the marker in `dyn_err`; ordinary transport retries and auth
codes keep their existing behavior.
