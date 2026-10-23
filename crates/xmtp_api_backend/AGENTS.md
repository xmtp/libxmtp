# xmtp_api_backend

Backend endpoints, middleware, decoders, static/bidi transport.
Run `dev/nix-shell 'just test crate xmtp_api_backend'`.

- `MessageBackendBuilder` takes one URL; `xmtp_api` owns retries and request limits.
- Reuse the scripted peer in `test/bidi.rs`. Fault tests use
  `ToxicTestClientCreator` and the worktree backend proxy; get URLs from
  `xmtp_configuration`, never hardcoded ports.
- Auth state changes, including callback completion, take `AuthHandle`'s lock.
  Never retain/log callback error text. Unary/server streams replay once after
  current credential rejection; bidi opens leave reopen policy to transport.
- Auth behavior: [AUTH](../../docs/specs/AUTH-backend-auth.md);
  transport structure: [README](README.md).
