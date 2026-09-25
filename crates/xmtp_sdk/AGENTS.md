# XMTP SDK façade

Run commands from the repository root in the Nix shell. Run
`just backend status` to find this worktree's backend ports.

- `just sdk generate` builds the SDK libraries and writes Swift, Kotlin, Node,
  and WASM bindings to `target/sdk-generated/`.
- `just sdk lint` checks generated names and TypeScript source.
- `just sdk wasm-init` loads the staged WASM package in Node.
- `just sdk check-isolation` rejects a façade branch that changes `sdks/` or
  `bindings/`.
- `just test crate xmtp_sdk` runs the façade tests against the local backend.

The generator lives in `apps/xmtp_sdk_bindgen/`. Its global UniFFI config maps
`xmtp_sdk` to this crate root so Swift and Kotlin load `uniffi.toml`.
