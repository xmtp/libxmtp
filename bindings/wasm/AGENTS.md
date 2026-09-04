# bindings_wasm

wasm-bindgen bindings for browsers. Feeds `sdks/js/browser-sdk`.

## Commands

```bash
just wasm check
just wasm lint                          # clippy + rustfmt + prettier
just wasm build                         # nix build .#wasm-bindings
just wasm test                          # Rust tests on wasm32, v3 + d14n   # unverified
just wasm test-v3 backoff_retry             # one test
just wasm test-integration              # TypeScript tests in test/   # unverified
just wasm test-ci                       # what CI runs (Nix build)   # unverified
```

## Gotchas

- Uses `NIX_DEVSHELL=wasm`. Needs `just backend up`.
- `just wasm test` runs a fixed crate list: `wasm_packages` in `wasm.just`. Add a crate there to test it on wasm.
- No threads, no filesystem, no `std::time`. Use `xmtp_common` time and rand helpers.
