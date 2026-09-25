# XMTP SDK façade

Run commands from the repository root in the Nix shell. Run
`just backend status` to find this worktree's backend ports.

- `just sdk generate` builds the SDK libraries and writes Swift, Kotlin, Node,
  worker WASM, and pure browser WASM bindings to `target/sdk-generated/`.
- `just sdk lint` checks generated names and TypeScript source. It also
  rejects test-only hooks (`*ForTest`, `*_for_test`, `bridge_test_panic`) and
  benchmark exports in the default bindings and in
  `apps/xmtp_sdk_bindgen/runtime/`. Keep test hooks in test source sets.
- `just sdk wasm-init` loads the staged WASM package in Node.
- `just sdk conformance <swift|kotlin|node|browser>` runs scenarios 1, 2, and 7
  against this worktree's backend.
- `just sdk bench` compares 20 release-profile Node calls for a zero-row page
  and a 10,000-message page with the current Node binding. It also measures
  one empty SDK async call. It runs Node with `NODE_ENV=production`. It
  enables the off-by-default `bench` feature and writes separate bindings to
  `target/sdk-bench/`.
- `just sdk check-isolation` rejects a façade branch that changes `sdks/` or
  `bindings/`.
- `just sdk conformance-bridge` runs bridge Vitest, real WASM worker proofs,
  and Chromium proofs for pure codecs, worker failure, and browser storage.
- `just test crate xmtp_sdk` runs the façade tests against the local backend.

The generator lives in `apps/xmtp_sdk_bindgen/`. Its global UniFFI config maps
`xmtp_sdk` to this crate root so Swift and Kotlin load `uniffi.toml`.
