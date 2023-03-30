# XMTPv3 node-bindgen bindings

Using [node-bindgen](https://github.com/infinyon/node-bindgen)

## Setup

- `cargo install nj-cli`

## Structure

- `crates` (contains Rust crate with node-bindgen bindings)
- `run.sh` - minimal script that builds `dist` and runs app.js
- `app.js` - runs `app.js` which imports `dist`

## Build and run app.js

- `./run.sh`
