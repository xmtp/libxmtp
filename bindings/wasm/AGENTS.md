# bindings_wasm

wasm-bindgen bindings for browsers. Feeds `sdks/browser`.

## Commands

```bash
just wasm check
just wasm lint                          # clippy + rustfmt
just wasm build                         # nix build .#wasm-bindings
just wasm test                          # Rust tests on wasm32. Needs `just backend up`
just wasm test backoff_retry             # one test
just wasm test-integration              # TypeScript tests in test/. Needs `just backend up`
just wasm test-ci                       # what CI runs (Nix build). Needs `just backend up`
```

## Gotchas

- Uses `NIX_DEVSHELL=wasm`. Needs `just backend up`. gRPC-Web uses the backend listener on port 5050.
- `test-integration` makes the copied Nix output writable so later runs can replace it.
- `just wasm test` runs a fixed crate list: `wasm_packages` in `wasm.just`. Add a crate there to test it on wasm.
- No threads, no filesystem, no `std::time`. Use `xmtp_common` time and rand helpers.

## Conventions

- `src/errors.rs:ErrorWrapper` maps to `JsError` with a stable `code` property.
  Use `ErrorWrapper::js(e)` and `errors.rs:to_value` for BigInt-safe payloads.
  The shared stream failure suffix survives worker transfers.
- Exporting: `#[wasm_bindgen]`, `#[wasm_bindgen(js_name = camelCase)]`, `#[wasm_bindgen(constructor)]`, and `#[wasm_bindgen_numbered_enum]` from `bindings_wasm_macros` (`crates/wasm_macros`). `async fn` becomes a Promise.
- Builders use `#[xmtp_macro::wasm_builder]`. Run `just wasm build` to
  regenerate `dist/`; never hand-edit it.

## Durable message readers

- Keep tokens in the worker. Call `checkOwner` before app handoff. Use
  `enrichedMessage` for the pending item; a null result requires reselection.
  Codec errors are terminal.
- `close` releases the default owner. Do not call `free` while an async reader
  method holds a borrow. `beginningDeliveryCursor` starts replay at the first
  retained item.
