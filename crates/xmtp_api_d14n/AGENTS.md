# xmtp_api_d14n

xmtpd (d14n) API client. Slated for deletion this project.

## Commands

```bash
just check crate xmtp_api_d14n
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_api_d14n
just test v3 -p xmtp_api_d14n --ignore-default-filter test_forwards_to_inner   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp_api_d14n -E 'test(/middleware::/)'"   # one module
```

## Gotchas

- Needs `just backend up` (`xmtpd`, `gateway`).
- Delete-only. No new features.
