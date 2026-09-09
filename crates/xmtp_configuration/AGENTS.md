# xmtp_configuration

Constants and tunables. URLs, limits, timeouts.

## Commands

```bash
just check crate xmtp_configuration
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_configuration
dev/nix-shell "cargo nextest run --profile ci -p xmtp_configuration -E 'test(/common::/)'"   # one module
```

## Gotchas

- New constants go here. No magic numbers elsewhere.

## Conventions

- Constants that cross crate boundaries (URLs, page sizes, shared timeouts) live here. A private implementation constant stays in its own module (`crates/xmtp_mls/src/worker.rs:26`, `crates/xmtp_mls/src/groups/change_callbacks.rs:88`). Never copy a shared value locally.
- Layout (`src/lib.rs`): `common/` is always compiled and re-exported. `test/` replaces `prod/` under `cfg(any(test, feature = "test-utils"))`, and both halves must export the same symbol names. Put a new constant in `common/<area>.rs` if one value fits every build, otherwise in both `prod/<area>.rs` and `test/<area>.rs` (example: `prod/api.rs:MAX_PAGE_SIZE` = 100, `test/api.rs:MAX_PAGE_SIZE` = 20).
- Areas: `common/{api,backend,db,metadata,mls,scw,tracing}.rs`. Add a new file plus a `mod` / `pub use` line, not more lines in one file.
- `SCREAMING_SNAKE_CASE` for a free constant. The backend URL is explicit. `DockerUrls::ANVIL` is the local SCW test chain.
- Crate features are `test-utils` and `dev` only. Keep it dependency-light.
