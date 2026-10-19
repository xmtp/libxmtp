# Installed consumer conformance

Run the peer's reload-settlement regression from the repository root:

```sh
dev/nix-shell 'just react-native test-control'
```

This test uses the real HTTP control helper. It proves signer/response settlement
the bounded stale-call observation, and final-result admission, not native runtime teardown. Installed
Hermes consumer runs remain required for mobile execution evidence.
