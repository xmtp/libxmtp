# xmtp_api_grpc

gRPC transport for the `xmtp_api` traits.

## Commands

```bash
just check crate xmtp_api_grpc
just test crate xmtp_api_grpc
just test workspace -p xmtp_api_grpc grpc_client::client:: # local header and transport tests
just test workspace -p xmtp_api_grpc does_not_starve_s2   # one test
just test workspace -p xmtp_api_grpc streams::   # one module
```

## Gotchas

- Needs `just backend up` (`backend`, `toxiproxy`).
- `test-utils` pulls `toxiproxy_rust`. Fault tests talk to the `toxiproxy` container.
