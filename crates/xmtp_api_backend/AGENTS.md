# xmtp_api_backend

Backend client, endpoints, decoders, middleware, and static streams.

```bash
dev/nix-shell 'cargo check -p xmtp_api_backend'
dev/nix-shell 'cargo test -p xmtp_api_backend'
dev/nix-shell 'cargo clippy -p xmtp_api_backend --all-targets -- -D warnings'
```

`BackendClient<C>` implements the backend unary trait. `MessageBackendBuilder`
requires one host URL and keeps the auth callback, auth handle, app version,
and read-only option. The wrapper in `xmtp_api` owns retries and request limits.

Static streams and bidi streams use backend frames and scalar topic cursors.
`queries/backend/` holds the backend binding and transport implementation.
`queries/{bidi,bidi_transport}.rs` hold the shared transport.
`queries/bidi_transport/tests/` groups transport tests by behavior.
`queries/bidi_transport_props.rs` holds the property tests.
The connection, transport, and property tests share the scripted peer in
`test/bidi.rs`. The legacy protocol and client modules have been removed.

Tests use a mock transport. Fault tests can use `ToxicTestClientCreator` with
the local `backend` proxy. Backend test URLs come from `xmtp_configuration`.
