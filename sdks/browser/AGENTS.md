# XMTP Browser SDK

The public SDK is staged by `dev/js/sdk-package browser`. Its generated product
is copied to `sdks/browser/dist` for local workspace imports. Releases pack
`target/sdk-packages/browser` directly.

```bash
dev/nix-shell 'just js test-browser-sdk-ci'
```

The browser tests use Playwright and gRPC-Web on the backend listener.
Close each client after its test. Do not share a client between tests.
Await `client.end()` before a database restore or storage replacement.
The generated package owns the worker and OPFS pool. Do not add another worker
or storage dispatcher here. Pure synchronous codecs use the `/pure` entry.
`test/recovery-proxy.ts` is a TCP proxy that Vitest browser commands run in the
Node process. Stream recovery tests point one client at it and drop its
connections. `test/download-host.ts` is an HTTP host that Vitest browser commands
run in the Node process. It answers attachment downloads with a redirect,
failure statuses, and an object with a changed tag. `test/worker-failure.test.ts`
wraps the `Worker` constructor to stop the package worker; it does not start a
worker of its own.

`test/platform` holds the browser platform proofs that need a fixture build,
a held worker, or real OPFS: Chromium, real WASM worker, storage, package,
logging, attachment lifetime, decode-once, stream opening, pure codec, and
panic proofs, with the loopback object store in `test/platform/object-store`.
`pnpm test` does not run them, and the package lint and typecheck skip them.
Run them with `dev/nix-shell 'just sdk test-browser'`; `just sdk lint`
type-checks the shared helpers. See `crates/xmtp_sdk/AGENTS.md`.

Migration platform tests are in `test/platform/migration`. Run only them with
`dev/nix-shell 'just sdk test-browser-migration'`. The normal platform runner
builds their private legacy fixture host and runs every migration assertion.
