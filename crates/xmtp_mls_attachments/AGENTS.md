# xmtp_mls_attachments

The pending remote attachment runtime: uploads, downloads and their local
files, reconciliation, and the cleanup worker. Depends on `xmtp_mls_core`
and `xmtp_attachments`; names no engine or facade type. Native and WASM.
`xmtp_mls` re-exports it as `attachments`.

```sh
dev/nix-shell 'just backend up'
dev/nix-shell 'just test crate xmtp_mls_attachments'
```

- The `attachments` target builds real clients through the `xmtp_mls`
  dev-dependency, which also enables this crate's `test-utils` and
  `test-hooks`, so the default command builds it. It reaches the runtime
  through the doc-hidden `test_support` and `test_hooks` modules.
- `attachment_memory` measures transfer allocations natively, and
  `attachment_opfs` runs the browser OPFS tests in their own binary.
  Unit tests in `src/tests.rs` need no client.
- `test-hooks` changes runtime behavior; enable it only from tests.
  `xmtp_mls` forwards `test-utils` and `test-hooks`.
