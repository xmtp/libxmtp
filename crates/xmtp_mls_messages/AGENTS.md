# xmtp_mls_messages

Decodes stored group messages into `DecodedMessage` / `MessageBody` and
enriches them; no client, network or backend. Native and WASM.
`xmtp_mls` re-exports it as `xmtp_mls::messages`.

```sh
dev/nix-shell 'just test crate xmtp_mls_messages'
```

- The `conformance` feature counts decoded bodies for the SDK conformance
  suite; `xmtp_mls` and `xmtp_sdk` forward it.
