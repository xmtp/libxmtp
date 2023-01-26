
# Wasmpkg

Wasmpkg is a template configuration to bootstrap new rust projects easily.

## QuickStart

## Project Structure

`crate/src/` -- Location for rust specific code.
`src/pkg` -- Compiled rust code.
`src/wasmpkg.ts` -- This is thin wrapper around the calls to wasm.

## Slim Module

By default, the `Wasmpkg` entrypoint includes Wasm that is base64 inlined. This allows developers consuming this library to be unaware that wasm is used, and no extra steps are required.
Developers seeking more control can use `wasmpkg/slim` package that operates the exactly the same except wasm can now be supplied at runtime.

```js
import { Wasmpkg } from "wasmpkg/slim";
import wasm from "wasmpkg.wasm";

const parser = await Wasmpkg.initialize({ wasm });

```
