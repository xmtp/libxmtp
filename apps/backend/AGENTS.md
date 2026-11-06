# xmtp-backend

Self-hosted gRPC/gRPC-Web service; PostgreSQL 17+.

```sh
dev/nix-shell 'just backend db-up'
dev/nix-shell 'just backend test --lib <filter>'
dev/nix-shell 'just backend sql-prepare' # after SQL/schema changes
dev/nix-shell 'just backend sql-check'
```

- Migrations are append-only. Startup never deletes databases. Commit `.sqlx`
  updates via the root recipes, never SQLx `--workspace`; keep sqlx-cli aligned.
- Configuration types/checks belong in `xmtp_backend_config`; runtime validation
  belongs here. Share payload validation/encoding with `xmtp_mls_validation` and
  `xmtp_proto`, never a client database.
- Storage helpers use internal records and typed errors; API code owns protobuf
  conversion and request normalization. Use behavior names, not requirement IDs.
- Use module-local tests and `TestDatabase` cleanup guards. Never skip database
  tests when services are unavailable. Export fixtures through `test-utils`.
  Replica tests must wait for replay before connecting; read
  [test setup](DEVELOPMENT.md) before adding replica/auth fixtures.
- Use `parking_lot::Mutex` for short synchronous locks; never hold guards across
  await. Stream termination tests need one total deadline.
- Document important functions' invariants, errors, and cancellation with RustDoc.
  Never log payloads, topics, or auth headers. Keep `telemetry::CATALOGUE` aligned
  with OPS and the [observability guide](../../docs/backend-observability.md).
- Build, S3, TLS, native services, and deployment: [development guide](DEVELOPMENT.md).
