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

- The jobs in `test-ios.yml` use disposable native services through
  `dev/nix-shell 'just backend ci COMMAND'`. Each job creates its own database
  and S3 bucket. Failed-job-only reruns do not need a deployment job.
- Keep the `native_backend` and `swift` path groups in `.github/ci-suites.yml`
  current when native setup inputs change.
  The `native` job in `test-backend.yml` checks wrapper cleanup and the real S3
  contract. It also checks the owned loopback listeners and metrics endpoint.
  It does not pass a cache-write token to setup-nix, and fork pull requests
  skip it. Service logs are retained for 7 days.
- The Linux job in `test-backend.yml` builds `.#backend-tests.x86_64-linux`.
  The derivation starts its own PostgreSQL primary, streaming replica, and
  VersityGW in the build sandbox, so the job starts no Docker services. Cachix
  keeps a passed result. A run with unchanged inputs does not test again.
- `test-xdbg.yml` owns the observability check. It runs xdbg against the
  Docker stack, so backend changes also select the xdbg suite.
- `test-ios.yml` owns the Swift lifecycle checks (`just ios test-lifecycle`)
  and Swift consumer checks. `test-android.yml` owns Kotlin consumer checks.
  The iOS `tests` job runs `just ios test skip-lifecycle`, so it does not
  repeat the lifecycle.

## CI selection

`.github/ci-suites.yml` declares every CI suite: its workflow, `path_filters` (globs, Cargo
packages, and path groups), Kache scopes, and policy (`run_on`, `disable_on_forks`,
`secrets`, `permissions`, `covered_by`). Every suite key except `covered_by` is
required; nothing has a default. Edit only that file. Then run
`dev/nix-shell 'python3.11 dev/ci-suites generate'` and commit the generated
`.github/ci-suites.json`, `lint-generated.yml`, and `test-generated.yml`.
`just lint-config` runs `dev/ci-suites check`. It fails on stale generated
files, a wrong Kache scope, a suite workflow that takes inputs, a tracked file
that no rule names, and a pattern that matches no tracked file.
`dev/ci-suites explain PATH...` shows what a path selects.
`dev/ci-suites replay [N] [REF]` replays recent commits as pull requests.

Each suite is one reusable workflow without inputs. It runs all of its jobs.
`ci.yml` lists the changed files with a pinned dorny step, and
`dev/ci-select` selects the suites. `ci.yml` then calls `lint-generated.yml`
with the selected lint suites and `test-generated.yml` with the selected test
suites. Each generated workflow has one job per suite and a `required` job.
That job fails when a selected suite did not succeed, including when it was
skipped. The `Lint` and `Test` jobs in `ci.yml` pass only when detection and
their call succeed. Branch protection on `main` requires them by name.

Pull request pushes run the suites whose inputs changed. Draft pushes run only
suites with `draft_pr_push` in `run_on`; their aggregates are named `Draft lint`
and `Draft checks`. A merge to `main` or `self-hosted` runs every suite with
`merge` in `run_on`. A manual run runs every suite. These cases also run every
eligible suite: a shared input, a path that no rule names, more than 3,000
changed files, and unavailable detection. Rename-expanded path counts do not
select a full run. Empty diffs are valid. Fork pull requests skip suites with
`disable_on_forks`. A suite with `covered_by` does not run when its cover runs.
A `neutral` file selects a suite only through a repository-wide glob that
starts with `**/`, such as `**/*.md` or the formatter's file types. A pull
request that deletes or renames a file also runs the `on_deleted_path`
suites, because `dev/ci-suites check` can then find a pattern that names no
file. A new pull request push cancels the older run; merge runs are not
cancelled.

The job summary lists the full-run reasons, matched suites, shared and unknown
paths, policy exclusions, and the selected suites. Lists show up to 200
entries. JSON path data goes through standard input to preserve quotes and
line breaks and avoid environment size limits.

Draft Cargo-Deny checks run for Cargo lock/manifests, deny configuration, or
scanner workflow changes. Ready PRs keep all four Cargo-Deny checks.
Source lint does not generate SDK products or run compiler checks. Test owns
full types, full lint, Clippy, SDK and runtime checks. Pull requests that only
change Rust core crates omit the SDK and mobile suites; the merge run covers
them. The standalone Rust reference keeps rustdoc and glossary checks for Rust
changes. Automatic PR backend image checks use amd64; two-architecture
publication runs only on main, self-hosted, tag pushes, or reusable calls.

All CI commands select a targeted Nix shell. Full root Nix warming still builds
all outputs and dependencies, including the default developer shell. Kache is
the default compiler wrapper; its Darwin helper bypasses linked outputs.
Recovery runs only through the manual workflow. Windows installed-package
smoke remains a manual owner in `test-sdk.yml`.

## Compiler cache

`setup-nix` uses a private local Kache store and disables GitHub compiler-cache
archives. All enabled callers pass S3 settings with a stable build scope.
Keep callers with `kache: false` disabled and free of S3 inputs. The setup
action's README lists each scope and its access mode.
`dev/kache-ci-config` requires a complete key pair, bucket, and build scope for
S3. No keys selects local-only; a partial pair fails before the action starts.

Audited compiler jobs select `kache-s3-writer` only for protected branch pushes to
`main` or `self-hosted`. Other events use an empty environment name and select
no GitHub environment. Same-repository PRs use repository reader secrets; fork
PRs build local-only. Writer secrets belong only in the restricted writer
environment. Do not create a reader environment.
Existing deployment jobs retain their environments and use reader access.
Tags, dispatches, and jobs that build a selected ref cannot write. Source lint,
docs quality and composition, Nix output warming, and manual recovery are
readers. The backend suite uses `secrets: inherit`. Keep the native backend
acceptance job free of cache writer keys: pass only the Kache reader pair to
its setup-nix step, and do not reference other secrets in that job.

When adding an enabled caller, pass both optional Kache secrets through every
reusable call in its chain. Declare them under `workflow_call.secrets` when a
caller uses an explicit secret map. Share a stable scope across the same outer
Cargo build family, including reader-only consumers. Keep Clippy, check, test,
doc, and release variants separate. Jobs with no outer Cargo compilation share
`nix-only`; Nix derivations do not use this remote. Identical test shards share
a scope. The action adds runner OS and architecture.
Keep credentials out of global `AWS_*` settings and Nix derivations.

`just lint-config` tests CI selection, result gates, summaries, the backend
selector, and write policy without keys, a compiler daemon, or cloud access.
See the setup action's README for settings.
