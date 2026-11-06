# xmtp_api

Backend wrapper: retries, limits, paging, result mapping.
Run `dev/nix-shell 'just test crate xmtp_api'`; real RPC tests need the backend,
mock tests use `MockBackendClient`.

- Keep commits/proposals in one unsplittable `PublishUnit` with canonical bytes.
  Use `new_within` / `single_within` and deployment limits from `limits()`;
  default constructors are only for callers without a configuration snapshot.
- Match structured gRPC codes/details, never error text. Preserve ordered inbox
  results, duplicates, missing-key `None`, and per-topic query cursors.
- Every RPC/stream open goes through the guarded adapter's shared preflight hook.
  Never expose raw transport outside tests. `ConfigurationFetch` stays restricted
  and credential-free to avoid recursive admission.
- Preserve typed Auth and Preflight errors through `dyn_err`. Preflight failure
  ends this logical request; retryability permits a new operation, not a transport retry.
