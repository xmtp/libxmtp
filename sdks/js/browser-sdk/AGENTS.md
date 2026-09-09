# XMTP Browser SDK

Read `../AGENTS.md` for workspace commands. Run commands from the repository root.

- Require `backendUrl` for client creation. Do not select a URL from `env`.
- Use `env` only as the label in the default database file name.
- Keep the API-client cache key as `<backendUrl>|<appVersion>`.
- Keep file archive export and import tests.

```bash
export XMTP_BACKEND_URL=http://127.0.0.1:5050
just backend up
just js check
just js lint
just js test-browser-sdk-ci
```

The browser tests use Playwright and gRPC-Web on the backend listener.
