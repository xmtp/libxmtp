# xdbg

Outside default members; explicitly check/test Cargo package `xdbg` with root
Just recipes in Nix. Network commands need the worktree backend and `--url`.

- Use `MessageBackendBuilder` / `ApiClientWrapper` for queries and MLS streams;
  do not add a local builder. URL hashes isolate storage.
- `generate-api-key` prints only the key, without URL, logging, metrics, or storage.
- Keep Nix compile inputs commit-independent; `.#xdbg` supplies the commit SHA
  at runtime. Version also selects stored state. Cargo builds ignore `XDBG_GIT_SHA`.
- CLI usage: [README](README.md). Telemetry and `just backend observe-check`:
  [observability](../../docs/backend-observability.md).
