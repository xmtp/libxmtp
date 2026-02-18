[![Lint](https://github.com/xmtp/libxmtp/actions/workflows/lint.yml/badge.svg)](https://github.com/xmtp/libxmtp/actions/workflows/lint.yml)
[![Test](https://github.com/xmtp/libxmtp/actions/workflows/test.yml/badge.svg)](https://github.com/xmtp/libxmtp/actions/workflows/test.yml)
[![built with garnix](https://img.shields.io/endpoint.svg?url=https%3A%2F%2Fgarnix.io%2Fapi%2Fbadges%2Fxmtp%2Flibxmtp%3Fbranch%3Dmain)](https://garnix.io/repo/xmtp/libxmtp)
![Status](https://img.shields.io/badge/Project_status-Alpha-orange)

<!-- LOGO -->
<h1>
<p align="center">
  <img src="https://raw.githubusercontent.com/xmtp/brand/1bf5822708c9ce7e06964b85121093d69b3a4ff2/assets/postmark-outlined-color.svg" alt="Logo" width="128">
  <br>libXMTP
</h1>
  <p align="center">
    shared library encapsulating the core functionality of the XMTP messaging
    protocol, such as cryptography, networking, and language bindings.
    <br />
    <a href="https://docs.xmtp.org/">Documentation</a>
    ·
    <a href="CONTRIBUTING.md">Contributing</a>
  </p>
</p>

## Requirements

- Install [Rustup](https://rustup.rs/)
- Install [Docker](https://www.docker.com/get-started/)
- Install
  [Foundry](https://book.getfoundry.sh/getting-started/installation#using-foundryup)

## Development

Adding Dependencies

- adding dependencies will require re-generating the `workspace-hack` crate,
  which can be done with:

```bash
nix develop --command cargo hakari generate
```

to verify correctness you can optionally run

```bash
nix develop --command cargo hakari verify
```

Start Docker Desktop.

- To install other dependencies and start background services:

  ```bash
  dev/up
  ```

  Specifically, this command creates and runs an XMTP node in Docker Desktop.

- This project uses [`just`](https://github.com/casey/just) as a command
  runner. Run `just` to list all available recipes, including submodules for
  Android, iOS, Node.js, and WASM:

  ```bash
  just          # List all recipes
  just format   # Format code
  just lint     # Run all linting
  ```

- To run tests:

  ```bash
  RUST_LOG=off cargo test
  ```

  Many team members also install and use `cargo nextest` for better test
  isolation and log output behavior.

- run tests and open coverage in a browser:

```bash
./dev/test/coverage
```

- To run WebAssembly tests headless:

  ```bash
  dev/test/wasm
  ```

- To run WebAssembly tests interactively for a package, for example, `xmtp_mls`:

  ```bash
  dev/test/wasm-interactive xmtp_mls
  ```

- To run browser SDK tests:

  ```bash
  dev/test/browser-sdk
  ```

## Tips & Tricks

### Log Output Flags for Tests

- Output test logs in a async-aware context-specific tree format with the
  environment variable `CONTEXTUAL`

```
CONTEXTUAL=1 cargo test
```

- Filter tests logs by Crate

```
RUST_LOG=xmtp_mls=debug,xmtp_api=off,xmtp_id=info cargo test
```

- Output test logs as in a structured JSON format for inspection with
  third-party viewer

```
STRUCTURED=1 cargo test
```

- Two ways to replace InboxIds/InstallationIds/EthAddresses with a
  human-readable string name in logs

_NOTE_: Only works when using `CONTEXTUAL=1` flag. So to get the replacement,
`CONTEXTUAL=1 cargo test`

1.)

Before the test runs, add an `TestLogReplace` declaration to the top
`replace.add` accepts two arguments: the string to replace in logs and the
string to replace it with. Note that on dropping the "TestLogReplace" object,
the replacements will no longer be made.

```rust
let mut replace = TestLogReplace::default();
replace.add(alix.installation_id(), "alix_installation_id");
```

2.) Build the `TesterBuilder` `with_name`

```rust
let tester = Tester::builder().with_name("alix").build().await;
```

This replaces all instances of alix's InboxIds, InstallationIds and Identifiers
with "alix", "alix_installation", "alix_identifier" respectively, in test output
logs.

## Quick Start (Dev Containers)

This project supports containerized development. From Visual Studio Code Dev
Containers extension specify the Dockerfile as the target:

`Reopen in Container`

or

Command line build using docker

```bash
docker build . -t libxmtp:1
```

## Quick Start (nix)

This project supports [Determinate Nix](https://docs.determinate.systems/) for
reproducible development environments. Nix provides pinned toolchains for Rust,
Android, iOS, WebAssembly, and Node.js builds.

```bash
./dev/nix-up    # One-time setup: install Determinate Nix + direnv + binary caches
nix develop     # Enter the default dev shell
```

To temporarily disable/enable direnv without uninstalling anything:

```bash
dev/direnv-down  # Disable direnv auto-activation
dev/direnv-up    # Re-enable direnv
```

See [docs/nix-setup.md](docs/nix-setup.md) for the full setup guide, including
binary cache configuration, available dev shells, and direnv usage.

## Structure

libxmtp/

├ apps/

│ ├ [`android`](./apps/android): Example Android app (in progress)

│ ├ [`cli`](./apps/cli): Example XMTP console client. Use the CLI to try out
sending double ratchet messages on the XMTP `dev` network.

│ └ [`mls_validation_service`](./apps/mls_validation_service): MLS validation
service

├ bindings/

│ ├ [`mobile`](./bindings/mobile): FFI bindings for Android and iOS

│ ├ [`node`](./bindings/node): Node.js bindings

│ └ [`wasm`](./bindings/wasm): WebAssembly bindings

├ crates/

│ ├ [`xmtp_api_grpc`](./crates/xmtp_api_grpc): API client for XMTP's gRPC API

│ ├ [`xmtp_cryptography`](./crates/xmtp_cryptography): Cryptographic operations

│ ├ [`xmtp_mls`](./crates/xmtp_mls): Version 3 of XMTP which implements
[Messaging Layer Security](https://messaginglayersecurity.rocks/)

│ └ [`xmtp_proto`](./crates/xmtp_proto): Generated code for handling XMTP
protocol buffers

├ sdks/

│ └ [`android`](./sdks/android): Android SDK (Kotlin)

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

## Code Coverage

Code coverage is generated using `cargo llvm-cov` and is integrated into ci and
reported to [codecov](https://codecov.io).

To run the tests locally you can run the `dev/llvm-cov` script to run the same
workspace tests and generate both an lcov and html report.

If you have installed the `Coverage Gutters` extension in vscode (or a
derivative) you can get coverage information in your IDE.

## Contributing

See our [contribution guide](./CONTRIBUTING.md) to learn more about contributing
to this project.
