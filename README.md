# LibXMTP

![https://github.com/xmtp/libxmtp/actions/workflows/test.yml/badge.svg](https://github.com/xmtp/libxmtp/actions/workflows/test.yml/badge.svg)
![https://github.com/xmtp/libxmtp/actions/workflows/lint.yml/badge.svg](https://github.com/xmtp/libxmtp/actions/workflows/lint.yml/badge.svg)
![Status](https://img.shields.io/badge/Project_status-Alpha-orange)

LibXMTP is a shared library encapsulating the core functionality of the XMTP
messaging protocol, such as cryptography, networking, and language bindings.

## Requirements

- Install [Rustup](https://rustup.rs/)
- Install [Docker](https://www.docker.com/get-started/)
- Install
  [Foundry](https://book.getfoundry.sh/getting-started/installation#using-foundryup)

## Development

Start Docker Desktop.

- To install other dependencies and start background services:

  ```
  dev/up
  ```

  Specifically, this command creates and runs an XMTP node in Docker Desktop.

- To run tests:

  ```
  dev/test
  ```

- To run WebAssembly tests headless:
  ```
  dev/test-wasm
  ```
- To run WebAssembly tests interactively for a package, for example, `xmtp_mls`:
  ```
  dev/test-wasm-interactive xmtp_mls
  ```

## Quick Start (Dev Containers)

This project supports containerized development. From Visual Studio Code Dev
Containers extension specify the Dockerfile as the target:

`Reopen in Container`

or

Command line build using docker

```bash
docker build . -t libxmtp:1
```

## Structure

libxmtp/

├ [`bindings_ffi`](./bindings_ffi): FFI bindings for Android and iOS (in
progress)

├ [`bindings_wasm`](./bindings_wasm): Wasm bindings (in progress)

├ examples/

│ ├ [`android/xmtpv3_example`](./examples/android/xmtpv3_example): Example
Android app (in progress)

│ └ [`cli`](./examples/cli): Example XMTP console client. Use the CLI to try out
sending double ratchet messages on the XMTP `dev` network.

├ [`xmtp_api_grpc`](./xmtp_api_grpc): API client for XMTP's gRPC API, using code
from `xmtp_proto`

├ [`xmtp_api_http`](./xmtp_api_http): API client for XMTP's gRPC Gateway API,
using code from `xmtp_proto`

├ [`xmtp_cryptography`](./xmtp_cryptography): Cryptographic operations

├ [`xmtp_mls`](./xmtp_mls): Version 3 of XMTP which implements
[Messaging Layer Security](https://messaginglayersecurity.rocks/).

├ [`xmtp_proto`](./xmtp_proto): Generated code for handling XMTP protocol
buffers

### Run the benchmarks

**possible benchmarks include:**

- `group_limit`: benchmarks surrounding maximum members adding/removed from
  group
- `crypto`: benchmarks surrounding cryptographic functions

**Example Commands**

- **Run a specific category of benchmark**
  `cargo bench --features bench -p xmtp_mls --bench group_limit`
- **Run against dev grpc** DEV_GRPC=1 cargo bench --features bench -p xmtp_mls
  --bench group_limit
- **Just run all benchmarks** ./dev/bench
- **Run one specific benchmark** ./dev/bench add_1_member_to_group
- **Generate flamegraph from one benchmark** ./dev/flamegraph
  add_1_member_to_group

## Contributing

See our [contribution guide](./CONTRIBUTING.md) to learn more about contributing to this project.
