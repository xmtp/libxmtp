# XMTP Node SDK

```bash
just js test-node-sdk-ci
just js test-node-sdk-ci test/createBackend.test.ts # one file
just js test-node-sdk-ci -t create # tests with matching titles
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

## Recovery contract

Core owns network recovery for message and conversation notification streams.
A terminal Core error ends that stream. An app can open a new stream on the
same client, including from `onError` or an iterator's error handler. The old
stream stays ended. A replacement resumes saved progress and gets its own
network budget. Do not add automatic reader replacement in the SDK wrapper.

The notification wrapper has a separate finite fallback for an unexpected
native close. That retry count lasts for the JS stream. Only a new JS stream
resets it. A successful native reopen does not reset the fallback count.
Durable message streams do not use that fallback or the legacy retry options.
Node notification streams open without a separate pre-sync. Call an explicit
`sync()` method if the app needs a current snapshot before it listens.

The public recovery matrix checks exact reply IDs, message order, membership,
epoch, and processed cursors. It does not expose or compare MLS authenticators.
Core tests and the chaos inspector cover that separate check. Keep the real
90-second wire-silence bound when setting blackhole test deadlines.

Pure notification fallback tests need no backend or native addon:

```bash
NIX_DEVSHELL=js-node dev/nix-shell 'pnpm --filter @xmtp/node-sdk exec vitest run test/streams.test.ts test/streamRetryBudget.test.ts --retry=0'
```
