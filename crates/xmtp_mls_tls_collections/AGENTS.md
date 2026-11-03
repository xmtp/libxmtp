# xmtp_mls_tls_collections

`TlsMap` and `TlsSet`: maps and sets with a canonical TLS encoding, and their
deltas. Native and WASM. Run
`dev/nix-shell 'just test crate xmtp_mls_tls_collections'`.

- Hash a key with `TlsKeyHash::of`; `from_bytes` only rebuilds a hash read
  from the wire.
