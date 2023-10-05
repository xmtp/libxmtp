# ECIES Bindings WASM

A small package wrapping the XMTP ECIES library using [wasm-pack](https://rustwasm.github.io/docs/wasm-pack/introduction.html)

## Publishing

This package uses [Changesets](https://github.com/changesets/changesets) for publishing. If you modify this file or it's dependencies, run `npx changeset` in this folder to create a new Changeset.

## Build

Run `npm run build` to use wasm-pack to build the artifacts for the package. This will create two versions of the package, one targeting Node.js and one targeting the browser. Consumers should be able to use the correct version by reading the `browser` or `main` field of the package.json.
