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

Static streams use backend frames and scalar topic cursors. The files in
`queries/{bidi,bidi_transport,bidi_transport_props}.rs` and `queries/v3/` stay
until the transport task replaces their frame types. `protocol/mod.rs` contains
only the types that these files still need. Do not use them in new code.

Tests use a mock transport. Fault tests can use `ToxicTestClientCreator` with
the local `backend` proxy. Backend test URLs come from `xmtp_configuration`.
