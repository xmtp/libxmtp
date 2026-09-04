# xmtp_mls

Core client. Groups, messages, sync, streams.

## Commands

```bash
just check crate xmtp_mls
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_mls                # needs `just backend up` (anvil)
just test v3 -p xmtp_mls --ignore-default-filter test_valid_deletion_by_sender   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp_mls -E 'test(/messages::/)'"   # one module
```

## Gotchas

- Needs `just backend up`.
- `just test crate` = nextest `default` profile. `just test v3` = `ci` profile (CI). Both skip flaky stream tests. `--ignore-default-filter` runs them.
- `just test d14n <test_name>` = d14n lane. Builds with `--features d14n`. Separate artifacts, slow.
