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
run in the Node process. It answers attachment downloads with a redirect and
failure statuses. `test/worker-failure.test.ts` wraps the `Worker` constructor to
stop the package worker; it does not start a worker of its own.
