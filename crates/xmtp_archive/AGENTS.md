# xmtp_archive

Archive import and export.

## Commands

```bash
just check crate xmtp_archive
just test crate xmtp_archive
just test workspace -p xmtp_archive --ignore-default-filter test_generic_array_ext   # one test
just test workspace -p xmtp_archive util::   # one module
```

## Gotchas

- On-disk format is versioned. Old archives must still load.
