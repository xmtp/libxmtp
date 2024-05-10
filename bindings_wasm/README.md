# [Wasm](https://webassembly.org/) bindings for XMTP v3

# WARNING: DO NOT USE FOR PRODUCTION XMTP CLIENTS

This code is still under development.

## Build for Node.js 

    cd bindings_wasm
    wasm-pack test --node
    wasm-pack build --target nodejs

## Build for the web

    cd bindings_wasm
    wasm-pack test --headless --chrome
    wasm-pack build

## Test the WASM bindings from Javascript (Node.js)

    cd bindings_wasm
    node index.js
