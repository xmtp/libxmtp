# bindings_node

NAPI-RS bindings for Node. API tests are TypeScript (`test/*.test.ts`). Error conversion also has a Rust unit test.

## Commands

```bash
just node check                         # install + build release NAPI to dist/
just node lint                          # prettier
dev/nix-shell 'cd bindings/node && yarn lint'   # clippy + rustfmt
just node test                          # install + build with test-utils + vitest
just node test-ci                       # what CI runs (Nix build)
dev/nix-shell 'cd bindings/node && yarn vitest run test/inboxId.test.ts'           # one file
dev/nix-shell 'cd bindings/node && yarn vitest run -t "should generate an inbox id"'   # one test
```

## Gotchas

- Needs `just backend up`. Tests use `XMTP_BACKEND_URL` or `http://127.0.0.1:5050`.
  The helper uses the IPv4 address on purpose: `localhost` can resolve to IPv6,
  where the Docker port forward resets the connection.
- Tests import `../dist`. Run `just node test` once before a single-file run.
- `test-ci` makes the copied Nix output writable so later runs can replace it.
- `check` builds `--release`. `test` rebuilds with `--features test-utils`. Each switch is a full rebuild.

## Conventions

A binding is a thin translation layer. Business logic belongs in `xmtp_mls` or a shared crate.

- Errors: `src/lib.rs:ErrorWrapper<E: ErrorCode>` maps to `napi::Error::from_reason("[{code}] {msg}")`. Call sites use `.map_err(ErrorWrapper::from)?` (`src/client/backend.rs:88`). Do not write a new conversion for an error that has an `ErrorCode`.
- Naming: bare names, deliberately identical to `bindings/wasm` (`Client`, `Conversation`, `BackendBuilder`) so the two JS SDKs stay symmetric. Pick the same name on both.
- Exporting: `#[napi]`, `#[napi(object)]`, `#[napi(getter)]`, `#[napi(string_enum)]`, `#[napi(js_name = "...")]`. `pub async fn` becomes a Promise. Add `#[xmtp_common::err_span]` to exported methods (`src/client/mod.rs:54`).
- Builders: `#[xmtp_macro::napi_builder]` (`src/client/backend.rs:9`). Field attributes: `#[builder(required)]`, `#[builder(optional)]`, `#[builder(default = "expr")]`, `#[builder(skip)]`. `build()` is always hand-written (`crates/xmtp_macro/src/builders.rs`).
- Regeneration: `just node build` (`yarn napi build --platform --esm`, then `node.just:_prepare-dist` moves output to `dist/`). `dist/` is a build product. Never hand-edit it.

Auth callback bridges return only `auth callback failed` on failure. Never retain
or log callback error text or credential values. The middleware owns retryability.

Run the error conversion test without a Node runtime:
`dev/nix-shell 'cargo nextest run --profile ci -p bindings_node --features napi/dyn-symbols,napi/noop auth_codes_reach_node_errors'`.
