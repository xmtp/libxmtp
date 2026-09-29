# xmtp_configuration

Constants and tunables. URLs, limits, timeouts.

## Commands

```bash
just check crate xmtp_configuration
just test crate xmtp_configuration
just test workspace -p xmtp_configuration common::   # one module
```

## Gotchas

- New constants go here. No magic numbers elsewhere.

## Conventions

- Constants that cross crate boundaries (URLs, page sizes, shared timeouts) live here. A private implementation constant stays in its own module (`crates/xmtp_mls/src/worker.rs:26`, `crates/xmtp_mls/src/groups/change_callbacks.rs:88`). Never copy a shared value locally.
- Layout (`src/lib.rs`): `common/` is always compiled and re-exported. `test/` replaces `prod/` under `cfg(any(test, feature = "test-utils"))`, and both halves must export the same symbol names. Put a new constant in `common/<area>.rs` if one value fits every build, otherwise in both `prod/<area>.rs` and `test/<area>.rs` (example: `prod/mls.rs:KEYS_EXPIRATION_INTERVAL_NS` = 1 day, `test/mls.rs:KEYS_EXPIRATION_INTERVAL_NS` = 3 seconds).
- Areas: `common/{api,attachments,backend,db,metadata,mls,scw,server,streams,tracing}.rs`. Add a new file plus a `mod` / `pub use` line, not more lines in one file.
- `common/server.rs` holds `ServerConfiguration`, the `ConfigProvider` trait, and
  the two providers. These are the values one backend deployment publishes
  (spec 006), not compile-time constants, but every field falls back to a
  `BACKEND_DEFAULT_*` constant in this crate.
- `SCREAMING_SNAKE_CASE` for a free constant. A value that differs per worktree is a function, not a constant: `backend_test_url()`, `backend_test_toxic_url()`, and `DockerUrls::anvil()` read the environment and fall back to a `*_DEFAULT` constant. Native reads at run time; wasm uses `option_env!`, so the value is baked in at build time.
- Crate features are `test-utils` and `dev` only. Keep it dependency-light.
