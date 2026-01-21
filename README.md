# LibXMTP

[![Lint](https://github.com/xmtp/libxmtp/actions/workflows/lint-workspace.yaml/badge.svg)](https://github.com/xmtp/libxmtp/actions/workflows/lint-workspace.yaml)
[![Test](https://github.com/xmtp/libxmtp/actions/workflows/test-workspace.yml/badge.svg)](https://github.com/xmtp/libxmtp/actions/workflows/test-workspace.yml)
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

  ```bash
  dev/up
  ```

  Specifically, this command creates and runs an XMTP node in Docker Desktop.

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

This project has an option to use nix as the development environment. Nix sets
up a reproducible & deterministic environment of the dependency tree libxmtp
requires. In the future the hope is to cover all SDKs -- currently, Android &
Wasm are best supported. Flake outputs are cached with
[determinate nix](https://docs.determinate.systems/). Determinate is a
distribution of nix catered towards developers & CI with sophisticated caching
ability.

### Install

use the `./dev/nix-up` script and follow the prompts. this will install
determinate nix & direnv. Direnv is a useful tool to auto-load default nix
environments (with your consent, given `direnv allow` && `direnv deny` commands)
with your already-used shell environment.

### Uninstall

use the `./dev/nix-down` script & follow prompts. this will uninstall nix &
direnv.

### Using direnv

to configure direnv for a project, run the command
`echo "use flake" . > .envrc"` in the project root. direnv will prompt you to
allow the environment which can be done with `direnv allow`. using a non-default
environment (ex: android) can be done using `nix develop .#environment`. EX:
`nix develop .#android`. the environment description must be available in nix
flake `devShells` output.

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
