# xmtp_common

Shared helpers. Owns `#[xmtp_common::test]`, retry, time, rand.

## Commands

```bash
just check crate xmtp_common
just test crate xmtp_common
just test workspace -p xmtp_common --ignore-default-filter bundled_roots_config_is_accepted_by_reqwest   # one test
just test workspace -p xmtp_common http::   # one module
```

## Gotchas

- Every crate depends on it. A change rebuilds the workspace.
- Shared helpers go here. Never copy a helper into another crate.
- `test-utils` is portable. Native Toxiproxy helpers require `test-utils-network`.
