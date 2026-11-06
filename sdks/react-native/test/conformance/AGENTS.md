# Installed consumer conformance

Test RN boundaries (JSI values, callbacks, streams, OS sandbox, runtime), not
SDK behavior already covered by Rust.

- Keep reports matched to the exact requested, ordered phases in `run.py` and
  `peer.mjs`. Read [journey modes](JOURNEYS.md) when changing phase selection.
- Run `dev/nix-shell 'just react-native test-control'` for HTTP control changes.
  It does not prove native teardown; installed Hermes journeys remain required.
