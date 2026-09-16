# XMTP Browser SDK

```bash
just js test-browser-sdk-ci
```

The browser tests use Playwright and gRPC-Web on the backend listener.
Test creation helpers close their clients after each test. Do not share those clients across tests.

Await the client's `close()` before a whole-database restore. Close releases the
database owner before it stops the worker.
