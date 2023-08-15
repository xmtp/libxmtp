# LibXMTP

![https://github.com/xmtp/libxmtp/actions/workflows/test.yml/badge.svg](https://github.com/xmtp/libxmtp/actions/workflows/test.yml/badge.svg) ![https://github.com/xmtp/libxmtp/actions/workflows/lint.yml/badge.svg](https://github.com/xmtp/libxmtp/actions/workflows/lint.yml/badge.svg) ![Status](https://img.shields.io/badge/Project_status-Alpha-orange)

LibXMTP is a shared library encapsulating the core functionality of the XMTP messaging protocol, such as cryptography, networking, and language bindings.

> **Important**  
> This software is in **alpha** status and ready for you to start experimenting with. However, we do not recommend using alpha software in production apps. Expect frequent changes as we add features and iterate based on feedback.

## Requirements

- Install [Rustup](https://rustup.rs/)
- Install [Docker](https://www.docker.com/get-started/)

## Development

Start Docker Desktop.

- To install other dependencies and start background services:

  ```
  dev/up
  ```

  Specifically, this command creates and runs an XMTP node in Docker Desktop.

  > **Tip**  
  > You can use this local node with the [example CLI](https://github.com/xmtp/libxmtp/blob/main/examples/cli/README.md) to try out sending XMTP v3-alpha double ratchet messages.

- To run tests:

  ```
  dev/test
  ```

## Structure

- [`xmtp`](https://github.com/xmtp/libxmtp/tree/main/xmtp): Pure Rust implementation of XMTP APIs, agnostic to any per-language or per-platform binding
- [`xmtp_cryptography`](https://github.com/xmtp/libxmtp/tree/main/xmtp_cryptography): Cryptographic operations
- [`xmtp_networking`](https://github.com/xmtp/libxmtp/tree/main/xmtp_networking): API client for XMTP's gRPC API, using code from `xmtp_proto`
- [`xmtp_proto`](https://github.com/xmtp/libxmtp/tree/main/xmtp_proto): Generated code for handling XMTP protocol buffers
- [`examples/cli`](https://github.com/xmtp/libxmtp/tree/main/examples/cli): Example XMTP console client
- [`examples/android/xmtpv3_example`](https://github.com/xmtp/libxmtp/tree/main/examples/android/xmtpv3_example): Example Android app (in progress)
- [`bindings_ffi`](https://github.com/xmtp/libxmtp/tree/main/bindings_ffi): FFI bindings for Android and iOS (in progress)
- [`bindings_js`](https://github.com/xmtp/libxmtp/tree/main/bindings_js): JS bindings (in progress)
- [`bindings_wasm`](https://github.com/xmtp/libxmtp/tree/main/bindings_wasm): Wasm bindings (in progress)
