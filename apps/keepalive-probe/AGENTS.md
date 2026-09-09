# keepalive-probe

CLI to hold gRPC connections and measure keepalive survival.

## Commands

```bash
just check crate keepalive-probe
dev/nix-shell 'cargo clippy --locked -p keepalive-probe --all-targets -- -D warnings'
just test crate keepalive-probe
dev/nix-shell 'cargo run -p keepalive-probe -- --help'
dev/nix-shell 'cargo run -p keepalive-probe -- --endpoint http://127.0.0.1:5050 --duration 35s --subscribe-group 00000000000000000000000000000000'
```

## Gotchas

- `--endpoint` selects the server. The other CLI options and output format stay the same.
- Subscription mode uses `xmtp.backend.v1.SubscriptionService/Subscribe`.
- The probe adds the group topic after `Started` and replies to server `Ping` frames.
- Only message envelopes count as received payloads.
- Not in `default-members`. Root `just check`, `just test`, and `just lint-rust` skip this crate.
