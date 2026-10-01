# XMTP SDK façade

Run commands from the repository root in the Nix shell. Run
`just backend status` to find this worktree's backend ports.

- `just sdk generate` builds the SDK libraries and writes Swift, Kotlin, Node,
  worker WASM, and pure browser WASM bindings to `target/sdk-generated/`. In
  each TypeScript tree, `index.ts` is the package root: the public layer that
  the projection generates. The stock UniFFI root is the private `binding.ts`.
  The Node public layer imports it to load the native binding; otherwise only
  the worker, the benchmark, and transport tests import it.
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
  The browser run also checks storage layouts and attachments, with failure
  records in the conformance-featured panic fixture. Worker death uses the
  generated public package and its shared worker manager.
  All host runs start `conformance/ts/object-store.mjs` for their
  download fixtures; `SDK_OBJECT_STORE_PORT=9067` also makes it the upload
  target of a backend with no S3 of its own, as in CI, where
  `crates/xmtp_sdk/dev/deploy-fly-backend` deploys that backend to Fly for the
  Swift run. Each run sets `SDK_RELAY_TARGET` to the backend. The fixture can hold a
  small PUT response, count upload grants and object requests, and refuse
  selected relayed backend URLs. It uses `protoc` from the Rust shell to
  replace only the upload URL in a real backend response. Native clients use
  the fixture's HTTP/2 relay; browser clients use its gRPC-web relay. Both
  preserve gRPC status trailers.
- `just sdk bench` compares 20 release-profile Node calls for a zero-row page
  and a 10,000-message page with the current Node binding. It also measures
  one empty SDK async call. It runs Node with `NODE_ENV=production`. It
  enables the off-by-default `bench` feature and writes separate bindings to
  `target/sdk-bench/`.
- `just sdk check-isolation` rejects shipped-code changes in `sdks/` or
  `bindings/` on a façade branch. Its Task 1 exception accepts only the reviewed
  PROC-032 backlink removal in four named SDK source files, checked against
  their full base content. Later backlink changes need a reviewed gate update.
  Its other exceptions are the exact files of the two design SDK-040 changes
  (retained undecodable content; the foreign Restored DM peer getter), pinned
  in `crates/xmtp_sdk/dev/isolation-pins.tsv` to the git blob hash of their
  reviewed content: a listed file passes only while it hashes to its pin.
  After the last reviewed change to a listed file, run
  `crates/xmtp_sdk/dev/check-isolation --pin` and commit the table with it.
  The gate rejects code, scripts, generated output, and file-mode changes.
  Locally, pass the base branch (`just sdk check-isolation self-hosted`): a
  branch tip that merges trunk otherwise looks like a pull request merge commit.
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
- `just sdk public-consumer` stages the generated Swift, Kotlin, Node, and
  browser SDKs as separate public products under `target/sdk-public/`. It then
  compiles separate consumers in `conformance/public/`: a SwiftPM package, an
  Android library that uses a real `Context`, and TypeScript projects that
  install the Node and browser packages in `node_modules`. The consumers call
  the retained public Client surface, the identity unions, received identity,
  and Message actions on the package roots. Each Swift/Kotlin codec record has
  isolated wrong-value probes for encode, send, and reply. Negative probes check that the
  binding Client, its factories, the generated identity routes, the browser
  worker session, and private package paths stay private. The Node root must
  export exactly the public names through both `import` and `require`. Before it compiles them,
  `dev/check-public-members.py` checks that every retained Client member in
  `docs/self-hosted/sdk-api-manifest.md` is public in each installed product,
  in the static or instance placement that the manifest names.
  Run `just sdk generate` first.
- `just sdk codec-author types` stages independent Node and browser codec
  packages and checks valid calls plus six wrong-value rejections per target.
  `just sdk codec-author node` and `just sdk codec-author browser` also run
  encode/send/receive/reply checks against this worktree's backend; the browser
  run uses Chromium and the real package worker. Both read this worktree's
  Docker backend to check published push flags. Run SDK generation first.
  `XMTP_SDK_GENERATED_DIR` can select a separate generated input directory.
  For final package checks, `XMTP_SDK_PACKAGES_DIR` selects staged `node` and
  `browser` package folders and preserves their manifests, bundled dependencies,
  and assets.
  The proof installs local copies under `target/sdk-codec-author/` and uses
  only the supported ESM roots in the codec package.
- `just sdk manifest-check` compares `docs/self-hosted/sdk-api-manifest.md`
  with the old SDK sources, then checks that each Node and browser binding
  re-export row names a real export of the generated package roots. A rename
  names the new export; a removal names its replacement. Run
  `just sdk generate` first.
- `just test crate xmtp_sdk` runs the façade tests against the local backend.

The generator lives in `apps/xmtp_sdk_bindgen/`. Its global UniFFI config maps
`xmtp_sdk` to this crate root so Swift and Kotlin load `uniffi.toml`.

Content and conversation exports use named `include!` files to keep UniFFI
module paths stable. Use ordinary modules for helpers without exported metadata.
