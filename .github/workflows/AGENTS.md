# GitHub workflows

Validate workflow edits with `dev/nix-shell --shell rust 'just lint-config'`.

SDK release tags use `GH_APP_ID` and `GH_APP_PK` for the release App.
The App installation must grant Contents write and Workflows write.
App tokens are available only in isolated permission and tag push jobs.
These jobs do not run SDK or release tools. The tag push job imports a Git
bundle into a new bare repository.
Resolve the requested ref before SDK code runs. All SDK jobs use that fixed
source SHA for checkout, build, and tag checks. Keep the requested ref for
version classification. The version CLI gets its hash from the pinned HEAD.
Android and npm tags must point to that SHA. iOS can use one direct
release commit
that changes only the iOS release files. The push job checks this before it
creates its token.
Npm dry runs resolve the source but do not create an App token or push a tag.
`just lint-config` checks the tag transfer with a local Git HTTP remote.

- Do not enable full Nix build logs by default in CI. For explicit debugging, run `nix log <drv-path>` or add `--print-build-logs` to a manual `nix build` command.
- Pass JavaScript shard flags directly to the `just` recipe. An extra `--` is forwarded to Vitest and prevents sharding.

- The iOS jobs and the Swift job in `test-sdk.yml` use disposable native
  services through `dev/nix-shell 'just backend ci COMMAND'`. Each job creates
  its own database and S3 bucket. Failed-job-only reruns do not need a
  deployment job.
- Keep the Swift job filters (`ios`, `ios_direct`, `sdk_swift`) current when
  native setup inputs change.
  `test-native-backend.yml` checks wrapper cleanup and the real S3 contract.
  It also checks the owned loopback listeners and metrics endpoint. Backend
  source and build-input changes select this job, and it gates aggregate
  `Test`. The native acceptance job has no cache-write token.
  Service logs are retained for 7 days.
- Selected iOS and Android lint and test jobs gate `Lint` and `Test`.
  The Swift seam job runs `just ios test-seams` and `just ios check-consumer`
  without a Linux product dependency. Keep the upstream same-repository PR
  rule. `test-ios` runs `just ios test skip-seams` and example checks for broad
  Swift inputs. Specific native inputs add simulator checks. The required seam
  job owns the seam proofs. Android unit and consumer checks stay broad.
  The Android unit job runs `just android check-consumers` even when AAR staging
  is not selected. Specific native inputs select both emulator jobs and all-ABI
  packaging. Direct reusable calls default to all checks. Each selected Android
  child result must succeed; selected skipped or missing work cannot pass.
  The SDK runtime matrix keeps bridge runtime and Browser platform proofs in
  separate rows. The platform job builds the exact debug panic and release
  pure fixtures. Do not replace them with the public default SDK product.

`fh-cache.yml` selects full warming from a successful push run of the same
workflow at the push ancestor. It keeps both host builds and treefmt checks.
Only verified prose and handwritten JavaScript paths can skip warming. The
selector pins the audited Rust, generator, and Nix input contract. A changed
contract keeps full warming until its skip paths and pin have been reviewed.
Run `python3 -B dev/ci/test-nix-output-selection.py` for local selection and
workflow command fixtures. They do not build packages or call GitHub.

Darwin always runs the full warming flow. Its Xcode and Apple SDK inputs are
impure. The source-only ancestor proof cannot prove those host inputs match.
Linux can skip warming only when the selector proves its input contract.

Every CI call to `dev/nix-shell` or a Just recipe must select a targeted shell
at workflow, job, or step scope, or use `--shell`. Keep matrix shell values
explicit. Reusable workflows do not inherit the caller's workflow environment.
`setup-js` always uses `js-node` for pnpm lookup and installation. It keeps
installation scripts enabled. SDK source staging uses `rust` for generation.
Browser execution uses `js`; documentation tests use `docs`; native platform
commands use `android` or `ios`.
Full JavaScript type and lint checks use `js`. Astro renders Mermaid diagrams
with the pinned Chromium browser during those checks.
Run `dev/nix-shell --shell rust 'python3.11 -B dev/ci/check-targeted-shells.py && python3.11 -B dev/ci/test-targeted-shells.py'`
to check workflow and composite inheritance. The fixtures use command stubs.
Nix output warming builds and caches the complete root flake, including
`devShells.<system>.default`, so developers can use the full environment locally.
This is separate from the targeted shells that execute CI build and test commands.
Keep the root lockfile check and all warming outputs.

Run `37685867750` proved cold production Node and Browser builds and fresh
installed-package loads on Linux. All five raw roles built in targeted shells.
Swift and Kotlin source generation on Linux does not prove an Apple platform
library build. The temporary proof workflow and guard were removed after this run.

`docs-rust-reference.yml` keeps the Rust reference and glossary checks in one
reusable job. Full site builds call it through `deploy-docs.yml`. A selected
Rust-only change calls it directly, without Node or Browser product jobs.
Select one route per run and require its result in the `Test` gate. Both routes
retain the exact reference key, byte stamp, and `docs-rust` artifact. Keep the
immutable Nix compiler tools so the exact reference cache can remain eligible.
This job sets `kache: "false"` because it reuses the complete reference output,
and custom compiler wrappers are not qualified reference inputs. It keeps
`CARGO_INCREMENTAL=0`. Other direct Cargo jobs keep the default Kache policy.
Reference schema 3 pins the policy and native provenance helpers. External
Cargo configuration, unknown compiler flags, and ignored glossary source files
disable reuse. Clean supported Rust inputs can reuse references. Kotlin and
Swift references build fresh until their global tool inputs are qualified.
SDK producer metadata retains compiler cache counts and immutable wrapper
identity. It omits raw events and old build times for reused raw roles.

`lint.yml` and `test.yml` use selected suite matrices with `fail-fast: true`.
Their fixed target routers validate each selected child result. Cancelled,
missing, or skipped selected work must fail the required gate. Compiler,
generated SDK, docs, and platform jobs outside these matrices can continue.
Keep cleanup and artifact retention bounded; matrix cancellation is not instant.
The hosted nested proof `37676422267` stopped the inner sibling about 45 seconds
after the failure marker and outer siblings about 105 seconds after it. This is
one controlled result, not a cancellation-time guarantee. Caller gates and
cleanup add time before a suite failure can stop its siblings.

`manual-sdk-recovery.yml` is the only CI owner of public Node recovery tests.
It runs only on `workflow_dispatch`, builds current Node and backend products,
runs all eight cases in isolated stacks, and tears down each stack. Normal
suite routing must not call it or add recovery to `test-node-sdk.yml`.
