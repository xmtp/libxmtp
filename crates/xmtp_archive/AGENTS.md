# xmtp_archive

Archive import and export.

## Commands

```bash
just check crate xmtp_archive
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_archive
just test v3 -p xmtp_archive --ignore-default-filter test_generic_array_ext   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp_archive -E 'test(/util::/)'"   # one module
```

## Gotchas

- On-disk format is versioned. Old archives must still load.
