# xmtp_api

Backend API wrapper. It owns retries, request limits, paging, and result maps.

```bash
dev/nix-shell 'cargo test -p xmtp_api'
dev/nix-shell 'cargo clippy -p xmtp_api --all-targets -- -D warnings'
```

The real RPC test needs the backend at `http://localhost:5050` and PostgreSQL.
Set `XMTP_BACKEND_URL` to use another test instance.
Use `just backend up` for the Docker stack and its reduced query row limit.
Use `just backend db-up`, `just backend build`, and `just backend run` to run
outside Docker. Mock tests use `MockBackendClient`.
Run one test with `dev/nix-shell 'cargo nextest run --profile ci -p xmtp_api read_topic_boundaries'`.

Keep each commit and its proposals in one `PublishUnit`. The unit retains
canonical bytes and cannot be split. All request limits come from
`xmtp_configuration::BACKEND_DEFAULT_MAX_*`.

`query_all` advances a separate cursor for each topic and reads until
`has_more` is false. Key-package results include `None` for missing keys.
Inbox results keep input order and duplicates. Match gRPC codes and structured
publish details; do not inspect error message text.
