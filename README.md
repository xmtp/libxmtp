# XMTP <> Vodozemac Development Branch

> :warning: **Under Construction**: This code is WIP and should not be used in any real-world context

This repo contains Rust crates, platform-bindings and examples needed to build a new XMTP protocol using [vodozemac](https://github.com/matrix-org/vodozemac)

## Prerequisites

- Go get Rust: [website](https://www.rust-lang.org/tools/install) or [script install](https://doc.rust-lang.org/cargo/getting-started/installation.html)
- For Android: [Android Studio](https://developer.android.com/studio)
- For wasm and Node you need [npm](https://www.npmjs.com/)
- You need [Docker](https://www.docker.com/) for cross-compilation along with [cross-rs](https://github.com/cross-rs/cross)

## Structure

Top-level
- crates - contains all business logic Rust code for our v-mac based protocol
 - xmtpv3 - consumes vodozemac as a dependency and currently surfaces a single function: `e2e_selftest`
- bindings - contains various types of platform bindings aka how `xmtpv3` turns into stuff that apps can use
 - wasm - builds a lightweight layer around xmtpv3 in a wasm binding, contains tests and an example webapp
 - ffi - uses Mozilla [uniffi](https://github.com/mozilla/uniffi-rs) to create native libraries with FFI bindings
   - Currently supports Android via Kotlin

## Bindings

All the variations of bindings have their own Rust crate with some wrapper code. So the normal structure is:

`binding crate` depends on `xmtpv3` (via path dependency) and platform/binding-specific logic links this to the non-Rust part of the binding.

The development flow will most commonly be:
1. Make a change in `xmtpv3`
2. Write unit tests for those changes in `xmtpv3/src`
3. Make a change in the binding crate you're working in e.g. `bindings/ffi/src/lib.rs`
4. Write unit tests for those changes in the binding Rust crate
5. Finally, run it end-to-end.

Note you can write unit tests in `xmptv3` and the binding crate.

## xmtpv3 quickstart

- cd `crates/xmtpv3` then utter `cargo test`, this installs dependencies, builds and runs the unit tests.

The future idea is that `lib.rs` will expose our higher level messaging API, built on top of v-mac.

## WASM QuickStart

- cd `bindings/wasm`
- Run `npm install`
- Run `npm run build` to build the rust crate and Node.js bindings.
- Run `npm run test` to build the xmtpv3 crate, the wasm bindings crate and run against Node.js tests

NOTE: currently broken due to thread-rng dependency in Rust. You should see a "panic unreachable" message.

## Running in browser

- cd `bindings/wasm/example_web`
- In one process, run `./run_server.sh`

NOTE: currently broken due to thread-rng dependency in Rust

## Android Quickstart

- cd `bindings/ffi` - read the [README](./bindings/ffi/README.md)

To build libraries from scratch:

- `./cross_build.sh` - currently does a release profile (smaller libraries) and uses cross for cross compilation. This is SLOW (5 minutes).
- Follow the instructions and open [./examples/xmtpv3_example](./examples/xmtpv3_example) in Android Studio (open the build.gradle at the root)
- You may need to install an emulator via AVD
- Hit Run to install the app on the emulator and see the self-test output
