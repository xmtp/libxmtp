# CI selection

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
example apps use this selection policy: the app's own paths, its workflow, and its
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

## React Native device chain

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
proofs in test, post-merge, and release jobs.

Post-merge `rn-*` cache jobs build and push the RN stages, test package, and
bare Android test app. `rn-consumers` also builds, pushes, and pins each system's
`ciSets.<system>.rn` as `rn-<system>`. `rn-smoke` runs every journey
(`XMTP_RN_JOURNEYS=full`) in mode `read` without a token. `rn-alert` runs on a
free runner whenever the run is not cancelled: it checks each RN set already in
`xmtp` with `dev/ci/warm-closure`, including a successful platform when the other
failed, and alerts on RN failures, timeouts, or closure-check failure.

Keep advisory RN failures isolated from other post-merge work:

- `build`, `closure`, docs, and deploy wait for no RN job. Docs wait for the
  non-RN empty-store closure check.
- `dev` and `release` contain only RN outputs built by blocking PR suites:
  Linux's JS layer (`sdk_js`) and `react-native` shell (`check_react_native`).
  All Darwin RN outputs belong to `rn`.
- `closure` checks every set except `rn`; `rn-alert` checks `rn`. The `nix_eval`
  suite runs `dev/ci/rn-isolation` to reject non-RN sets that build an RN member.
- RN cache jobs wait for `build` to substitute shared SDK outputs but must still
  run if it fails. After `!cancelled()`, their conditions check only the prior RN
  job; GitHub's implicit success check would otherwise skip them. Smoke also
  waits for `build` for backend/services but can run after its failure.
- Declare new RN outputs in `rnAll` in `nix/ci-sets.nix`; add them to
  `rnBlocking` only when a blocking Linux PR suite builds them. `ciStages`
  derives from the warm sets.

The test package outputs (`testStages`) are not set members. Warm the test
packages and bare Android test app only; full-scope Android and iOS test-app
warm builds are too costly for every merge. `dev/tests/test_warm_sets.py` covers
job conditions and the warm/pin/closure/isolation commands with fake tools.
