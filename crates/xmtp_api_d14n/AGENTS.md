# xmtp_api_d14n

Backend client, endpoints, decoders, middleware, and static streams.
The crate name stays until the Phase 3 rename task.

```bash
dev/nix-shell 'cargo test -p xmtp_api_d14n'
dev/nix-shell 'cargo clippy -p xmtp_api_d14n --all-targets -- -D warnings'
```

`BackendClient<C>` implements the backend unary trait. `MessageBackendBuilder`
requires one host URL and keeps the auth callback, auth handle, app version,
and read-only option. The wrapper in `xmtp_api` owns retries and request limits.

Static streams and bidi streams use backend frames and scalar topic cursors.
`queries/backend/` holds the backend binding and transport implementation.
`queries/{bidi,bidi_transport,bidi_transport_props}.rs` hold the shared transport
and its tests. The legacy protocol and client modules have been removed.

Tests use a mock transport. Fault tests can use `ToxicTestClientCreator` with
the local `backend` proxy. Backend test URLs come from `xmtp_configuration`.
