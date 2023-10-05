#!/usr/bin/env bash

set -euo pipefail

rm -rf ./dist
wasm-pack build --target bundler --no-pack --release --out-dir ./dist/browser
wasm-pack build --target nodejs --no-pack --release --out-dir ./dist/node
rm -f ./dist/**/.gitignore