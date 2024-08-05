# Custom Diesel Backend for Wasm wa-sqlite

#### Bundle the javascript in `package.js` to rust

`yarn run build`

#### Build the JS WASM interface

`wasm-pack build`

#### Run the Wasm Tests

wasm-pack test --chrome --headless

# TODO

- [ ] wa-sqlite should be included in `pkg` build w/o manual copy (wasm-pack
      issue?)
- [ ] OPFS

# Notes

- rust-analyzer doesn't like crates with different targets in the same
  workspace. If you want this to work well with your LSP, open
  `diesel-wasm-sqlite` as it's own project.
