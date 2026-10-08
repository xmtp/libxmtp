# GitHub workflows

Validate workflow edits with `dev/nix-shell 'just lint-config'`.

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
- `test-sdk.yml` gates aggregate `Test`; `test-ios` and `test-android` do not.
  The Swift seam proofs (`just ios test-seams`) and the Swift and Kotlin
  consumer checks run in `test-sdk.yml`. The `sdk` and `sdk_swift` filters
  list their paths under `sdks/`. `test-ios` runs `just ios test skip-seams`,
  so its macOS run does not repeat the seam proofs.

## CI selection

`ci.yml` owns required Lint and Test. Its selector uses the PR merge parent or
the push event's before SHA. Missing diffs and unknown build inputs select all
checks. The static source and runtime routers use fail-fast matrices and
require the selected child result. Selected skipped, failed, cancelled, or
missing jobs cannot pass. Direct reusable calls default to all checks.

Source lint does not generate SDK products or run compiler checks. Test owns
full types, full lint, Clippy, SDK and runtime checks. Pure Rust PRs omit host
language checks; post-merge runs retain language units and consumers. Platform
packaging uses native inputs. Full docs use docs, examples, and public API inputs.
The standalone Rust reference keeps rustdoc and glossary checks for Rust changes.

All CI commands select a targeted Nix shell. Full root Nix warming still builds
all outputs and dependencies, including the default developer shell. Kache is
the default compiler wrapper; its Darwin helper bypasses linked outputs.
Recovery runs only through the manual workflow. Windows installed-package
smoke remains a manual owner in `test-sdk.yml`.
