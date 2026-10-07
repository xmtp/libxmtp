# XMTP SDKs

The instructions below apply to the JavaScript SDKs in `node`, `browser`, and
`agent`. See `android/AGENTS.md` and `ios/AGENTS.md` for the native SDKs.

The JavaScript SDKs use the root pnpm workspace. Node and Browser use the
generated Rust SDK product. Agent uses Node. Published package names
stay `@xmtp/node-sdk`, `@xmtp/browser-sdk`, and `@xmtp/agent-sdk`.

## Commands

```bash
just install-js                        # install the root pnpm workspace once
just js sdk-products                    # stage Node and Browser products
just js build-node-sdk                  # stage the generated Node product
just js build-browser-sdk               # stage the generated Browser product
just js check-node                       # typecheck Node and agent SDKs
just js check-notification-surface       # published Node types; Browser/WASM absence
just js lint-node                      # lint Node and agent SDKs
just js build-node                       # build Node and agent SDKs
just js check                           # typecheck all
just js build
just js lint                            # oxlint
just js test                            # needs `just backend up`
```

Prepared CI consumers use
`dev/nix-shell 'just js test-node-sdk-prepared'`,
`dev/nix-shell 'just js test-browser-sdk-prepared'`, and
`dev/nix-shell 'just js test-agent-sdk-prepared'`.
The restore action must verify the current product and set
`XMTP_SDK_PREPARED_PRODUCTS=1` first. The recovery command is
`dev/nix-shell 'just js test-node-sdk-recovery-prepared 1'` (or 2 through 8).
It checks the full collected matrix before it runs one case. Each group must
own one case. It requires the real backend binary and saves test IDs and
results under `target/recovery-results`.

## Shared SDK rules

- Require an explicit `backend` for client creation. Do not select a URL from `env`.
  Browser also requires explicit `storage`.
- Use `env` only as the label in the default database file name.
- Keep the API-client cache key as `<backendUrl>|<appVersion>`.
- Keep file archive export and import tests.

Native streams stay open during retryable network faults and resume in order.

## Task graph

SDK recipes stage the selected SDK products before package tasks. The public
product is `target/sdk-packages/<target>`. Local imports use a full copy in
`sdks/<target>/dist`, with the pinned runtime assets. Source package builds run
`dev/nix-shell 'just sdk generate <target>'` before staging. An explicit
`XMTP_SDK_GENERATED_DIR` reuses that input and keeps the strict staging checks.
Do not use
`--parallel` or `--no-sort`; they can bypass task dependencies. See the
`writing-typescript` skill for the root pnpm workspace and formatting.

## Gotchas

- Start `just backend up` for tests; `just js` loads this worktree's backend URL.
- `agent` reads types from `node/dist`. Build `node` first.

## Durable message delivery

- Message iterators acknowledge the previous item only when the app requests the next item. `return` and `end` do not acknowledge it.
- Calling `await stream.onValue(callback)` selects callback mode and starts consumption. A successful callback return permits the next read to acknowledge delivery. Do not also iterate that stream.
- Core owns network recovery for message and conversation streams. Public stream options do not accept `retryAttempts`, `retryDelay`, `retryOnFail`, `onRetry`, `onFail`, `onRestart`, or `disableSync`. Callback or acknowledgement failure stops message delivery; it does not restart the callback.
- Conversation streams open without a separate pre-sync. Call an explicit `sync()` method when the app needs a current snapshot. Browser and Node lifecycle options are `signal`, `onClose`, and `onConnectionStateChange`.
- Storage errors end message streams after the operation's normal retry policy. Enrichment must preserve the storage cause. A replacement may repeat an app callback whose acknowledgement failed.
- Close and fence a failed reader before `onClose` reports a failed reason. Preserve the original error if cleanup fails. The caller can repair storage and open another stream on the same client.
- Use `from` with a `DeliveryCursor` for replay. Replay does not change default delivery progress.
- Use `beginningDeliveryCursor` for the first retained item, or the cursor from `messageHistorySnapshot` for history plus live delivery.
- Catch-up state does not depend on application acknowledgement. Keep typed
  stream failure details and all topic obligations when reporting errors.

The host reader tests use fake readers from the staged Node package. They need
the generated package but do not need a backend. The recipe stages the package:

```bash
dev/nix-shell 'just js test-node-sdk-ci test/streamRuntime.test.ts'
```

## Agent stream lifecycle

The Agent SDK reports terminal stream errors and requires an explicit `start()`
to open a new generation. Error middleware alone does not reopen streams.
`start()` skips the separate conversation pre-sync and reports local pump
readiness, including while offline.
