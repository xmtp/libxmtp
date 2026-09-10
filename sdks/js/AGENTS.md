# XMTP JS SDKs

Yarn workspace: `node-sdk` (over `bindings/node`), `browser-sdk` (over `bindings/wasm`), `agent-sdk` (over `node-sdk`).

## Commands

```bash
just js install
just js bindings                        # build node + wasm bindings via Nix, stage into bindings/*/dist
just js bindings-node                    # build only Node bindings via Nix
just js install-node-ci                  # install only Node and agent workspaces
just js check-node                       # typecheck Node and agent SDKs
just js lint-node                        # lint Node and agent SDKs
just js build-node                       # build Node and agent SDKs
just js check                           # typecheck all
just js build
just js lint                            # eslint
just js test                            # needs `just backend up`
just js test-node-sdk-ci                 # native SDK tests
just js test-browser-sdk-ci              # Playwright SDK tests
just js test-agent-sdk-ci                # agent SDK tests
NIX_DEVSHELL=js dev/nix-shell 'cd sdks/js && yarn workspace @xmtp/node-sdk run test'      # one SDK
NIX_DEVSHELL=js dev/nix-shell 'cd sdks/js && yarn workspace @xmtp/browser-sdk run test'   # playwright
NIX_DEVSHELL=js dev/nix-shell 'cd sdks/js && yarn workspace @xmtp/node-sdk run build && yarn workspace @xmtp/agent-sdk run test'
NIX_DEVSHELL=js dev/nix-shell 'cd sdks/js/node-sdk && yarn vitest run test/createBackend.test.ts'   # one file
NIX_DEVSHELL=js dev/nix-shell 'cd sdks/js/node-sdk && yarn vitest run -t "should create a backend with an explicit URL"'   # one test
```

## Gotchas

- Export `XMTP_BACKEND_URL=http://127.0.0.1:5050`. Tests require this URL.
- Needs `just backend up`. Run `just js install` and `just js bindings` once first for full local SDK work.
- Node and agent CI uses `NIX_DEVSHELL=js-node`, `just js install-node-ci`, and `just js bindings-node`.
- Verify dependency changes with the focused CI install. It omits root development tools; declare required tools in the selected workspace and run them with `yarn workspace <name> exec`.
- `agent-sdk` reads types from `node-sdk/dist`. Build `node-sdk` first.
- Formatting is treefmt prettier (`just lint-config`), not eslint.

## Durable message delivery

- Message iterators acknowledge the previous item only when the app requests the next item. `return` and `end` do not acknowledge it.
- Supplying `onValue` selects callback mode and starts consumption. Successful callback return acknowledges delivery. Do not also iterate that stream.
- Use `from` with a `DeliveryCursor` for replay. Replay does not change default delivery progress.
- Use `beginningDeliveryCursor` for the first retained item, or the cursor from `messageHistorySnapshot` for history plus live delivery.
- `catchUpSnapshot` and `catchUpChanged` report network and processing state. They do not depend on application acknowledgement.
- Client `streamSettings` fields are optional. Core defaults and validation apply. Timer fields use milliseconds.
- `getStreamFailureDetails(error)` reads typed barrier, catch-up, and published-but-unconfirmed details. It preserves all topic obligations. Sequence values are `bigint`. A null target means that target capture did not complete.
- Await the Browser SDK client's `close()` before a whole-database restore. Close releases the database owner before it stops the worker.

The pure delivery-boundary tests do not need a backend or generated bindings:

```bash
NIX_DEVSHELL=js-node dev/nix-shell 'cd sdks/js && yarn workspace @xmtp/node-sdk exec vitest run test/MessageStream.test.ts test/streamFailure.test.ts'
NIX_DEVSHELL=js-node dev/nix-shell 'cd sdks/js && yarn workspace @xmtp/browser-sdk exec vitest run test/MessageStream.test.ts test/WorkerBridge.test.ts test/streamFailure.test.ts --browser.enabled=false'
```
