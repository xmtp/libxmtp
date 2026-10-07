<!-- The branded header uses HTML and places status badges before the title. -->
<!-- markdownlint-configure-file {"MD041": false, "MD033": {"allowed_elements": ["h1", "p", "img", "br", "a"]}} -->

[![Lint](https://github.com/xmtp/libxmtp/actions/workflows/lint.yml/badge.svg)](https://github.com/xmtp/libxmtp/actions/workflows/lint.yml)
[![Test](https://github.com/xmtp/libxmtp/actions/workflows/test.yml/badge.svg)](https://github.com/xmtp/libxmtp/actions/workflows/test.yml)
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

- [Rustup](https://rustup.rs/)
- [Docker](https://www.docker.com/get-started/)
- [Foundry](https://book.getfoundry.sh/getting-started/installation#using-foundryup)
- [just](https://github.com/casey/just) (optional for testing)

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
  ./dev/up
  ```

  Specifically, this command creates and runs an XMTP node in Docker Desktop.

- This project uses [`just`](https://github.com/casey/just) as a command runner.
  Run `just` to list all available recipes, including submodules for Android,
  iOS, Node.js, and WASM:

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
  just wasm test
  ```

Note: If the tests fail with "bind() failed: Cannot assign requested address,"
Chrome is unable to bind to IPv6 and will fall back to IPv4. Although this
should be a warning, Chromedriver currently logs this message as SEVERE, which
halts the wasm-bindgen-test. You can optionally disable the Chromedriver logs
output to prevent this.

```bash
CHROMEDRIVER_ARGS="--log-level=OFF" just wasm test
```

- To run WebAssembly tests interactively for a package, for example, `xmtp_mls`:

  ```bash
  ./dev/test/wasm-interactive xmtp_mls
  ```

- To run browser SDK tests:

  ```bash
  ./dev/test/browser-sdk
  ```

## Tips & Tricks

### Log Output Flags for Tests

`#[xmtp_common::test]` installs the test logger. Control it with environment
variables:

```bash
RUST_LOG=xmtp_mls=debug,xmtp_api=off,xmtp_id=info cargo test   # filter by crate
STRUCTURED=1 cargo test                                        # JSON lines for a log viewer
SHOW_SPAN_FIELDS=1 cargo test                                  # include tracing span fields
XMTP_TEST_LOGGING=false cargo test                             # no test logging; CI=true does the same
```

Give test clients a name so log lines can be matched to them. `tester!(alix)`
does this for you; a hand-built client uses the builder:

```rust
let tester = Tester::builder().with_name("alix").build().await;
```

The name is logged with the client's installation id in an `AssociateName`
event. See `.agents/skills/writing-rust-tests/` for the full test guide.

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
./dev/direnv-down  # Disable direnv auto-activation
./dev/direnv-up    # Re-enable direnv
```

See [docs/nix-setup.md](docs/nix-setup.md) for the full setup guide, including
binary cache configuration, available dev shells, and direnv usage.

## Structure

libxmtp/

├ apps/

│ ├ [`xdbg`](./apps/xmtp_debug): comprehensive CLI for sending/load testing XMTP
clients & network

service

├ [`sdks`](./sdks): Generated public SDK packages

├ crates/

│ ├ [`xmtp_api_grpc`](./crates/xmtp_api_grpc): API client for XMTP's gRPC API

│ ├ [`xmtp_cryptography`](./crates/xmtp_cryptography): Cryptographic operations

│ ├ [`xmtp_mls`](./crates/xmtp_mls): Version 3 of XMTP which implements
[Messaging Layer Security](https://messaginglayersecurity.rocks/)

│ └ [`xmtp_proto`](./crates/xmtp_proto): Generated code for handling XMTP
protocol buffers

├ sdks/

│ ├ [`agent`](./sdks/agent): Agent SDK (TypeScript)

│ ├ [`android`](./sdks/android): Android SDK (Kotlin) and its example app

│ ├ [`browser`](./sdks/browser): Browser SDK (TypeScript)

│ ├ [`ios`](./sdks/ios): iOS SDK (Swift)

│ └ [`node`](./sdks/node): Node SDK (TypeScript)

### Run the benchmarks

Run commands from the repository root through `dev/nix-shell`.

#### Rust benchmarks

The following table lists all registered Criterion benchmark targets. Each
target requires the `bench` feature. The source links show the individual cases
and data sizes.

| Package | Target | Measurements |
| --- | --- | --- |
| `xmtp_mls` | [`group_limit`](./crates/xmtp_mls/benches/group_limit.rs) | Add members by identity or inbox ID to empty and existing groups. Remove all or half of the members. Add one member across group sizes. |
| `xmtp_mls` | [`crypto`](./crates/xmtp_mls/benches/crypto.rs) | Wrap Welcome payloads with Curve25519 and post-quantum HPKE across payload sizes. |
| `xmtp_mls` | [`identity`](./crates/xmtp_mls/benches/identity.rs) | Register an identity with an externally owned account (EOA). |
| `xmtp_mls` | [`groups`](./crates/xmtp_mls/benches/groups.rs) | Find groups, list conversations, and find groups with filters across database sizes. |
| `xmtp_mls` | [`messages`](./crates/xmtp_mls/benches/messages.rs) | Query messages with `find_messages` and `find_messages_v2`. Cases cover ordering, time bounds, message kind, delivery status, content types, and sender filters. |
| `xmtp_mls` | [`consent`](./crates/xmtp_mls/benches/consent.rs) | Find consent by DM ID across consent-record counts. |
| `xmtp_mls` | [`sync_conversations`](./crates/xmtp_mls/benches/sync_conversations.rs) | Sync conversations across 10 or 100 groups, with different counts of groups that have new messages. |
| `xmtp_db` | [`db_init_latency`](./crates/xmtp_db/benches/db_init_latency.rs) | Initialize a fresh encrypted database with injected fsync and write latency. Report SQLite I/O operation counts. |
| `xmtp_db` | [`conversation_list`](./crates/xmtp_db/benches/conversation_list.rs) | List conversations and find groups with consent, type, sync-group, and activity filters. Read five pages and find one group by ID. |

Run a target that needs no backend:

```sh
dev/nix-shell 'dev/agent-run cargo bench -p xmtp_mls --features bench --bench crypto'
dev/nix-shell 'dev/agent-run cargo bench -p xmtp_db --features bench --bench db_init_latency'
dev/nix-shell 'dev/agent-run cargo bench -p xmtp_db --features bench --bench conversation_list'
```

`conversation_list` uses 10,000 conversations by default. Set
`XMTP_BENCH_CONVERSATIONS` to change that count. Set `XMTP_BENCH_DIR` to select
the database directory for `db_init_latency`.

The other `xmtp_mls` targets need a backend. Use `just backend ci` to run them
with disposable local services and the correct connection settings:

```sh
dev/nix-shell 'just backend ci dev/agent-run cargo bench -p xmtp_mls --features bench --bench group_limit'
dev/nix-shell 'just backend ci ./dev/bench'
dev/nix-shell 'just backend ci ./dev/bench add_1_member_to_group'
```

`./dev/bench` runs all `xmtp_mls` benchmarks. Its optional argument filters the
benchmark names. Criterion writes results under `target/criterion/`. To collect
tracing data, set `XMTP_FLAMEGRAPH=trace` for a target that uses the benchmark
logger. The logger writes `tracing.folded`.

#### SDK host benchmarks

The SDK suite runs the following workloads on all four hosts:

| Workload | Measurements |
| --- | --- |
| `cold_start` | Create a client with a fresh database and a real ECDSA signer, after module load. |
| `page` | Read and normalize 1,000 messages with text, replies, attachments, and reactions. |
| `stream` | Publish 1,000 messages and 250 reactions while a group stream reads all 1,250 events. |

Each workload reports duration and peak memory, with p50 and p95. The stream
workload also reports message and event rates. The suite records package size;
Browser runs also record main-thread long tasks. Memory measurements have a
different scope on each host.

Stage the matching SDK package and start this worktree's backend before a run.
Swift needs an iOS Simulator. Kotlin needs an Android device or emulator. See
the [SDK benchmark guide](./crates/xmtp_sdk/benchmarks/README.md) for setup,
options, and result fields.

| Host | Command |
| --- | --- |
| Node | `dev/nix-shell 'just sdk bench node --samples 5'` |
| Browser (Chromium) | `dev/nix-shell 'NIX_DEVSHELL=js just sdk bench browser --samples 5'` |
| Swift (iOS Simulator) | `dev/nix-shell 'NIX_DEVSHELL=ios just sdk bench swift --samples 5'` |
| Kotlin (Android) | `dev/nix-shell 'NIX_DEVSHELL=android just sdk bench kotlin --samples 5'` |

Each command runs all three workloads and writes `results.json` under
`target/sdk-bench/`. To check the runners without a backend, device, or SDK
build, run `dev/nix-shell 'just sdk bench-check'`.

#### Database query benchmark

The [`xmtp-db-tools` query benchmark](./apps/db_tools/src/tasks/db_bench.rs)
measures queries on an existing database. It covers groups, messages, consent,
association state, identity updates, group intents, refresh state, key-package
history, conversation lists, commit logs, DMs, message deletion, device sync,
tasks, re-add status, pending removals, identity rotation, and group versions.

Run it on a copy of a populated database. Some cases update records or delete
expired messages. Supply `--db-key` when the database is encrypted:

```sh
dev/nix-shell 'dev/agent-run cargo run -p xmtp-db-tools -- query-bench /path/to/database-copy.db3'
```

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
