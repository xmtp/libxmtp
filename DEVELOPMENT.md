# Development tasks

Run these commands from the repository root through `dev/nix-shell`. See
[README.md](README.md#getting-started-working-in-this-repo) for the initial setup
and [CONTRIBUTING.md](CONTRIBUTING.md) for the contribution process.

## Add or change dependencies

Regenerate the `workspace-hack` crate after a dependency change:

```sh
dev/nix-shell 'dev/agent-run cargo hakari generate'
dev/nix-shell 'dev/agent-run cargo hakari verify'
```

## Test logging

`#[xmtp_common::test]` installs the test logger. Use these environment variables
to select the log format and level:

```sh
dev/nix-shell 'RUST_LOG=xmtp_mls=debug,xmtp_api=off,xmtp_id=info just test crate xmtp_mls'
dev/nix-shell 'STRUCTURED=1 just test crate xmtp_mls'
dev/nix-shell 'SHOW_SPAN_FIELDS=1 just test crate xmtp_mls'
dev/nix-shell 'XMTP_TEST_LOGGING=false just test crate xmtp_mls'
```

`STRUCTURED=1` produces JSON lines. `SHOW_SPAN_FIELDS=1` includes tracing span
fields. `XMTP_TEST_LOGGING=false` disables test logging; `CI=true` also disables
it.

Give each test client a name so you can match it to its log lines.
`tester!(alix)` sets the name. For a client built by hand, use the builder:

```rust
let tester = Tester::builder().with_name("alix").build().await;
```

The logger records the name and installation ID in an `AssociateName` event.
See the [Rust test guide](.agents/skills/writing-rust-tests/SKILL.md) for more
options.

## Benchmarks

Run commands from the repository root through `dev/nix-shell`.

### Rust benchmarks

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

### SDK host benchmarks

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

### Database query benchmark

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

## Code coverage

Run the workspace coverage script to generate LCOV and HTML reports:

```sh
dev/nix-shell './dev/llvm-cov'
```

CI also uses `cargo llvm-cov` and reports results to
[Codecov](https://codecov.io/gh/xmtp/libxmtp). Use the Coverage Gutters editor
extension to view coverage in source files.
