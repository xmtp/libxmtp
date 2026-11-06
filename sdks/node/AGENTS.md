# XMTP Node SDK

Generated public façade; change Rust or bindgen, not a handwritten wrapper here.

```sh
dev/nix-shell 'just js test-node-sdk-ci [test/file.test.ts]'
dev/nix-shell 'just js check-node'
```

- ESM, Node 22.12+. Await `client.end()` before removing storage.
- Build/stage through `just js build-node`; publish `target/sdk-packages/node`
  from the complete platform build, never the workspace source shell.
- Keep `type-tests/` in typechecking for public stream, notification, or config
  changes; Browser checks its own copy of the public fixtures.
- Before pushing stream terminality, fencing, or replacement changes, run the
  opt-in budget tests and report the result. Read [recovery testing](TESTING.md)
  for triggers, backend-process mode, and deadlines. Normal CI omits this suite.
- Core owns recovery; replacements are explicit. Call `ready()` before injecting
  faults; preserve the real 90-second wire-silence bound in blackhole tests.
