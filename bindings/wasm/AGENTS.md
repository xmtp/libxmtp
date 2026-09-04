# bindings_wasm

wasm-bindgen bindings for browsers. Feeds `sdks/js/browser-sdk`.

## Commands

```bash
just wasm check
just wasm lint                          # clippy + rustfmt + prettier
just wasm build                         # nix build .#wasm-bindings
just wasm test                          # Rust tests on wasm32, v3 + d14n. Needs `just backend up`
just wasm test-v3 backoff_retry             # one test
just wasm test-integration              # TypeScript tests in test/   # unverified
just wasm test-ci                       # what CI runs (Nix build)   # unverified
```

## Gotchas

- Uses `NIX_DEVSHELL=wasm`. Needs `just backend up`.
- `just wasm test` runs a fixed crate list: `wasm_packages` in `wasm.just`. Add a crate there to test it on wasm.
- No threads, no filesystem, no `std::time`. Use `xmtp_common` time and rand helpers.

## Conventions

A binding is a thin translation layer. Business logic belongs in `xmtp_mls` or a shared crate.

- Errors: `src/errors.rs:ErrorWrapper` maps to `JsError` with `"[{code}] {msg}"` and a real `code` property set via `js_sys::Reflect::set`. Use `ErrorWrapper::js(e)`, and `errors.rs:to_value` for serde payloads (BigInt-safe). Known gap: `src/client/backend.rs:59` builds a plain `JsError` and drops the code. Do not copy that line.
- Naming: bare names, deliberately identical to `bindings/node` (`Client`, `Conversation`, `BackendBuilder`). Pick the same name on both.
- Exporting: `#[wasm_bindgen]`, `#[wasm_bindgen(js_name = camelCase)]`, `#[wasm_bindgen(constructor)]`, and `#[wasm_bindgen_numbered_enum]` from `bindings_wasm_macros` (`crates/wasm_macros`). `async fn` becomes a Promise.
- Builders: `#[xmtp_macro::wasm_builder]` (`src/client/backend.rs:7`). Field attributes: `#[builder(required)]`, `#[builder(optional)]`, `#[builder(default = "expr")]`, `#[builder(skip)]`. `build()` is always hand-written (`crates/xmtp_macro/src/builders.rs`).
- Regeneration: `just wasm build` (`nix build .#wasm-bindings`) runs `wasm-pack build --target web --out-dir ./dist` (`package.json`, `nix/package/wasm.nix:99`). `dist/` is a build product. Never hand-edit it.
