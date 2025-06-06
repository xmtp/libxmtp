export RUSTFLAGS="-Ctarget-feature=+bulk-memory,+mutable-globals,+atomics --cfg getrandom_backend=\"wasm_js\"${RUSTFLAGS:=}"

cd worker
WASM_BINDGEN_SPLIT_LINKED_MODULES=1 wasm-pack build --target web --out-dir ./dist --no-opt --release
cd ..
cd lib
WASM_BINDGEN_SPLIT_LINKED_MODULES=1 wasm-pack build --target web --out-dir ./dist --no-opt --release
