# XMTP SDK façade

Shared public surface for every platform. Put behavior here or in core; platform
SDKs wrap it. The generator lives in `apps/xmtp_sdk_bindgen`.

```sh
dev/nix-shell 'just sdk generate [targets]'
dev/nix-shell 'just sdk lint' # requires generated default targets
dev/nix-shell 'just test crate xmtp_sdk' # needs backend
dev/nix-shell 'just sdk check-package-scripts' # packaging changes
```

- Export with `#[xmtp_macro::sdk_export]`; on records/enums put it above derives.
  Read [marker rules and hand-maintained areas](../../apps/xmtp_sdk_bindgen/README.md)
  when adding exports, events, identity routes, or redaction.
- Synchronous getters need `#[sdk(immutable)]`; mutable values are async.
- Async secret arguments use `SecretBytes` / `SecretString`, not bare buffers,
  so cancellation before first poll still wipes them. Redacted types must not
  also derive `Debug`.
- Keep named `include!` files for exported content/conversation types: their
  UniFFI module paths are stable. Use ordinary modules for private helpers.
- Keep test hooks in test source sets. Source files must stay below 1,000 lines;
  aim below 500.
- Generated bindings, runtimes, and libraries must be a matched set. Read
  [generation/package checks](DEVELOPMENT.md) when changing build routes,
  staging, receipts, platform proofs, or dependency inputs. Host debug tests
  do not replace release/runtime proofs (release panics abort).
- Stream terminality, fencing, or replacement changes require the
  [Node recovery budget tests](../../sdks/node/TESTING.md) before push.
