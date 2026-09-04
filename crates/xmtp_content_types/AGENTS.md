# xmtp_content_types

Content type codecs. Text, reaction, reply, attachment, actions.

## Commands

```bash
just check crate xmtp_content_types
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_content_types
just test v3 -p xmtp_content_types --ignore-default-filter encode_decode_actions   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp_content_types -E 'test(/actions::/)'"   # one module
```

## Gotchas

- Pure. No docker.
- A new content type needs matching work in `bindings/{mobile,node,wasm}`.
