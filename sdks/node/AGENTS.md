# XMTP Node SDK

```bash
dev/nix-shell 'just js test-node-sdk-ci'
dev/nix-shell 'just js test-node-sdk-ci test/Client.test.ts' # One file.
dev/nix-shell 'just js test-node-sdk-ci -t create' # Matching titles.
```

`streamRecovery.test.ts` owns its TCP proxies. To test actual graceful backend
shutdown and restart, build the backend and set its absolute binary path:

```bash
dev/nix-shell 'just backend build'
XMTP_RECOVERY_BACKEND_BINARY="$(realpath result/bin/xmtp-backend)" dev/nix-shell 'just js test-node-sdk-ci test/streamRecovery.test.ts'
```

The drain cases then start a separate backend process on private ports. They
use the worktree database and leave the shared backend running. Without the
binary path, those cases test TCP EOF. Neither mode proves a rolling deployment
through an external load balancer. CI builds the binary and runs this matrix in
its own job; the ordinary shards exclude this file. Run these tests one process
at a time.

### Recovery budget tests

The two "recovery budget" tests in `streamRecovery.test.ts` wait for the real
Core outage budget two times each. Together they take about 18 minutes. CI
does not run them. They run only when `XMTP_RECOVERY_BUDGET_TESTS=1`:

```bash
XMTP_RECOVERY_BUDGET_TESTS=1 dev/nix-shell 'just js test-node-sdk-ci test/streamRecovery.test.ts -t budget'
```

Run them before you push a change that can alter when a message stream
becomes terminal, or how an app opens a replacement stream:

- The recovery budget, outage, or healthy-period rules in
  `crates/xmtp_mls/src/subscriptions/recovery.rs`.
- Reader failure and fencing in
  `crates/xmtp_mls/src/subscriptions/message_reader.rs`, `incoming.rs`,
  `incoming/controller*`, or `local_delivery/`.
- The `NetworkRecoveryExhausted` error, its message, or its mapping in
  `crates/xmtp_sdk/src/delivery/`.
- `onClose`, iterator rejection, or `end()` in the generated host reader
  stream under `crates/xmtp_sdk`.
- Delivery lease or cursor ownership between a failed stream and its
  replacement.

Report in the PR that you ran them and the result. Skip them for other
changes. The Core controlled-clock tests in `recovery.rs` and
`incoming/controller/tests.rs` cover the budget arithmetic.

## Recovery contract

Core owns network recovery for message and conversation notification streams.
A terminal Core error ends that stream. An app can open a new stream on the
same client, including from `onError` or an iterator's error handler. The old
stream stays ended. A replacement resumes saved progress and gets its own
network budget. Do not add automatic reader replacement in the SDK wrapper.

The generated reader has no automatic host replacement. Use
`ConversationStream.open` or `MessageStream.open` for an explicit replacement.
Call `ready()` before the test causes a fault. The `onClose` callback carries
one `closed` or `failed` result. Notification streams open without a separate
pre-sync. Call `sync()` if the app needs a current state before it listens.

The public recovery matrix checks exact reply IDs, message order, membership,
epoch, and processed cursors. It does not expose or compare MLS authenticators.
Core tests and the chaos inspector cover that separate check. Keep the real
90-second wire-silence bound when setting blackhole test deadlines.

## Package build

`dev/nix-shell 'just sdk generate node'` creates the shared public source.
`dev/nix-shell 'pnpm --filter @xmtp/node-sdk build'` stages the Node package
and copies it to `sdks/node/dist` for workspace imports. The source package
contains no handwritten SDK facade. Publish `target/sdk-packages/node` from
the complete supported-platform build. Do not pack the workspace source shell.
`dev/nix-shell 'pnpm --filter @xmtp/node-sdk dev'` stages the package and
repeats the stage when SDK source changes. Stop it with Ctrl-C.

Node and agent version 8 require ESM and Node 22.12 or later. End a client with
`await client.end()` before removing its storage.

`dev/nix-shell 'pnpm --filter @xmtp/node-sdk typecheck'` checks the runtime
test sources and the four public type fixtures under `type-tests/`. Keep the
fixtures in this command when public stream, notification, or configuration
types change. `publicSurface.ts` holds `@ts-expect-error` lines for the
binding shapes that the generated root must hide. The Browser SDK `typecheck`
compiles a copy from `sdks/browser/type-tests/`.
