# xmtp_archive

Run `dev/nix-shell 'just test crate xmtp_archive'`.

- Preserve old archive readability; see [ARCH](../../docs/specs/ARCH-archive-format.md).
- Export uses one read transaction. Never hold it across await or let its sink
  use the database.
