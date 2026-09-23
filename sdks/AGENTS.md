# XMTP SDKs

The instructions below apply to the JavaScript SDKs in `node`, `browser`, and
`agent`. See `android/AGENTS.md` and `ios/AGENTS.md` for the native SDKs.

The JavaScript SDKs use the root pnpm workspace: `node` uses `bindings/node`,
`browser` uses `bindings/wasm`, and `agent` uses `node`. Published package names
stay `@xmtp/node-sdk`, `@xmtp/browser-sdk`, and `@xmtp/agent-sdk`.

## Commands

```bash
just install                           # install the root pnpm workspace once
just js bindings                        # build node + wasm bindings via Nix, stage into bindings/*/dist
just js bindings-node                    # build only Node bindings via Nix
just js check-node                       # typecheck Node and agent SDKs
just js check-notification-surface       # published Node types; Browser/WASM absence
just js lint-node                        # lint Node bindings, Node, and agent SDKs
just js build-node                       # build Node and agent SDKs
just js check                           # typecheck all
just js build
just js lint                            # oxlint
just js test                            # needs `just backend up`
```

## Shared SDK rules

- Require `backendUrl` for client creation. Do not select a URL from `env`.
- Use `env` only as the label in the default database file name.
- Keep the API-client cache key as `<backendUrl>|<appVersion>`.
- Keep file archive export and import tests.

Native streams stay open during retryable network faults and resume in order.

## Task graph

The root pnpm workspace runs package scripts through its task graph. SDK recipes
first stage the Node or WASM bindings with Nix, then run the selected package
tasks. Recursive SDK commands select workspace packages under `sdks/*`; they do
not build the binding packages. Do not use `--parallel` or `--no-sort`, because
either option can bypass task dependencies.

SDK packages build with tsdown. Use `pnpm build` for one build and `pnpm dev`
to run tsdown in watch mode from an SDK package.

## Gotchas

- Tests require `XMTP_BACKEND_URL`. The `just js` recipes load this worktree's value.
- Needs `just backend up`. Run `just install` and `just js bindings` once first for full local SDK work.
- Node and agent CI uses `NIX_DEVSHELL=js-node`, `just install`, and `just js bindings-node`.
- Verify dependency changes with the root install. Declare required tools in the selected workspace and run them with `pnpm --filter <name> exec`.
- `agent` reads types from `node/dist`. Build `node` first.
- Formatting uses Oxfmt. Run `just format-js` to write package formatting, or
  `just lint-js-format` to check it. Oxlint does not format files.

## Durable message delivery

- Message iterators acknowledge the previous item only when the app requests the next item. `return` and `end` do not acknowledge it.
- Supplying `onValue` selects callback mode and starts consumption. Successful callback return acknowledges delivery. Do not also iterate that stream.
- Core owns message-stream network recovery. Message streams accept but do not use legacy `retry*`, `onFail`, `onRetry`, `onRestart`, or `disableSync` options. These options still apply to notification streams. Callback or acknowledgement failure stops message delivery; it does not restart the callback.
- Storage errors end message streams after the operation's normal retry policy. Enrichment must preserve the storage cause. A replacement may repeat an app callback whose acknowledgement failed.
- Close and fence a failed reader before `onError` runs. Preserve the original error if cleanup fails. The caller can repair storage and open another stream on the same client.
- Use `from` with a `DeliveryCursor` for replay. Replay does not change default delivery progress.
- Use `beginningDeliveryCursor` for the first retained item, or the cursor from `messageHistorySnapshot` for history plus live delivery.
- `catchUpSnapshot` and `catchUpChanged` report network and processing state. They do not depend on application acknowledgement.
- `getStreamFailureDetails(error)` reads typed barrier, catch-up, and published-but-unconfirmed details. It preserves all topic obligations. Sequence values are `bigint`. A null target means that target capture did not complete.

The pure delivery-boundary tests do not need a backend or generated bindings:

```bash
NIX_DEVSHELL=js-node dev/nix-shell 'pnpm --filter @xmtp/node-sdk exec vitest run test/MessageStream.test.ts test/streamFailure.test.ts'
NIX_DEVSHELL=js-node dev/nix-shell 'pnpm --filter @xmtp/browser-sdk exec vitest run test/MessageStream.test.ts test/WorkerBridge.test.ts test/streamFailure.test.ts --browser.enabled=false'
```
