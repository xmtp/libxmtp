# Crate, module, and binding conventions

## Crates

- New crates go in `crates/`, binaries in `apps/`. Root `Cargo.toml` globs both.
- The package header inherits `license`, `version`, `rust-version`, `edition`,
  and `publish` from the workspace, then `[lints] workspace = true`. Copy
  `crates/xmtp_mls_common/Cargo.toml`.
- Dependencies: `name.workspace = true` or `{ workspace = true, features = [..] }`,
  pinned once in `[workspace.dependencies]`. A crate-local version needs a
  reason: target-specific or single-consumer.
- After any dependency change run `dev/nix-shell 'cargo hakari generate'` and
  `dev/nix-shell 'cargo hakari manage-deps'`. `just lint-rust` checks both.
- Every package has an `AGENTS.md`, with `CLAUDE.md` holding `@AGENTS.md`.
  Update it in the PR that changes its build or test commands.
- Versions come from files: `rust-toolchain.toml`, root `Cargo.toml`
  (`rust-version`, `edition`), `rustfmt.toml`, `.clippy.toml`.

## Modules

- Prefer `foo.rs` plus a `foo/` sibling directory to `foo/mod.rs`
  (`crates/xmtp_common/src/test.rs` + `test/`).
- Files under 1,000 lines; aim for 500. Move a large test suite to a
  module-local `tests.rs` or a `tests/` directory split by behaviour.
- Keep internals `pub(crate)`. Export a type only when a binding or another
  crate needs it. Large crates expose a `prelude` of query traits.
- Import from a crate root only what it re-exports. `xmtp_common` re-exports
  `retry`, `wasm`, `stream_handles`, `const`, `event_logging`, `hash`, `rand`;
  `time`, `fmt`, `hex`, `http`, `snippet`, `types`, `rate_limit` need the path.

## Features

Reuse the established name for an established capability: `test-utils`,
`bench`, `dev`, `sentry`, `metrics`. Narrower ones exist (`diesel`,
`test-utils-network`, `test-utils-anvil`, `grpc_client_impls`,
`update-schema`). A new capability may take a new name, never a synonym for an
old one.

`#[cfg(test)]` (or `if_only_test!`) for helpers only this crate's tests use.
`#[cfg(any(test, feature = "test-utils"))]` (or `if_test!`) when another crate
imports them. A crate that exposes test helpers forwards the feature:

```toml
# crates/xmtp_mls/Cargo.toml, abridged
[features]
test-utils = ["xmtp_db/test-utils", "xmtp_api/test-utils", "xmtp_common/test-utils-network", "dep:rstest"]
```

## RustDoc

Important functions in the backend and shared crates get `///` docs, including
private ones with invariants: purpose, non-obvious choices, caller obligations,
failure and cancellation behaviour. Document a return value when the type does
not explain it. `//` is for local notes, not a substitute. No requirement ids
in comments.

## Bindings

A binding is a thin translation layer. Business logic belongs in `xmtp_mls` or
a shared crate.

- Errors: the surface's wrapper (see [errors.md](errors.md)). In wasm use it so
  the JavaScript error keeps its `code` property.
- Names: node and wasm use bare, identical names (`Client`, `Conversation`,
  `BackendBuilder`) so the two JS SDKs stay symmetric. Mobile uses `Ffi*`.
- Builders: `#[xmtp_macro::napi_builder]`, `wasm_builder`, and
  `uniffi_builder` generate `new()` and setters from
  `#[builder(required | optional | default = ".." | skip)]`. An unannotated
  `Option<T>` is `optional`. `build()` is always hand-written
  (`bindings/wasm/src/client/backend.rs`).
- FFI entry points get `#[xmtp_common::err_span]`. Native ones call
  `install_crypto_provider()` first.
- `dist/` and `src/gen/**` are build products. Regenerate with the commands in
  the binding's `AGENTS.md`; never hand-edit.
- When a Ref plan is required, include new or changed public surface in it
  (root `AGENTS.md`).
