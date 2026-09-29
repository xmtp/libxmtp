# XMTP SDK façade

Run commands from the repository root in the Nix shell. Run
`just backend status` to find this worktree's backend ports.

- `just sdk generate` builds the SDK libraries and writes Swift, Kotlin, Node,
  worker WASM, and pure browser WASM bindings to `target/sdk-generated/`.
- `just sdk check-file-sizes` checks the 1,000-line limit for every SDK source
  file, including conformance files. Generated and ignored build files are excluded.
  Keep most new files below 500 lines.
- `just sdk lint` checks file sizes, generated names, and TypeScript source.
  It checks shared public value types on Node and browser, including negative
  consumers for readonly records, transport fields, credentials, and bytes. It also
  rejects test-only hooks (`*ForTest`, `*_for_test`, `bridge_test_panic`) and
  benchmark exports in the default bindings and in
  `apps/xmtp_sdk_bindgen/runtime/`. Keep test hooks in test source sets.
- `just sdk wasm-init` loads the staged WASM package in Node.
- `just sdk conformance <swift|kotlin|node>` runs scenarios against this
  worktree's backend. `just sdk conformance browser` runs scenarios 1-11
  plus a real WASM trap from a test-only panic fixture in Vitest Playwright
  Chromium, then checks real OPFS and worker behavior. Its recipe builds the
  pure codec and panic fixtures in the Rust shell before the JS shell.
  Scenario 7 checks readers and streams. Scenario 8 checks events and listeners.
- `just sdk bench` compares 20 release-profile Node calls for a zero-row page
  and a 10,000-message page with the current Node binding. It also measures
  one empty SDK async call. It runs Node with `NODE_ENV=production`. It
  enables the off-by-default `bench` feature and writes separate bindings to
  `target/sdk-bench/`.
- `just sdk check-isolation` rejects shipped-code changes in `sdks/` or
  `bindings/` on a façade branch. Its Task 1 exception accepts only the reviewed
  PROC-032 backlink removal in four named SDK source files, checked against
  their full base content. Later backlink changes need a reviewed gate update.
  The gate rejects code, scripts, generated output, and file-mode changes.
  Tests and changelogs remain outside the shipped-code guard.
- `just sdk conformance-bridge` runs bridge Vitest, real WASM worker proofs,
  and Chromium proofs for pure codecs, worker failure, and browser storage.
- `just sdk conformance-storage` runs the real-worker OPFS proof against the
  staged SDK. Run `just sdk generate` first after SDK or runtime changes.
- `just sdk conformance-package` checks package creation reservations, shared
  client/admin workers, final worker termination, and collection in Chromium.
  Run `just sdk generate` first after SDK or runtime changes.
- `just sdk conformance-bridge-unit <vitest arguments>` runs focused bridge
  unit tests against the staged SDK.
- `just sdk public-consumer` stages the generated Swift and Kotlin SDKs as
  separate public products under `target/sdk-public/`. It then compiles
  separate consumers in `conformance/public/`: a SwiftPM package and an
  Android library that uses a real `Context`. The consumers call the retained
  host Client surface, the identity overloads, and Message actions. Negative
  probes check that `.raw` and the generated Client factories stay private.
  Run `just sdk generate` first. Node and browser public roots are not staged yet.
- `just test crate xmtp_sdk` runs the façade tests against the local backend.

The generator lives in `apps/xmtp_sdk_bindgen/`. Its global UniFFI config maps
`xmtp_sdk` to this crate root so Swift and Kotlin load `uniffi.toml`.

Content and conversation exports use named `include!` files to keep UniFFI
module paths stable. Use ordinary modules for helpers without exported metadata.
