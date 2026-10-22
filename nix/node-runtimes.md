# Node runtime maintenance

Read this when changing `lib/node-versions.nix`, its patches, or the repository's
Node pins. These overrides secure repository-owned development, build, CI, Docker,
and Nix runtimes. They must not raise published JS SDK engine minimums, introduce
consumer runtime guards, or change the SDK's supported API surface. The existing
published minimum remains Node 22.12.0.

## Pins and scoped dependencies

Verify released versions against Node's official release index and source hashes
against each release's `SHASUMS256.txt`. Keep each repository runtime within its
existing major, and keep `.node-version` and `.nvmrc` matched to Node 26. Slim
overrides also update the full Node packages and Node-based tools within the
isolated runtime package set. The installed-package workflow and `sdk-platforms.json` separately use
secure Node 22 container images without tightening consumer requirements.

`lib/default.nix` imports the Node overlay in a separate package set. Repository
shells and native checks select `xmtp.nodejs_24` or `xmtp.nodejs_26`; pnpm, napi,
Markdown lint, Playwright, wasm-bindgen's check input, and the bindgen npm builds
also select that scope, including their embedded wrappers and build hooks.
React Native shells, JS products, package assembly, and native consumer builds
also select `xmtp.nodejs_26`.
Do not replace the outer package set's `nodejs` aliases: doing so changes
Chromium's build inputs and forces an unrelated browser source build. The
unchanged browser package can reuse its upstream cache; its historical build-time
Node input is not a runtime shipped in the browser closure. Ordinary CI suites
consume the checked Node outputs without an all-shell preparation job.

When Nixpkgs supplies these security releases and their complete shared-library
fixes, prefer its unmodified, cached Node packages in this isolated scope. Verify
native Linux/macOS cache availability and runtime checks before removing the
source overrides. A global Node overlay would still invalidate browser builds,
even if Node itself were cached.

Nix's Node builds use shared libraries. Updating only Node's source does not
update those libraries, so the overlay scopes the Brotli security patch, c-ares
update, and same-major OpenSSL fixes to Node. Its nghttp2/ngtcp2 inputs must resolve
to the same patched TLS, DNS, and Brotli dependencies. Do not fall back to the
unpatched libraries or update these packages globally.

Node's scoped simdutf is built with C++20 to export the atomic Base64 symbols V8
uses. Remove this override only when the underlying shared library exports those
symbols with the required ABI and the native link/runtime checks pass without it.

Reassess each scoped security override when updating nixpkgs: remove it only when
the replacement package already contains its fix and the dependency-wiring and
native-runtime checks still pass. The old shared-ICU Temporal and emulator patches
are superseded by upstream support in these releases and must not be reapplied.

## Darwin datagram fixture patch

`lib/node-dgram-localhost.patch` changes only the upstream
`test/parallel/test-dgram-connect-sync.js` fixture, for both Node majors on Darwin.
The original fixture blocks `127.0.0.1`, asserts `ERR_IP_BLOCKED`, then proves the
same socket is reusable by connecting to `127.0.0.2`. Nix's Darwin sandbox permits
canonical localhost connections but denies that secondary-loopback connection,
so the successful-connect step instead fails with `connect EPERM 127.0.0.2:12345`.

The patch swaps the two fixture addresses: reject the application-blocklisted
`127.0.0.2`, then connect the same socket successfully to `127.0.0.1`. It preserves
the `ERR_IP_BLOCKED`, remote-address, and socket-reuse assertions. It does not
change Node runtime behavior, widen sandbox permissions, skip tests, or affect
Linux derivations.

Remove the patch for a major once that major's release sources contain an
equivalent sandbox-compatible fixture. It can also be removed if the normal
upstream Nix Darwin sandbox permits the original fixture without a local
permission expansion. In either case, first build the unpatched final derivation
on native macOS with the unchanged upstream checks enabled, confirm the complete
suite passes (including this test's blocked-address and reuse assertions), and
smoke-test the installed runtime. Verify both majors before dropping the shared
Darwin patch application entirely; a Linux-only pass is not removal evidence.

The previously observed Node 26 WPT child-process timeout is a separate incident;
this address-only patch is not a fix for that timeout.

## Node 26 Worker-exit backport

Upstream commit `bc5af1c67cd8e029f86ec992688dde9307f4d61a` synchronizes actual Worker
completion in the shared-isolate C++ test. It replaces a bounded number of
nonblocking event-loop turns with elapsed-time waiting that yields to the Worker,
without changing assertions, disabling tests, or adding retries. Apply it only to
Node 26. The patch is vendored as `lib/node-worker-exit.patch` because GitHub
can change generated patch metadata for the same commit, breaking a fetch hash.
Remove the backport when the selected release sources contain that commit
or an equivalent fix, and both shared-isolate variants pass the unchanged native
upstream suite without the extra patch.

## Validation

Check direct and transitive library wiring with:

```bash
nix build --no-link .#node-runtime-dependencies.x86_64-linux
```

The Linux check also runs the real React Native JS build phase against a tiny
read-only generated-file fixture. It stubs only the staging command and drops
its unused Rust/runtime path contexts, so it builds no SDK or Node sources.
The sandbox must reject a direct rename with `EXDEV`; publication must preserve
the exact tree contents and leave the read-only source intact. This regression
runs through the existing scoped-runtime CI check, without another job.

Repository lint CI also evaluates the check for every supported host. It checks scoped
libraries, Node-based build tools and wrappers, and isolation from the outer
package set. Keep it independent of Node source builds so dependency regressions
fail before compilation.

Build both final full Node outputs and their preserved version checks natively on
Linux and macOS, retaining upstream test targets, skip flags, and sandbox settings.
Run the actual installed runtimes for exact-version, Brotli round-trip, and
Ed25519 signing/verification checks. Linux builds or cross-platform evaluation
alone do not establish a Darwin pass. When only comments/documentation change,
verify the final derivation identities remain unchanged rather than rebuilding
the same source outputs.

Validate configuration with `dev/nix-shell 'just lint-config'` and dev fixtures
with `dev/nix-shell 'just test dev'`. A fixture blocked
by a local tool limitation is not a pass or a waiver; the unchanged required CI
gate must still pass in its supported environment.
