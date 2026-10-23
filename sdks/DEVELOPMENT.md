# JavaScript SDK development

## Package preparation

SDK recipes stage the selected SDK products before package tasks. The public
product is `target/sdk-packages/<target>`. Local imports use a full copy in
`sdks/<target>/dist`, with the pinned runtime assets. React Native JS/types
can be staged without mobile binaries. Its installed consumer proofs use the
canonical tarball assembled from the Linux and Darwin native components; see
[RN package development](react-native/DEVELOPMENT.md). Source package builds
(`dev/js/sdk-package`) run `just sdk generate <target>` before Node/Browser
staging; with `XMTP_SDK_BUILD=nix` they copy the release-profile Nix trees
instead, as CI and release do. React Native defaults to its lightweight Nix
JS/types product; `XMTP_SDK_BUILD=cargo` explicitly selects local release-profile
generation. Shared typecheck, build, and full lint stage all three products.
Node-only and notification-surface checks keep their narrower inputs. The helper
refuses `XMTP_SDK_GENERATED_DIR`: it stages only what it
generated. Do not use
`--parallel` or `--no-sort`; they can bypass task dependencies. See the
`writing-typescript` skill for the root pnpm workspace and formatting.

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

File archive coverage lives in `sdks/node/test/DeviceSync.test.ts` and
`crates/xmtp_sdk/src/tests/archives.rs`; preserve these round trips.
