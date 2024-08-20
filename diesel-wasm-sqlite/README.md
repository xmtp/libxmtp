# Custom Diesel Backend for Wasm wa-sqlite

### Compile rust code without creating a npm package

`cargo build --target wasm32-unknown-unknown`

#### Build the JS WASM interfaces

`wasm-pack build`

#### Run the Wasm Tests

wasm-pack test --chrome

navigate to `http://localhost:8000` to observe test output

(headless tests don't work yet)

# TODO

- [ ] wa-sqlite should be included in `pkg` build w/o manual copy (wasm-pack
      issue?)
- [ ] OPFS

# Setting up the project in VSCode

rust-analyzer does not like crates with different targets in the same workspace.
If you want this to work well with your LSP, open `diesel-wasm-sqlite` as it's
own project in VSCode.
