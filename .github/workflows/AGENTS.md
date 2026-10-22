# GitHub workflows

Validate workflow edits with `dev/nix-shell 'just lint-config'` and
`dev/nix-shell 'just test dev'`.
Keep host runner validators in the dev tool tests.

SDK release tags use `GH_APP_ID` and `GH_APP_PK` for the release App.
The App installation must grant Contents write and Workflows write.
App tokens are available only in isolated permission and tag push jobs.
These jobs do not run SDK or release tools. The tag push job imports a Git
bundle into a new bare repository.
Resolve the requested ref before SDK code runs. All SDK jobs use that fixed
source SHA for checkout, build, and tag checks. `release.yml` freezes the
source in `validate` and computes every selected SDK version there, once,
from the requested ref (for dev classification) and one run timestamp. It
passes the source SHA and the version to each SDK workflow; the backend
computes its own version after it pins its source. Only a direct dispatch of
`release-agent-sdk.yml` or `release-cli.yml` computes its version in the SDK
workflow. The version CLI gets its hash from the pinned HEAD.
Each SDK workflow runs `check-release-push.yml` once before it builds.
`npm-publish.yml` takes the checked `source-sha` from its caller. Its publish
job authorizes itself as well: a real publish runs in the `release`
environment (for its branch policy; the job reads no App secret) and checks
that the source is on main or a release/** branch, so another branch's
workflow cannot call it to publish.
Android and npm tags must point to that SHA. iOS can use one direct
release commit
that changes only the iOS release files. The push job checks this before it
creates its token.
Npm dry runs resolve the source but do not create an App token or push a tag.
`just test dev` checks the tag transfer with a local Git HTTP remote.
`check-release-push.yml` refuses a source commit that is not on `main`
or a `release/**` branch; dry runs skip that check.
There is no scheduled release. A backend release also builds the xdbg image
and publishes it with the backend version as its tag (`release-xdbg` in
`release.yml`). Nothing deploys it.

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
Release, deploy, and trusted jobs do not use it. Under `CI`, the Android recipes pass `--build-cache` and never
`--configuration-cache`; `Test` tasks are not cacheable, because they load the
host library and call the backend outside their task inputs. The job that runs
`check-consumers` sets `cache-cleanup: always`, because those compiles fail by
design.

Product groups (`products:` in `.github/ci-suites.yml`) each get one generated
`products_<group>` job in `ci.yml`. It builds
`.#ciSets.x86_64-linux.<group>` (listed in `nix/ci-sets.nix`) in mode `ci`, so
the suites that wait for the group substitute the products from `xmtp-pr`. A
group runs only when the run has `CACHIX_PR_AUTH_TOKEN`. A
suite still builds every product it uses: when the group did not run, its jobs
build the products themselves.

- Do not enable full Nix build logs by default in CI. For explicit debugging,
  scope `XMTP_NIX_BUILD_LOGS=1` to the `dev/nix-shell` invocation that needs them,
  run `nix log <drv-path>`, or add `--print-build-logs` to a manual `nix build`
  command. Keep routine workflow setup quiet.
- Pass JavaScript shard flags directly to the `just` recipe. An extra `--` is forwarded to Vitest and prevents sharding.
- Failed Compose startup retains project logs and container state in
  `$RUNNER_TEMP/backend-startup-logs`. The Android SDK check uploads these for
  7 days; retain the same directory when adding diagnostics to other callers.

- The jobs in `test-ios.yml` use disposable native services through
  `dev/nix-shell 'just backend ci COMMAND'`. Each job creates its own database
  and S3 bucket. Failed-job-only reruns do not need a deployment job.
- Keep the `native_backend` and `swift` path groups in `.github/ci-suites.yml`
  current when native setup inputs change. Native backend changes select
  `test_ios`, the only run of the backend and the test services on macOS
  before a merge. `dev/tests/test_services.py` (in `just test dev`) checks the
  services contract on Linux.
- `test-nix-checks.yml` builds `.#validation`, `.#backend-tests`, and the
  xdbg and keepalive-probe Clippy derivations in one `nix build --keep-going`.
  Each stays its own derivation: Cachix keeps a passed result, so a check with
  unchanged inputs does not run again. The backend tests start their own
  PostgreSQL primary, streaming replica, and VersityGW in the build sandbox.
- `test-backend.yml` is the Docker backend job: it builds the backend image,
  checks its architecture, starts it with only the database, then starts the
  full stack and runs the observability check (xdbg traffic, traces, metrics,
  and the dashboard). Manual runs add an arm64 leg.
- `test-ios.yml` owns the Swift lifecycle checks (`just ios test-lifecycle`)
  and Swift consumer checks. `test-android.yml` owns Kotlin consumer checks.
  The iOS `tests` job runs `just ios test skip-lifecycle`, so it does not
  repeat the lifecycle.

- `test-example-android.yml` owns app build, lint, host, and integration
  checks in two jobs: `messenger-build` (the APK derivation,
  then `just example-android check-all`: unit tests, lint, and every variant in
  one Gradle call, with no backend) and `integration-tests`. Its `emulator`
  input picks one job, so two suites call the file:
  `test_example_android` and `test_example_android_emulator`. Both follow
  decision D1 (see Tiers below) and block on merges. App lint is in the test phase so that it can wait for the
  `sdk_android` products job.
- `test-android.yml` has two jobs: `sdk-build` (`just android check`, one
  Gradle call for `gradlew build` and the instrumented test APK, then
  `check-consumers`) and `sdk-emulator` (API 34 tests, then the API 23 smoke
  on merge and manual runs only). App-only changes do not select SDK checks;
  the Kotlin SDK and shared toolchain changes select the app as advisory.
- `test-example-ios.yml` owns example app lint, unit tests, and the Debug and
  Push builds in its `tests` job, and the simulator journeys in its
  `journeys` job. `test-ios.yml` runs SwiftLint and SwiftFormat for the SDK
  in its `tests` job; no macOS lint job exists.
- `lint-example-react-native.yml` owns the React Native example's typecheck,
  Oxlint, Oxfmt and unit tests (`just example-react-native check` against the
  JS-only SDK product). It runs on drafts. Native builds are local.
- Migration uses the normal SDK test targets. Browser platform proofs include
  migration; the Rust SDK tests cover the iOS and Android migration paths.

`release-android.yml` builds the example with
`dev/nix-shell 'just example-android apk'` before Maven publication.
Its build and publish jobs use the `release` environment for `OSSRH_USERNAME`,
`OSSRH_TOKEN`, `OSSRH_GPG_SECRET_KEY`, and `OSSRH_GPG_SECRET_KEY_PASSWORD`.
Use a Central Portal token for the username and token; no `MAVEN_PROFILE_ID`
is needed. Gradle prepares a signed Maven repository as a retained artifact.
The same signed publication goes to Maven Central and GitHub Packages at
`xmtp/libxmtp`. The caller grants `packages: write`, and the publish job uses
its own `GITHUB_TOKEN` for GitHub Packages.
The `publisher` Gradle project reads the retained signed files. Its GitHub
Packages job must succeed before the Maven Central job starts. Both registries
use Gradle's Maven publisher; the Nexus plugin manages Central staging and
release. The tag job waits for Central publication. Use **Re-run failed jobs**
to keep completed registry jobs from running again. Interrupted or ambiguous
uploads within a registry may need manual inspection and cleanup before retry;
the workflow does not promise automatic recovery of a partial upload. Never
rerun a successful Central job for the same version. The bundle, tag, and APK
artifacts are retained for 30 days.
After the isolated tag push, it attaches the test-signed
universal APK to the matching GitHub Release for every release type. Keep the
APK upload job dependent on the tag push and use `--verify-tag` on creation.
The APK build needs no signing secret. Its Nix derivation uses the public AOSP
testing key. The `messenger-build` job in `test-example-android.yml` builds that
derivation first; it uses its own pinned Gradle downloads, not the restored
Gradle User Home, and no backend.

## CI selection

`.github/ci-suites.yml` declares every CI suite: its title, workflow, filters
(globs, Cargo packages, path groups, and `pr_label` names joined with OR),
Kache scopes, and policy (`run_on`, `disable_on_forks`, `secrets`,
`permissions`). `covered_by`, `runners`, `with`, and `advisory_filters` are
optional; every other key is required and nothing has a default. `secrets` is
`inherit`, `kache` (only the Kache keys), `cachix-pr` (only
`CACHIX_PR_AUTH_TOKEN`), or `none`; use `none` for a suite whose workflows
read no secret. Example iOS suites use `cachix-pr` and declare that optional
secret in their reusable workflows. They receive no release App or
trusted-cache credentials.
Edit only that file. Then run
`dev/nix-shell 'python3.11 dev/ci-suites generate'` and commit the generated
`.github/ci-suites.json` and `.github/workflows/ci.yml`. Do not edit `ci.yml`
by hand. `just lint-config` runs `dev/ci-suites check`. It fails on stale
generated files, a wrong Kache scope, a missing or repeated title, a runner
label outside its allowlist, an input that the suite workflow does not declare
or a required one that the caller does not pass, a tracked file that no rule
names, and a pattern that matches no tracked file.
`dev/ci-suites explain PATH...` shows what a path selects, by tier.
`dev/ci-suites replay [N] [REF]` replays recent commits as pull requests.

`ci.yml` has the job `Select suites` (id `detect-changes`), the products
jobs, one caller job per suite, and the `Lint` and `Test` gates. Branch
protection on `main` requires `Lint` and `Test` by name. A check reads
`<suite title> / <job name>`. `Select suites` lists the changed files with a
pinned dorny step, and `dev/ci-select` selects the suites. Each caller runs
when its suite is selected. A suite without `with` inputs runs all of its
jobs. Each gate always runs. It fails unless detection passed, and fails when
a selected blocking suite of its phase did not succeed, including when it was
skipped.

Tiers: a match in `pr_filters` makes a suite blocking. The suite's workflow
files count as `pr_filters`, or as `advisory_filters` for a suite whose
`pr_filters` select nothing. A suite that only `advisory_filters` match runs as advisory, in a
second caller job (`<suite>_advisory`, named `<title> (advisory)`) that no gate
needs. A full run of a pull request runs a suite with `advisory_filters` as
advisory unless a `pr_filters` entry matched. When detection fails or a pull
request has more than 3,000 files, a suite that `pr_filters` can select
blocks. Merge and manual runs run every selected suite as blocking. The
example apps follow decision D1: the app's own paths, its workflow, and its
label block; the SDK the app consumes and the SDK and toolchain Nix inputs
(`example_ios_nix`, `example_android_nix`) run it as advisory; nothing else
selects it, so backend and service inputs and full runs leave it out
(`skip_full_runs`). A products job runs only for blocking suites, so the
Test gate never waits for advisory work.

`runners` maps a workflow input to a `{pr, merge}` pair of labels. The caller
passes the `pr` label on pull requests and the merge label on merge and manual
runs, so a manual run on a branch measures the merge runners; the suite
workflow uses it as `runs-on: ${{ inputs.NAME }}`. Merge and manual runs use
free GitHub-hosted runners, except the Gradle jobs, which write the Gradle
home that pull requests restore from Blacksmith's cache, and the macOS jobs,
which run on WarpBuild for pull requests and merges. On a GitHub-hosted
Linux runner, `setup-nix` first removes preinstalled toolchains to free disk.
`with` passes other inputs (for example a scope). The top-level `runners` key
sets the runners of `Select suites`, the products jobs, and the gates.

Pull request pushes run the suites whose inputs changed. Draft pushes run only
suites with `draft_pr_push` in `run_on`; test suites never do. On drafts the
gates are named `Lint (draft)` and `Test (not run on drafts)`, so a draft run
cannot satisfy the required checks. The Nix evaluation suite (`nix_eval`)
skips drafts. A merge to `main` runs every suite with `merge` in `run_on`. A
manual run takes a `suites` input, a comma-separated list of suite keys, and
runs those suites; an empty input runs every suite, and an unknown key fails
`Select suites`. A run of a subset names its gates `Lint (manual subset)` and
`Test (manual subset)`, so it cannot satisfy the required checks. These cases also run every eligible suite: a shared input, a
path that no rule names, more than 3,000 changed files, and unavailable
detection. Rename-expanded path counts do not select a full run. Empty diffs
are valid. Fork pull requests skip suites with `disable_on_forks`. A suite
with `covered_by` does not run when its cover runs in the same or a stronger
tier. A suite with `skip_full_runs` runs in a full pull request run only when
its own filters or labels match: the full RN device chain and the example
apps do, so other full runs get the Android smoke and no example app. A `neutral` file selects a suite only through a repository-wide glob
that starts with `**/`, such as `**/*.md` or the formatter's file types. A
pull request that deletes or renames a file also runs the `on_deleted_path`
suites, because `dev/ci-suites check` can then find a pattern that names no
file, and a Nix file can still name it. A new pull request push cancels the
older run. A running merge run is not cancelled, but GitHub keeps one pending
run per concurrency group, so a newer push to `main` replaces a merge run that
is still waiting: merge runs are latest-wins. Manual runs have their own group
and never replace a merge run.

Products need `CACHIX_PR_AUTH_TOKEN`. Fork runs have no secrets. Dependabot
runs have only Dependabot secrets, so the token reaches them once it is added
as a Dependabot secret. Without it, a run builds no product group and its
suites build the products themselves. Release, deploy, and post-merge jobs
never read `xmtp-pr`.

PR label additions and removals also recalculate selection. `pr_label` matches
the exact name in the event's `pull_request.labels`; it can select a suite
without any matching path. Path exclusions and neutral files do not block
label matches. Example app test suites declare their app directories, relevant
Nix inputs (`example_ios_nix` and `example_android_nix` path groups), and
platform labels (`iOS` and `android`); the automatic workflow and full-run
rules above still apply. Keep those Nix groups current when app packaging,
SDK products, platform shells, or test service inputs change. Product groups
keep path-only `path_filters`.

The job summary lists the full-run reasons, the requested suites of a manual
run, the suites matched by blocking and by advisory filters, labels, shared
and unknown paths, policy exclusions, the blocking suites of each phase, the
advisory suites, and the product groups. Lists show up to 200 entries. JSON
path data goes through standard input to preserve quotes and line breaks and
avoid environment size limits.

The `lint_config` suite (`lint-config.yml`) is the one Linux lint job: hakari,
`just lint-deny` (cargo-deny bans, licenses, and sources), buf, treefmt with
the CI declaration check and the release tools, Oxlint, Oxfmt, the docs script
tests, markdownlint, and the docs source check. It runs on drafts too, and on
every merge. Each check step runs even when an earlier one failed. The cargo-deny
advisories check runs only in the daily `lint-deny.yml` schedule on the
default branch, so a new advisory never fails a pull request or merge. A backend change selects the backend suites and
the client suites `test_workspace`, `test_wasm`, and `test_node`; the Android,
browser, and SDK platform suites run on the merge. `test_workspace` builds
with the `ci-test` Cargo profile: release speed with overflow checks and
debug assertions in the workspace crates. Android lint has no suite of its own: `sdk-build` in
`test-android.yml` runs `gradlew build`, which includes it.
Source lint does not generate SDK products or run compiler checks. Test owns
full types, full lint, Clippy, SDK and runtime checks. Pull requests that only
change Rust core crates omit the SDK and mobile suites; the merge run covers
them. The standalone Rust reference keeps rustdoc and glossary checks for Rust
changes. Automatic PR backend image checks use amd64; two-architecture
publication runs only on main, tag pushes, or reusable calls.

All CI commands select a targeted Nix shell. Full root Nix warming still builds
all outputs and dependencies, including the default developer shell. Kache is
off unless a job sets `kache: "true"`; other jobs get `KACHE_DISABLED=1`.
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
readers or leave Kache off.

When adding an enabled caller, add it to the setup action's README table. Pass both optional Kache
secrets through every reusable call in its chain. Declare them under
`workflow_call.secrets` when a caller uses an explicit secret map. Share a
stable scope across the same outer
Cargo build family, including reader-only consumers. Keep Clippy, check, test,
doc, and release variants separate. Jobs with no outer Cargo compilation leave
Kache off; Nix derivations do not use this remote. Identical test shards share
a scope. The action adds runner OS and architecture.
Keep credentials out of global `AWS_*` settings and Nix derivations.

`just test dev` tests CI selection, result gates, summaries, the backend
selector, and write policy without keys, a compiler daemon, or cloud access.
See the setup action's README for settings.

`check-react-native.yml` runs the RN unit checks (`check-package`, `test-react`,
`test-native`, `test-control`) on Linux with no device and blocks merges.
`test-react-native.yml` is the advisory device chain: components, the npm
package, and the installed bare/Expo apps. Its `scope` input is `full` or
`android-smoke` (the bare Android app). Pull request runs build the partial test
package (x86_64 Android, iOS simulator, version `-partial`, rejected by
`verify-package`); other events build the full package. The Android smoke
builds the partial package without iOS (package kind `test-android`) on every
event, so it runs no macOS job. Jobs with the `xmtp-pr`
token push each stage and the next stage substitutes it; forks and releases
exchange signed same-run closures (zstd). Receipts run only with the `debug`
input or a debug re-run.

Once GitHub registers the workflow, `gh workflow run test-react-native.yml --ref
REF` runs only the RN chain. Platform jobs retain
`target/react-native-results/*.json` in `react-native-proof-PLATFORM`; release
jobs use `react-native-release-proof-PLATFORM`, post-merge
`react-native-post-merge-proof-PLATFORM`. Do not upload peer logs with these
reports. The consumer builds retain 100 Nix failure lines so CocoaPods hook
exceptions remain visible before their Ruby stack traces.

The shared `react-native-product` action owns RN component exchange, package
assembly, and consumer builds. Callers pass the fixed source SHA, package
version (empty for the default), package kind, exchange choice, artifact prefix,
and cache mode. Cache setup, credentials, explicit uploads, and trusted writer
environments stay in the calling workflows. Native derivations select their
declared Xcode; the consumer action sets `DEVELOPER_DIR` for simulator commands
outside Nix. `react-native-conformance` executes fresh installed bare/Expo
proofs in test, post-merge, and release jobs. Post-merge `rn-*` cache jobs build
and push the RN stages, the test package, and the bare Android test app;
`rn-consumers` then builds, pushes, and pins the RN warm set
(`ciSets.<system>.rn`, pin `rn-<system>`), one leg for each system.
`rn-smoke` runs every journey (`XMTP_RN_JOURNEYS=full`) in mode `read` without
a token. `rn-alert` runs after the RN jobs whenever the run goes on, on a free
runner: it dry-runs the RN set of each system whose set `xmtp` holds
(`dev/ci/warm-closure`), so a platform that pinned is checked even when the
other leg failed, and posts a Slack message when an RN job or that check fails
or times out.

The device chain is advisory, so an RN break can reach trunk. It must hold
back only the RN jobs:

- `build`, `closure`, the docs jobs, and `deploy` wait for no RN job.
- The dev and release sets hold only the RN outputs that blocking pull
  request suites build: on Linux, the RN JavaScript layer (the JavaScript
  suites and the docs read it through `sdk_js`) and the `react-native` shell
  that `check_react_native` enters. No blocking suite builds RN on Darwin, so
  every Darwin RN output is in rn. `closure` checks every set but `rn`.
  `nix_eval` runs `dev/ci/rn-isolation`, which fails when a set other than
  `rn` builds an `rn` member.
- The RN cache jobs start after `build`, because the RN components share their
  native SDK builds with the Android and iOS release outputs: `build` compiles
  them once and the RN jobs substitute them. They also run when `build` fails.
  GitHub skips a job whose `if` calls no status function when any job before
  it, direct or not, did not succeed, so each RN job's `if` checks only the RN
  job before it, after `!cancelled()`: a cancelled run starts no RN job.

The smoke waits for `build`, which pushes the backend and test services that
it starts. `dev/tests/test_warm_sets.py` checks these rules with a model of
GitHub's job rules, and runs the warm, pin, closure, and isolation steps with
a fake `nix`, `cachix`, and `curl`. The test package outputs (`testStages`) are
not set members: the cache jobs warm the test packages and the bare Android
test app. No job warms the full-scope Android and iOS test apps, which only
full-scope RN pull requests build; warming them would add a paid app build to
every merge. `ciStages` takes its members from the warm sets, so declare a new
RN output in `rnAll` of `nix/ci-sets.nix`, and also in `rnBlocking` when a
blocking suite builds it on Linux.
