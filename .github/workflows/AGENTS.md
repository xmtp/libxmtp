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
- Failed Compose startup retains project logs and container state in
  `$RUNNER_TEMP/backend-startup-logs`. The Android SDK check uploads these for
  7 days; retain the same directory when adding diagnostics to other callers.

- The iOS jobs and `test-swift-lifecycle.yml` use disposable native
  services through `dev/nix-shell 'just backend ci COMMAND'`. Each job creates
  its own database and S3 bucket. Failed-job-only reruns do not need a
  deployment job.
- Keep the `ci.yml` selector's Swift and native input routes current when
  native setup inputs change.
  `test-native-backend.yml` checks wrapper cleanup and the real S3 contract.
  It also checks the owned loopback listeners and metrics endpoint. Backend
  source and build-input changes select this job, and it gates aggregate
  `Test`. The native acceptance job has no cache-write token.
  Service logs are retained for 7 days.
- Selected iOS and Android jobs gate aggregate `Test` in `ci.yml`.
  `test-swift-lifecycle.yml` owns the Swift lifecycle checks (`just ios test-lifecycle`)
  and Swift consumer checks. `test-android.yml` owns Kotlin consumer checks.
  `test-ios` runs `just ios test skip-lifecycle`, so it does not repeat the lifecycle.

## CI selection

`ci.yml` owns required Lint and Test. Its pinned dorny filters use PR changed
files or the push event's before SHA. `.github/ci-paths.yml` names filters after
checks. `dev/ci-select` reads Dorny's `changes` output and builds one plan for
scheduling and result gates. PRs with more than 3,000 changed files, unavailable
detection, unknown paths, and shared inputs select all checks. The PR file total
comes from event metadata or the PR API. Rename-expanded path counts do not
select full validation. Empty diffs are valid.
The job summary lists matched filters, full-run reasons, shared and unknown
paths, policy exclusions, selected checks, and required jobs. Lists show up to
200 entries. JSON path data goes through standard input to preserve quotes and
line breaks and avoid environment size limits.
The static source and runtime routers use fail-fast matrices and require the
selected child result. Selected skipped, failed, cancelled, or missing jobs
cannot pass. Direct reusable calls default to all checks.

Explicit draft PRs run only path-selected source checks and docs quality.
Their aggregates are named `Draft lint` and `Draft checks`; they do not produce
the merge check names `Lint` and `Test`. Ready transitions restore normal
selection. Missing draft state, pushes, and manual runs use the normal policy.
Draft Cargo-Deny checks run for Cargo lock/manifests, deny configuration, or
scanner workflow changes. Ready PRs keep all four Cargo-Deny checks.

Source lint does not generate SDK products or run compiler checks. Test owns
full types, full lint, Clippy, SDK and runtime checks. Pure Rust PRs omit host
language checks; post-merge runs retain language units and consumers. Platform
packaging uses native inputs. Full docs use `docs/`, the docs app, examples, and
public API inputs. Files under `docs/` select docs builds and are excluded from
shared inputs.
`sdks/js.just` selects Node, Browser, and Agent SDK checks, including generated
JS SDK products. Gradle and Kotlin build scripts select Android checks only;
copies under `docs/` select docs builds. Android build scripts do not select public API docs.
The standalone Rust reference keeps rustdoc and glossary checks for Rust changes.
Automatic PR backend image checks use amd64; two-architecture publication runs
only on main, self-hosted, tag pushes, or reusable calls.

All CI commands select a targeted Nix shell. Full root Nix warming still builds
all outputs and dependencies, including the default developer shell. Kache is
the default compiler wrapper; its Darwin helper bypasses linked outputs.
Recovery runs only through the manual workflow. Windows installed-package
smoke remains a manual owner in `test-sdk.yml`.

## Compiler cache

`setup-nix` uses a private local Kache store and disables GitHub compiler-cache
archives. Native Clippy and the Darwin SDK check are the S3 pilot callers.
Other callers stay local-only until the pilot's live checks pass.
`dev/kache-ci-config` requires a complete key pair, bucket, and build scope for
S3. No keys selects local-only; a partial pair fails before the action starts.

The pilot jobs select `kache-s3-writer` only for protected branch pushes to
`main` or `self-hosted`. Other events use an empty environment name and select
no GitHub environment. Same-repository PRs use repository reader secrets; fork
PRs build local-only. Writer secrets belong only in the restricted writer
environment. Do not create a reader environment.
Existing deployment jobs retain their environments and use reader access when
added later. Tags, dispatches, and jobs that build a selected ref cannot write.
Keep the native backend acceptance job free of cache writer keys.

The two pilot workflows retain redacted JSON Kache reports for seven days.
Keep their compiler commands and scopes unchanged when checking reader reuse.
Reports are recorded before the Kache action's post step; its final summary
includes later transfers. Failed setup skips report collection, and missing
reports or report-service errors do not change the compiler result. A missing
report leaves cache-reuse acceptance incomplete even when the build passes.

`just lint-config` tests CI selection, result gates, summaries, the backend
selector, and write policy without keys, a compiler daemon, or cloud access.
See the setup action's README for settings.
