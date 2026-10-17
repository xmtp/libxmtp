# xmtp_content_types

Content type codecs. Text, reaction, reply, attachment, actions.

## Commands

```bash
just check crate xmtp_content_types
just test crate xmtp_content_types
just test workspace -p xmtp_content_types --ignore-default-filter encode_decode_actions   # one test
just test workspace -p xmtp_content_types actions::   # one module
```

## Gotchas

- Pure. No docker.
- A new content type needs matching work in `bindings/{mobile,node,wasm}`.
