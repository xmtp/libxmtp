# bindings_node

NAPI-RS bindings for Node. API tests are TypeScript (`test/*.test.ts`). Error conversion also has a Rust unit test.

## Commands

```bash
just install-js                         # install the root workspace once
just node check                         # build release NAPI to dist/
just node lint                          # clippy + rustfmt
just node test                          # install + build with test-utils + vitest
just node test-ci                       # what CI runs (Nix build)
```

## Gotchas

- Needs `just backend up`. Tests use `XMTP_BACKEND_URL` or `http://127.0.0.1:5050`.
  The helper uses the IPv4 address on purpose: `localhost` can resolve to IPv6,
  where the Docker port forward resets the connection.
- Tests import `../dist`. Run `just node test` once before a single-file run.
- Run one Vitest process at a time. Its teardown deletes all test database files in `test/`.
- `test-ci` makes the copied Nix output writable so later runs can replace it.
- `check` builds `--release`. `test` rebuilds with `--features test-utils`. Each switch is a full rebuild.

## Conventions

- `src/lib.rs:ErrorWrapper<E: ErrorCode>` maps errors to `napi::Error` with a
  stable code. Structured stream failures use the shared
  `[XMTP_STREAM_FAILURE_V1]` suffix; the JS SDK reads it with
  `getStreamFailureDetails`.
- Exporting: `#[napi]`, `#[napi(object)]`, `#[napi(getter)]`, `#[napi(string_enum)]`, `#[napi(js_name = "...")]`. `pub async fn` becomes a Promise. Add `#[xmtp_common::err_span]` to exported methods (`src/client/mod.rs:54`).
- Builders use `#[xmtp_macro::napi_builder]`. Run `just node build` to
  regenerate `dist/`; never hand-edit it.

## Durable message readers

- Call `checkOwner` before app handoff. Use `enrichedMessage` to read the
  pending item; a null result requires reselection. Codec errors are terminal.
- `close` releases the default owner. `beginningDeliveryCursor` starts replay
  from the first retained item.

Run the error conversion test without a Node runtime:
`just test workspace -p bindings_node auth_codes_reach_node_errors`.
The N-API test dependency enables `dyn-symbols` and `noop` so Rust test binaries
can run without Node. Normal addon builds do not enable these test features.
