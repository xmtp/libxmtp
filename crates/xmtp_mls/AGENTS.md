# xmtp_mls

The client facade: `Client`, `ClientBuilder`, `ClientError`, `MlsFeatures`,
the task handlers, `tester!` and the prelude. It re-exports the lower
`xmtp_mls_*` crates at their old paths (`groups`, `subscriptions`,
`worker::device_sync`, `client::notifications`); change their code in
those crates.

```sh
dev/nix-shell 'just backend up'
dev/nix-shell 'just test crate xmtp_mls'
dev/nix-shell 'just test workspace -p xmtp_mls <filter>'
```

- Use `id::calculate_message_id` / `calculate_message_id_for_intent`; never
  reimplement message hashing. Move shared functionality below this crate if
  the backend needs it.
- Use `tester!` / `TesterBuilder` for clients. Custom configuration and ephemeral
  backends: [fixtures and admission rules](TESTING.md).
- Derive `ErrorCode` when a stable code crosses FFI; preserve typed inner errors
  and delegate `RetryableError` explicitly.
- Proxy tests need a separate nextest invocation with `--test-threads 1` and no
  concurrent proxy test process.
- After fixture dependency/environment changes, run `dev/check-ephemeral-backend`.
- Before pushing stream terminality, fencing, or replacement changes, run the
  [Node recovery budget tests](../../sdks/node/TESTING.md).
