# GitHub workflows

Validate workflow edits with `dev/nix-shell 'just lint-config'`.
Messenger benchmarks run only through `benchmark-example-android.yml` with
`workflow_dispatch`, or the explicit Android Just command. Do not add benchmark
jobs or dependencies to normal test workflows. Keep host runner validators in
normal config tests.

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
The nightly libxmtp hub tag (`v<version>`, `release.yml`) uses the same tag
push job, so a `v*` tag ruleset can list the App as its bypass actor.
`check-release-push.yml` refuses a source commit that is not on `main`
or a `release/**` branch; dry runs skip that check.

Every `setup-nix` call states its Cachix cache mode (`read`, `ci`, or
`trusted`), and `setup-js` gets the same mode. Release, publish, deploy,
schedule, and post-merge jobs use `read`; CI jobs use `ci`; writer jobs use the
protected-push expression. See "Cache modes" in
`.github/actions/setup-nix/README.md`. `dev/tests/test_kache_env.py` checks
the wiring for every entry workflow and event.

Android CI jobs in mode `ci` restore Gradle User Home with
`gradle/actions/setup-gradle` (pinned by SHA), placed after `setup-nix`. Its
`cache-read-only` is the negated protected-push expression: protected trunk
pushes save the CI-only Gradle cache, and every other run only reads it.
Release, deploy, benchmark, and trusted jobs do not use it. The AAR that
`sdk-build` stages for the benchmarks is a CI artifact and never ships. Under `CI`, the Android recipes pass `--build-cache` and never
`--configuration-cache`; `Test` tasks are not cacheable, because they load the
host library and call the backend outside their task inputs. The job that runs
`check-consumers` sets `cache-cleanup: always`, because those compiles fail by
design.

Product groups (`products:` in `.github/ci-suites.yml`) each get one generated
`products_<group>` job in `test-generated.yml`. It builds
`.#ciSets.x86_64-linux.<group>` (listed in `nix/ci-sets.nix`) in mode `ci`, so
the suites that wait for the group substitute the products from `xmtp-pr`. A
suite still builds every product it uses: when the group did not run, its jobs
build the products themselves.

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
  It runs in cache mode `ci` without the `xmtp-pr` token, and fork pull requests
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

- `test-example-android.yml` owns app build, lint, host, and integration
  checks in two jobs: `messenger-build` (the APK derivation,
  then `just example-android check-all`: unit tests, lint, and every variant in
  one Gradle call) and `integration-tests`. App lint is in the test phase so
  that it can wait for the `sdk_android` products job.
- `test-android.yml` has two jobs: `sdk-build` (`just android check`, one
  Gradle call for `gradlew build` and the instrumented test APK, then
  `check-consumers` and the AAR staged over the Nix libraries) and
  `sdk-emulator` (API 34 tests, then the API 23 smoke). App-only changes do not select SDK
  checks; SDK and shared toolchain changes select app compatibility checks.
- `lint-example-ios.yml` owns example app lint. `test-example-ios.yml` owns
  the Debug and Push builds, unit tests, simulator journeys, and Nix launch.
  Keep both registered in `.github/ci-suites.yml`.
- Benchmark workloads require an explicit command or a `workflow_dispatch`
  workflow. Do not include them in normal test suites or generated CI suites.
  `benchmark-example-android.yml` owns the manual Messenger benchmark.

- Migration uses the normal SDK test targets. Browser platform proofs include
  migration. The iOS and Android unit jobs include their
  migration tests.

`release-android.yml` builds the example with
`dev/nix-shell 'just example-android apk'` before Maven publication.
After the isolated tag push, it attaches the test-signed
universal APK to the matching GitHub Release for every release type. Keep the
APK upload job dependent on the tag push and use `--verify-tag` on creation.
The APK build needs no signing secret. Its Nix derivation uses the public AOSP
testing key. The `messenger-build` job in `test-example-android.yml` builds that
derivation first; it uses its own pinned Gradle downloads, not the restored
Gradle User Home, and no backend.

## CI selection

