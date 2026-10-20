# Installed consumer conformance

The installed bare and Expo consumers run these journeys on a device. They check
only what the React Native layer can break: JSI values, callbacks, streams, the
OS sandbox, and the native runtime. Rust tests own SDK behavior. Do not add a
journey that retests it.

`XMTP_RN_JOURNEYS` selects the journey set. `test/consumers/run.py` and
`peer.mjs` both require the report to name exactly these phases, in order:

- `pr`, the default when unset: `foundation` (`journey.ts`: Hermes and Fabric,
  a uint64 identity vector, byte views, OS sandbox paths, a DM exchange with the
  Node peer, the closed-client error, an encrypted reopen, and one message
  stream read). Conformance mode adds `content` (`content-vectors.ts`),
  `boundary` (`boundary.ts`: a signed timestamp, a nested uint64, and a held,
  re-entrant signer), `listener` (`events.ts`), and `react` (the `/react` entry
  resolves).
- `full`: adds `runtime-reload` (`runtime.ts`) and a separate cold background
  launch (`background.ts`). Post-merge and release runs select it.

Run the peer's reload-settlement regression from the repository root:

```sh
dev/nix-shell 'just react-native test-control'
```

This test uses the real HTTP control helper. It proves signer/response settlement
the bounded stale-call observation, and final-result admission, not native runtime teardown. Installed
Hermes consumer runs remain required for mobile execution evidence.
