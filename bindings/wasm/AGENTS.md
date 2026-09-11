# bindings_wasm

wasm-bindgen bindings for browsers. Feeds `sdks/js/browser-sdk`.

## Commands

```bash
just wasm check
just wasm lint                          # clippy + rustfmt + prettier
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

A binding is a thin translation layer. Business logic belongs in `xmtp_mls` or a shared crate.

- Errors: `src/errors.rs:ErrorWrapper` maps to `JsError` with `"[{code}] {msg}"` and a real `code` property set via `js_sys::Reflect::set`. Use `ErrorWrapper::js(e)`, and `errors.rs:to_value` for serde payloads (BigInt-safe). `src/client/backend.rs` wraps builder errors with `BackendBuilderError` to keep a stable code.
- Structured stream failures append the shared `[XMTP_STREAM_FAILURE_V1]` JSON suffix. This suffix survives worker transfers. Use the core encoder. The JS SDK exposes `getStreamFailureDetails` to read all topic obligations.
- Naming: bare names, deliberately identical to `bindings/node` (`Client`, `Conversation`, `BackendBuilder`). Pick the same name on both.
- Exporting: `#[wasm_bindgen]`, `#[wasm_bindgen(js_name = camelCase)]`, `#[wasm_bindgen(constructor)]`, and `#[wasm_bindgen_numbered_enum]` from `bindings_wasm_macros` (`crates/wasm_macros`). `async fn` becomes a Promise.
- Builders: `#[xmtp_macro::wasm_builder]` (`src/client/backend.rs:7`). Field attributes: `#[builder(required)]`, `#[builder(optional)]`, `#[builder(default = "expr")]`, `#[builder(skip)]`. `build()` is always hand-written (`crates/xmtp_macro/src/builders.rs`).
- Regeneration: `just wasm build` (`nix build .#wasm-bindings`) runs `wasm-pack build --target web --out-dir ./dist` (`package.json`, `nix/package/wasm.nix:99`). `dist/` is a build product. Never hand-edit it.

## Durable message readers

- `messageReader` returns one message and one opaque acknowledgement token.
- Keep each token in the worker. Posting a message to the app does not acknowledge it.
- Call `checkOwner` before the app callback. A false result requires a new read without acknowledgement.
- A callback acknowledges after the app returns. An iterator acknowledges when the app requests the next item.
- `close` releases the default owner. Do not call `free` while an asynchronous reader method holds a borrow.
- An explicit `DeliveryCursor` starts replay without changing default delivery progress.
- `messageHistorySnapshot` returns history and its cursor from one database snapshot. `beginningDeliveryCursor` starts replay from the first retained item.