`.github/ci-suites.yml` declares every CI suite: its workflow, `path_filters` (globs, Cargo
packages, and path groups), Kache scopes, and policy (`run_on`, `disable_on_forks`,
`secrets`, `permissions`, `covered_by`). Every suite key except `covered_by` is
required; nothing has a default. `secrets` is `inherit`, `kache` (only the
Kache keys), `cachix-pr` (only `CACHIX_PR_AUTH_TOKEN`), or `none`; use `none`
for a suite whose workflows read no secret. Example iOS suites use `cachix-pr`
and declare that optional secret in their reusable workflows. They receive no
release App or trusted-cache credentials.
Edit only that file. Then run
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
and `Draft checks`. A merge to `main` runs every suite with
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

The `lint_deny` suite (`lint-deny.yml`) runs `just lint-deny` (cargo-deny
bans, licenses, and sources) and the advisories check for Cargo lock and
manifest or deny configuration changes, on drafts too, and on every merge. An
advisory does not fail a pull request or merge; the daily scheduled run on the
default branch fails on one. A backend change selects the backend suites and
the client suites `test_workspace`, `test_wasm`, `test_node`, and
`check_sdk_unit`; the Android, iOS, browser, agent, and SDK platform suites run
on the merge. Android lint has no suite of its own: `sdk-build` in
`test-android.yml` runs `gradlew build`, which includes it.
Source lint does not generate SDK products or run compiler checks. Test owns
full types, full lint, Clippy, SDK and runtime checks. Pull requests that only
change Rust core crates omit the SDK and mobile suites; the merge run covers
them. The standalone Rust reference keeps rustdoc and glossary checks for Rust
changes. Automatic PR backend image checks use amd64; two-architecture
publication runs only on main, tag pushes, or reusable calls.

All CI commands select a targeted Nix shell. Full root Nix warming still builds
all outputs and dependencies, including the default developer shell. Kache is
off unless a job sets `kache: "true"`; its Darwin helper bypasses linked outputs.
Recovery runs only through the manual workflow. The release `load` jobs of
`build-sdk-node-platforms.yml` run the Windows installed-package smoke.

## Compiler cache

`setup-nix` uses a private local Kache store and disables GitHub compiler-cache
archives. Kache is off by default. Set `kache: "true"` only in a job that runs
cargo outside Nix; a job that only runs `nix build` passes no Kache input and
selects no writer environment. All enabled callers pass S3 settings with a
stable build scope. The setup action's README lists each scope and its access
mode, and `dev/tests/test_kache_env.py` checks the wiring.
`.github/actions/setup-nix/kache-ci config` requires a complete key pair, bucket, and build scope for
S3. No keys selects local-only; a partial pair fails before the action starts.

Audited compiler jobs select `kache-s3-writer` only for protected branch pushes to
`main`. Other events use an empty environment name and select
no GitHub environment. Same-repository PRs use repository reader secrets; fork
PRs build local-only. Writer secrets belong only in the restricted writer
environment. Do not create a reader environment.
Existing deployment jobs retain their environments and use reader access.
Tags, dispatches, and jobs that build a selected ref cannot write. Source lint,
docs quality and composition, Nix output warming, and manual recovery are
readers or leave Kache off. The backend suite uses `secrets: inherit`. Keep
the native backend acceptance job free of cache writer keys: pass only the
Kache reader pair to its setup-nix step, and do not reference other secrets in
that job.

When adding an enabled caller, add it to the setup action's README table and
to `KACHE_CALLERS` in `dev/tests/test_kache_env.py`. Pass both optional Kache
secrets through every reusable call in its chain. Declare them under
`workflow_call.secrets` when a caller uses an explicit secret map. Share a
stable scope across the same outer
Cargo build family, including reader-only consumers. Keep Clippy, check, test,
doc, and release variants separate. Jobs with no outer Cargo compilation leave
Kache off; Nix derivations do not use this remote. Identical test shards share
a scope. The action adds runner OS and architecture.
Keep credentials out of global `AWS_*` settings and Nix derivations.

`just lint-config` tests CI selection, result gates, summaries, the backend
selector, and write policy without keys, a compiler daemon, or cloud access.
See the setup action's README for settings.
