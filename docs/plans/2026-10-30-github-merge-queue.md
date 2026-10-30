# Move main validation into GitHub's merge queue

Move the validation policy currently run by `CI` on main pushes onto GitHub's
prospective merge groups, before they land. Keep fast pull-request checks and full
nightly coverage, and leave trusted cache warming, deployment, release publication,
and checks of the actual published artifacts after merge. Start with one queue
build at a time, then remove redundant main validation only after a measured pilot.

Status: proposed; approval authorizes the tasks below, not activation or a merge.
Source baseline: `origin/main` at `5a305a2b4c5ebd029c603e0398a0031ab70dc389`,
including merged PR [#4840](https://github.com/xmtp/libxmtp/pull/4840).
Research date: 2026-10-30. Owner: Nick Molnar; coordination: LIBX-4; plan: LIBX-9.

## Context and boundaries

The catalyst is Nick's request to validate a prospective merge before landing,
without moving deployment or cache publication into the queue. `restore` is not
synonymous with `publish`: substitution, pnpm/Gradle restoration, and local scratch
writes are necessary to run validators. Remote cache publication, trusted warming,
registry writes, deployments, and external coverage publication are different effects.

This is CI/tooling work. The specification index and SPEC's admission rules do not
identify a product contract changed here. Existing source-selection rules live in
`.github/CI.md` and `.github/ci-suites.yml`; cache rules live in
`.github/actions/setup-nix/README.md`; release rules live in `.github/RELEASING.md`.
No SDK, bindings, public API, database, wire protocol, or product policy changes are
intended. An unexpected public surface requires stopping and revising this plan.

The scope is the main-push **CI validation policy**, not every command run after a
merge. Post-merge closure availability checks and docs build/compose/browser checks
remain publication prerequisites on the exact artifacts about to ship. Full native
docs rendering on every candidate would undo #4840's deliberately deferred prose
rendering coverage; this plan does not add it. Likewise, full examples and RN device
tests stay nightly/manual or path/label-selected on PRs, not universal queue work.

### Observed GitHub configuration, not assumptions

Read-only REST queries show a public repository owned by an organization, with
default branch `main`. `GET repos/xmtp/libxmtp/rulesets/19701574` and
`GET repos/xmtp/libxmtp/rules/branches/main` expose the active `main` repository
ruleset: target `~DEFAULT_BRANCH`, deletion and force-push restrictions, required
PRs, squash as the allowed merge method, and required `Test` and `Lint` contexts
bound to integration ID `15368` (GitHub Actions). Strict up-to-date checks are off;
required approval count is zero; the querying identity cannot bypass. No
`merge_queue` rule appears in these responses. Preserve these existing review
policies rather than silently adding approvals or bypasses.

Legacy `GET .../branches/main/protection` still returns HTTP 403. Repository
merge-method flags are absent/null in the accessible metadata, not proven disabled.
Environment policies, secret placement, runner isolation, inherited controls not
exposed by these reads, and the admin UI's queue controls remain unverified. Nick
or a repository administrator must inspect them before activation; do not change
permissions merely to make the research credential read more.

Fresh official documentation says merge queues are available to public
organization repositories (this repository qualifies on those observed facts).
An admin still has to verify that the setting is available and compatible with
all effective rules. Use the existing repository ruleset rather than an
organization-level merge-queue rule or a wildcard branch-protection rule.

## Decisions

- Treat `merge_group.checks_requested` as a distinct event, with the same suite
  eligibility as today's main `merge` tier. Do not reuse PR path selection or
  interpret absence of `pull_request` metadata as a ready PR.
- Run **every** current `run_on: merge` suite on every queue candidate. Paths and
  labels do not narrow this set. This preserves main coverage and avoids a new
  batched-PR lookup service. Keep `merge` as the existing manifest policy name;
  explain that it means prospective-merge validation as well as shadow main runs.
- Use existing merge runner labels: GitHub-hosted Linux where currently used,
  Blacksmith for Android, WarpBuild for iOS. Do not move all queue jobs to paid PR
  runners or change Nix build parallelism without measurements.
- Queue callers pass no long-lived secret, select no writer environment, and
  request only `contents: read` (plus `pull-requests: read` in detection if retained
  for the PR path). Mode `ci` without its optional writer token reads both Cachix
  stores; Kache falls back to local storage. Do not introduce a new cache mode.
- Keep PR and nightly CI-only `xmtp-pr` writes as approved. The queue's tokenless
  path is an explicit first-rollout choice, not a claim that existing CI-only
  writes are trusted writes. Queue job artifacts and JUnit uploads remain allowed.
- Do not promote candidate caches or artifacts to trusted stores, even for equal
  Git trees. Rebuild/substitute from trusted sources at the final main SHA.
- Keep the entire current main CI run during the pilot. After cutover, retain
  only the four maintenance suites named below on main; their repeated validation
  is justified by existing cache/coverage writes, not treated as free savings.

Open decisions (recommendation first; neither is needed to finish this plan):

1. **Approve the conservative queue pilot**, with concurrency 1, squash, only
   non-failing PRs, and no queue cache writer credentials. Alternative: defer
   activation until runner/cache measurements justify a different configuration.
   Do not widen credentials as an implicit latency fix.
2. **Approve removing duplicate pure main validation after the pilot criteria
   below**, while retaining main cache/coverage maintenance. Alternative: retain
   full main validation longer and pay the duplication until confidence improves.
3. **Preserve release-requested integration using an isolated release-App queue
   admission job**, subject to admin verification of its Pull requests write
   permission and protected environment. Alternative: when the base requires a
   queue, finish publication and hand admission to Nick manually. Do not reuse
   `GITHUB_TOKEN` and assume it triggers downstream queue CI.

## Workflow and job disposition

All paths below are relative to the repository. Workflow filenames are under
`.github/workflows/`. A reusable workflow retains `workflow_call`; only the
generated `ci.yml` gets the queue trigger. Jobs inherit the caller's event/ref.

### CI suites, dependency graph, and selection

Current graph: `detect-changes` produces phase suite arrays and product arrays;
`products_sdk_js`/`products_sdk_android` prebuild only when the CI cache token is
available; selected suite callers wait for their declared products. A missing
product because there is no writer token is normal: each suite builds its own
inputs. `Lint` and `Test` depend on detection, their blocking callers, and relevant
products. Advisory callers are deliberately absent from their `needs` graph.

| Suite key → workflow / jobs | Current PR behavior | Queue validation | After cutover on main |
| --- | --- | --- | --- |
| `lint_config` → `lint-config.yml / lint` | Path-selected; also drafts; source/spec/Markdown, treefmt, deny except advisories | Always, unchanged commands | Remove duplicate |
| `nix_eval` → `nix-eval.yml / ci-sets` | Nix/data/tooling inputs; ready PR only; RN isolation | Always | Remove duplicate |
| `lint_example_react_native` → `lint-example-react-native.yml / lint` | RN paths/label; also drafts | Always, as on main now | Remove duplicate |
| `test_dev` → `test-dev.yml / test` | Tooling/config/dependency paths | Always | Remove duplicate |
| `check_rust` → `check-rust.yml / clippy` | Rust closure; native/generator/WASM Clippy | Always; local Kache, no S3 keys | Retain trusted `rust-clippy` warming using existing commands |
| `test_backend` → `test-backend.yml / backend-image` | Backend closure; amd64 local image/build tests; also dispatchable | Always; no image publication | Remove duplicate; image publication stays separate |
| `test_nix_checks` → `test-nix-checks.yml / check` | Backend/validation/Nix closure | Always | Remove duplicate |
| `test_workspace` → `test-workspace.yml / test` | Workspace/backend closure; Nix tests, JUnit, Codecov | Always; JUnit yes, Codecov upload no | Retain coverage generation/upload at main SHA; explicitly repeated tests |
| `test_wasm` → `test-wasm.yml / archive → partition` | Sensitive WASM/browser/storage/manifests, label `ci-wasm-full`: all four partitions | Always all four, same archive/dependency graph | Remove duplicate |
| `test_wasm_sample` → same workflow, scope `sample` | Ordinary relevant Rust/backend PR: one rotating partition | Not separately selected; full suite covers it | Not selected |
| `docs_rust` → `docs-rust-reference.yml / rust` | Rust closure; covered by `docs_site` when that runs | Always rustdoc/glossary; no trusted environment | Retain trusted `rust-reference` warming |
| `check_types` → `check-types.yml / check` | JS workspace/SDK paths; needs `sdk_js` when prebuilt | Always | Remove duplicate |
| `check_sdk` → `check-sdk-products.yml / generated` | SDK/package/binding inputs; needs both products | Always | Remove duplicate |
| `test_node` → `test-node-sdk.yml / test` | Node/agent/CLI/backend closure; `sdk_js` | Always | Remove duplicate |
| `test_browser` → `test-browser-sdk.yml / test` | Browser inputs; `sdk_js` | Always | Remove duplicate |
| `test_sdk_platform` → `test-sdk-platform.yml / proof` | Storage/worker/bridge closure; `sdk_js` | Always | Remove duplicate |
| `docs_site` → `deploy-docs.yml / rust + site → compose` | Site/API/tooling changes, not prose-only; `sdk_js`; no deploy | Not universal: absent from current main CI tier | Full actual publishing pipeline stays post-merge |
| `test_ios` → `test-ios.yml / tests` | Swift/SDK/Apple toolchain inputs | Always full iOS | Remove duplicate |
| `test_backend_darwin` → `test-backend-darwin.yml / smoke` | Backend-only Darwin service smoke; covered by full iOS | Covered by `test_ios`, not a second job | Not selected |
| `test_android` → `test-android.yml / sdk-build + sdk-emulator` | SDK/platform inputs; API 34; `sdk_android` | Always, including API 23 minimum-SDK smoke and proof artifact | Retain both jobs initially to refresh the same Gradle User Homes |
| `test_example_android`, `test_example_android_emulator` → `test-example-android.yml / messenger-build` or `integration-tests` | Own paths/platform label blocking; consumed SDK/toolchain advisory | Not selected | Not restored to every merge |
| `test_example_ios` → `test-example-ios.yml / tests + journeys` | Own paths/iOS label blocking; consumed SDK/toolchain advisory | Not selected | Not restored to every merge |
| `check_react_native` → `check-react-native.yml / unit` | RN/JS inputs; Linux package/react/native/control checks; `sdk_js` | Always | Remove duplicate |
| `test_react_native`, `test_react_native_smoke` → `test-react-native.yml / components → package → consumers` | RN device chain advisory; broad Android smoke opt-in `ci-rn-smoke`; full covers smoke | Not selected | Existing RN warmers stay, no devices |

Keep PR filters, labels, neutral paths, shared/unknown fallback, deletion handling,
`covered_by`, fork policy, advisory tiers, draft gate names, and manual subset gate
names unchanged. Every suite above remains available in the daily `09:17 UTC`
full run and manual full runs, subject to existing cover suppression. Examples and
full RN devices are blocking there; the standalone scheduled `lint-deny.yml / deny`
continues checking advisories, not PR/queue gates. `manual-sdk-recovery.yml / recovery`
remains manual. Standalone backend/RN/docs dispatches remain manual validators.

### Post-merge effects and release graph

| Workflow / jobs or steps | Disposition and ordering |
| --- | --- |
| `post-merge.yml / build` (Linux, Darwin, arm64) | Protected main only. Build trusted `dev`/`release` sets, complete uploads, push/pin `xmtp`. Never read `xmtp-pr`. |
| `post-merge.yml / closure` | After all `build` legs/post-upload steps: empty-store availability proof excluding RN. It measures the published cache, so cannot move before publication. |
| `post-merge.yml / pnpm-store` | Independent protected-main lockfile-key save, no install scripts; retain. PR/queue restoration is read-only. |
| `post-merge.yml / rn-components → rn-package → rn-consumers → rn-alert` | Retain trusted warming, test packages/bare Android warm app, RN set pins/closure alerts. `rn-components` waits for completion of `build` but still runs on its failure; later jobs require their RN predecessor. Other warmers/docs/deploy do not wait on RN. No new device tests. |
| `post-merge.yml / docs-swift + docs-kotlin → docs → deploy` | References wait for non-RN closure; docs receives same-run native artifacts, builds Rust/site, composes/checks/browser-tests/Lighthouse, uploads Pages artifact; only deploy has Pages/OIDC write. Keep these checks on final published bytes, including the custom-domain health warning. |
| `ci.yml` main maintenance (new selection, same workflow identity) | `check_rust`, `docs_rust`, `test_android`, `test_workspace` only. Retain current protected-main Kache/Gradle writes and main Codecov reporting. Keep this in `ci.yml` so Gradle's workflow/job-key identity does not silently lose its reusable homes. Name aggregate contexts `Lint (main maintenance)` / `Test (main maintenance)` after cutover; never use them as required contexts. |
| `push-backend.yml / source → publish(matrix) → manifest → deploy-dev` and `deploy-backend-dev.yml / deploy` | Keep current source pinning, two-architecture GHCR publication on main pushes or release calls (no tag trigger), then guarded main-only Fly deploy. Never a queue trigger/caller. Preserve superseded-main guards. |
| `deploy-web-chat.yml / deploy` | Keep main push/manual build and Vercel publish, trusted reads and current-main guard. No queue deploy or deployment requirement for admission. |
| `push-keepalive-probe.yml / build` | Keep path-selected main/manual image publication and immutable SHA tags/current-head latest-tag guard. It is not validation coverage to transfer. |
| `release.yml / validate → selected release-* → merge-pr → notify` | Manual release orchestration remains independent. Fixed source SHA/version, existing `no-merge`/dry-run options and release-specific checks stay. Update `merge-pr` semantics as described below: queue acceptance is not completion. |
| `release-node-sdk.yml`, `build-sdk-node-platforms.yml` | Preserve permission check → pinned platform builds → assembly → installed-package `load` smoke → npm publish/tag. No queue release build. |
| `release-browser-sdk.yml`, `release-agent-sdk.yml`, `release-cli.yml` | Preserve pin/version/setup/build → `npm-publish.yml / publish → push-tag`; agent/CLI ordering behind selected Node release stays. |
| `release-react-native-sdk.yml` | Preserve permissions → components → package build → installed consumer proofs → publication. Same-run signed exchanges stay separate from CI caches. |
| `release-android.yml` | Preserve bundle/APK preparation → GitHub Packages → Central → isolated tag → APK attachment. No repeated successful Central publish; ambiguous uploads require operator inspection. |
| `release-ios.yml` | Preserve permission check → build/package → allowed release-only commit → isolated tag → publication. |
| `release-backend.yml`, `release.yml / release-xdbg` | Preserve pinned image/release publication; xdbg publication follows successful backend release, no new deployment. |
| `check-release-push.yml`, `push-release-tag.yml`, `npm-publish.yml` | Keep source branch admission, isolated App token jobs and release environment constraints. Queue refs are not a release source. |
| `create-release-branch.yml`, `release-notes.yml` | Keep manual release-branch creation and release-branch push/manual/call notes update/redispatch. No main queue triggers or generic new automation. |

A release's existing `gh pr merge --squash` call targeting a queued branch may
enqueue, or implicitly enable auto-merge when required checks have not passed;
omitting `--auto` does not prevent that behavior. Retain the operator's explicit
release intent and `no-merge` escape hatch, but replace this path when the PR's
GraphQL `isMergeQueueEnabled` is true with `enqueuePullRequest`, passing the PR's
node ID, `expectedHeadOid` equal to the verified head, and `jump: false`. A changed
head, unmet requirement, API error, or absent returned queue entry stops admission;
do not fall back to auto-merge or `--admin`. Report `queued`, not `merged`, and
retain the branch. When the base has no queue (including before activation), keep
the normal protected squash path with `--match-head-commit` and confirm the merged
state before reporting it. Unknown queue capability is an error, not false.
The notification distinguishes publication completed from integration queued,
merged, or failed; it does not wait indefinitely for queue completion. This is one
inline admission path, not a new service. Verify the API schema and token permission
with read-only queries before implementing it, then prove admission in the pilot.

The queue path uses a short-lived installation token from the existing release
App, minted in an isolated protected `release` job that runs no checkout or SDK
tools, with only Pull requests write requested for admission. Admin must confirm
that installation permission or explicitly approve adding it; the current release
guide establishes Contents/Workflows write, not Pull requests write. Keep the token
in this job, revoke it on completion, and never pass it to candidate CI or SDK
builders. The workflow's existing `GITHUB_TOKEN` is not a substitute: GitHub's
anti-recursion rules suppress most workflows caused by that token. The pilot must
prove both queue checks and subsequent protected-main push workflows run after
release-requested admission. If this App permission is not approved, choose the
explicit manual handoff above rather than silently leaving a queued release stuck.

## Candidate identity, selection, and gate contract

Generate this addition to `ci.yml` from `dev/ci-suites`, without workflow-level path
filters:

```yaml
merge_group:
  types: [checks_requested]
```

Detection validates `github.event.merge_group.base_ref == refs/heads/main`, action
`checks_requested`, and an exact `head_sha` matching checkout `HEAD` and
`github.sha`. Checkout the event SHA, not a moving queue ref or PR head. Log base
ref/SHA, head ref/SHA, Git tree OID, workflow revision, and selected suite IDs in
the job summary. Do not parse PR numbers or trust level from
`refs/heads/gh-readonly-queue/main/...`: its format is not an identity contract.

The candidate contains the queue's base plus earlier entries and this entry;
it may contain several PRs. `HEAD^1`, the PR API files list, the current `main`
tip, or a diff against just the last PR can omit earlier changes. **No changed-file
result controls queue eligibility in this plan**: skip dorny and the PR metadata
request on `merge_group`; select the exact unconditional main set above and apply
`covered_by`. This also preserves full coverage for empty diffs and huge batches.
If diagnostic path reporting is later added, compare the payload's pinned
`base_sha` to `head_sha` after fetching both exact objects and verifying ancestry;
use NUL-delimited names with rename endpoints/deletions, never PR pagination.
Unavailable diagnostics cannot shrink coverage. Such diagnostics are not needed
for initial activation and are not a new implementation task.

Malformed event identity, wrong base, absent object, checkout mismatch, unknown
event type, or selector exception fails detection and both aggregate gates. There
is no success-with-empty-array fallback for an unrecognized event. On valid queue
events there is no optional path service to fail: the conservative full main set
is the normal path. PR detection failure still selects the existing conservative
full eligible set, not an empty list.

`Lint` and `Test` always report on PR and merge-group SHAs. Keep their `always()`
execution and existing gate script's non-success rejection for detection and every
selected blocking dependency, including `skipped`, `cancelled`, missing result,
invalid JSON, and failed product jobs. Unselected suites may be skipped; selected
ones may not. A covered suite may disappear only when its equal/stronger cover is
selected. Queue arrays contain only blocking main-tier suites; no advisory caller
can satisfy them. Keep manual subset/draft contexts distinct. Do not require every
matrix leg directly in the ruleset; that would create pending checks on PRs whose
selection legitimately excludes them. Do not use `paths-ignore` or commit-message
skip directives for the required workflow: missing required contexts stall admission.

Native stacks must enter bottom-up/contiguous, respecting GitHub's stack checks.
Current GitHub docs say stacks enqueue in dependency order and ejection removes
the dependent upper PRs; stacks can exceed configured merge limits. Therefore a
maximum merge limit of one is not proof of a one-PR candidate. Classic dependent
branches need their base merged/retargeted before queueing to main. No custom
dependency inference or queue-reordering bot is proposed. Never bypass a failed
lower layer to admit a higher one.

## Cache, credentials, runners, and provenance

The generated caller currently uses `secrets: inherit` for workspace/WASM and
some SDK suites. Replace inheritance with explicit secret maps for existing
non-queue consumers, and resolve every long-lived secret value to empty on queue
events. The selector's `PR_CACHE_TOKEN` flag must be false on queue even when the
repository has that secret, so both `products_*` jobs are unselected. For this
tokenless path each consumer builds/substitutes its inputs itself. In particular,
WASM partitions may independently rebuild the archive on a miss; do not pretend
that a local archive job's store persists across runners. Measure this duplication
before considering a separate same-run artifact design or CI-only writer policy.

Use `cachix-pr` for the current inherited WASM, Node, browser, SDK-platform, iOS,
Android, and Android-example suites, and add one scoped manifest secret mode
`cachix-pr-codecov` containing exactly `CACHIX_PR_AUTH_TOKEN` and `CODECOV_TOKEN`
for workspace tests. Declare these optional secrets in each affected
`workflow_call`. Existing `kache`, `cachix-pr`, and `none` modes keep their meanings;
generate event-conditioned values for every explicit map. No current CI suite
needs general secret inheritance after these changes.

| Resource/effect | Queue policy | PR/nightly and main policy |
| --- | --- | --- |
| Cachix substitution / Nix output reuse | `ci`, read public `xmtp` and `xmtp-pr`, verify normal Nix signatures; no push token | Existing CI writes remain CI-only; trusted jobs read only `xmtp` |
| Kache S3 | No long-lived reader/writer keys; private local cache only | Preserve existing read keys and protected-main writer scopes |
| pnpm store | `setup-js` mode `ci`: restore only | Save only protected-main trusted job; `read` jobs restore no CI pnpm cache |
| Gradle homes | `cache-read-only: true` for merge_group; never broaden protected-push guard | Retain main maintenance writer jobs and keys; schedules remain read-only |
| Workspace coverage | Generate local output/JUnit, skip Codecov upload explicitly on queue | Keep existing PR/schedule reporting and main reporting; no queue tokenless-upload guess |
| Run artifacts | Reports and same-run inputs only, scoped by run ID/attempt and SHA in summary | No cross-run download by artifact name alone, no trusted promotion |
| Deploy/registry/Pages/OIDC/release App tokens | No permissions, environments, calls, or credentials | Existing protected main/release entry points only |

An equal source tree does not imply equal commit SHA: squash commits and queue
commits differ. A derivation may depend on source files, toolchain/lockfile,
platform, feature/profile, and revision metadata. Reuse only when Nix's actual
derivation/store identity matches and its configured signature checks pass; never
rewrite a cache key or relabel a passing PR SHA as the queue SHA. Record whether
test outputs were substituted versus actually executed. Cacheable test results
remain the current approved policy, not a new promise of fresh execution.

`xmtp-pr` is explicitly untrusted: the cache guide documents that PR authors can
publish crafted outputs. Its signature establishes cache origin, not correctness.
This migration preserves CI's existing exposure rather than claiming merge queues
solve cache poisoning. Trusted main builds, deployments, releases, compiler writers,
and saved pnpm stores must never consume it or imported candidate artifacts.

Credential omission is defense in depth, **not a security boundary against edits
to workflow YAML**: a merge-group workflow can reference repository secrets itself.
Before activation, admin must verify all trust-bearing deployment/signing/cache
writer/App credentials are environment-scoped with branch policies excluding queue
refs, and remove any repository/organization duplicate exposed to candidate jobs.
The existing writer environment should admit only protected main; `release` must
admit only the existing authorized source branches; `web-chat`, backend deployment,
and Pages environments must retain their narrower policies. Inventory secret
names/locations and IAM permissions only, never values. Existing CI-only credentials
are not a promotion credential; no claim is made that omission prevents malicious
workflow edits from using a repository-level CI-only token. Any unavoidable trusted
repository secret blocks activation until its owning workflow is isolated.

Likewise, a workflow's `permissions: contents: read` is not an immutable cap on a
malicious replacement workflow. The admission operator must review changes to
workflows and their local actions/scripts before queueing, as part of authorizing
that exact PR head; this plan does not auto-admit unreviewed workflows or claim
that ordinary repository token defaults prevent YAML permission escalation.
If the organization needs arbitrary workflow edits to execute without that trust
decision, stop for a separate immutable-workflow/token-boundary design rather than
claiming this migration provides it. No new approval-count policy is assumed.

Fork origin is absent from merge-group's PR object. Never infer internal trust
from `github.repository`, actor, a queue ref, or maintainer admission. All queue
code gets the same credentialless caller. Current suites are fork-capable; a future
`disable_on_forks` exception must not silently skip a required candidate validator.
GitHub-hosted runners are ephemeral; Blacksmith/WarpBuild labels report
`self-hosted`, which is not evidence of isolation. Admin/operator must verify fresh
VMs, no ambient cloud/deploy identity, no writable shared trusted cache, and no
privileged network access for candidate jobs, including approved external PRs.
Existing PR approval and protected-environment controls remain; do not auto-approve
fork workflows. If a required platform cannot run untrusted code safely, block the
rollout rather than dropping that validator or handing it deployment secrets.

## Queue operations, ordering, and cost

Recommended initial settings: squash; build concurrency **1**; minimum/maximum
merge limits **1/1**; wait-to-minimum **0 minutes** if accepted (otherwise the UI
minimum, irrelevant when minimum is one); only merge non-failing PRs **enabled**;
status-check timeout **180 minutes**. The timeout accommodates the existing 120
minute product limit and sequential/matrix work but is provisional: measure actual
tokenless runs before activation and raise it only if a healthy measured critical
path needs it. Do not shorten existing job timeouts or add blanket retries.

Build concurrency controls simultaneous candidate builds, not matrix parallelism.
Merge limits control landing batches, not coalescing CI into one cheaper build.
Increasing either is not a demonstrated saving. After the pilot, concurrency 2 is
an optional measured latency tradeoff, not an implementation default. Preserve
existing matrix fail-fast behavior and Nix resource policy.

Use event-isolated concurrency: PRs keep `ci-pr-<number>` latest-push cancellation;
queue runs use `ci-merge-group-<head_sha>` with cancellation disabled; schedules,
dispatches, main CI, and post-merge retain separate groups. Different candidate
SHAs must never cancel one another, even if a temporary ref name is reused.
Retries of one SHA may share its group; GitHub may replace a pending duplicate,
but never the sole run for a different candidate. Invalidated/deleted groups may
leave orphan work until GitHub cancels it or its job timeout expires; no cleanup
bot is needed initially. Record those runner minutes. Main/post-merge are already
latest-pending-wins, not guaranteed execution for every intermediate push.

A failure, timeout, conflict, or protection failure ejects a PR/group. Classify it
before retrying: source failures need a reviewed fix; infrastructure/flaky failures
need evidence and an explicit retry/re-entry by the maintainer. A rerun of an old
SHA does not validate a recreated group. Confirm the active head SHA, use normal
queue re-entry when needed, and let GitHub rebuild downstream groups. Do not turn
off “only merge non-failing PRs,” quarantine tests, auto-requeue indefinitely, jump
the queue routinely, or use an admin bypass as a flake policy. An emergency bypass
requires Nick's specific authorization, incident record, and restoration of main
validation before that non-queue merge; it is not granted by plan approval.

Accepted landing triggers the existing independent protected-main writers and
deployers at `github.sha`; do not trigger them from `workflow_run` of candidate CI.
There is no new global deploy-after-all-caches barrier. Keep existing dependencies
in the inventory: docs waits on non-RN closure, backend deployment on its image
manifest, web-chat on its own trusted build. Nix paths and lockfile cache keys make
same-SHA warming repeatable; pins, latest tags, and deployments are mutable and
must retain serialization/current-main guards. A successful publish is not a
general license to rerun all release jobs. Keep Pages' `queue: max`, other existing
latest-pending policies, SHA tags, and release retry guidance; no new idempotency
service or guarantee of every-main-commit deployment is introduced.

Initial cost is additive: normal fast PR CI + candidate full main-tier CI + full
main CI + existing warming/nightly. Later cost removes duplicate pure main suites,
but keeps Clippy/rustdoc/Android/workspace maintenance and publication integrity
checks. Queue invalidations, rejected candidates, local-only Kache, and no product
cache publishing can add substantial time. LIBX-6's existing audit covered 500
runs/11,131 jobs over October 27–30, not a full 28-day baseline; only 23 cohorts had
baseline n >= 10. The observed ~14 Warp runner-minutes per full iOS invocation is
an order-of-magnitude cost warning, not proof of savings. #4840's post-change
savings and nightly behavior were unmeasured. Do not quote dollars without rates
or sum heterogeneous runner minutes as if they were equally priced.

## Requirements

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| P1 | Exact candidate validation | On a main `merge_group.checks_requested` event, CI MUST validate the checked-out event head SHA using every current main-tier suite. | PR results alone omit intervening changes. |
| P2 | Reject invalid identity | When event type, base ref, or checkout identity is invalid, detection MUST fail. | Silent empty selection could admit untested code. |
| P3 | Stable fail-closed gates | On PR and queue events, `Lint` and `Test` MUST reject any non-success selected blocking dependency or detection result. | Skipped jobs otherwise appear successful to protection. |
| P4 | Preserve selection policy | On PR, schedule, and dispatch events, CI MUST retain #4840's filters, tiers, covers, partition policy, and gate naming. | Queue adoption must not undo approved coverage/cost choices. |
| P5 | No queue write authority | On queue events, generated callers MUST pass no long-lived secrets or writer/deploy environments and MUST grant no publish permissions. | Admission is not authorization to publish. |
| P6 | Preserve cache trust | When a job reads CI-only outputs, it MUST NOT write a trusted cache or supply a deployment/release artifact. | Tree equality is not provenance. |
| P7 | Keep post-merge effects | After landing on protected main, workflows MUST retain the listed warming, coverage, publication, and closure/deploy ordering. | Removing CI's push trigger wholesale loses existing writers. |
| P8 | Preserve candidate isolation | When two queue runs have different head SHAs, their concurrency groups MUST differ. | Latest-wins main grouping would cancel needed queue evidence. |
| P9 | Preserve evidence identity | For every queue run, CI MUST record head/base SHA, tree OID, selected suites, workflow revision, and run identity. | Failures and cache reuse cannot be attributed by PR number alone. |
| P10 | Verify activation controls | Before requiring the queue, the admin MUST verify effective rules, credential placement, environment policies, and runner isolation using the activation checklist. | YAML conditions alone cannot exclude candidate access to repository secrets. |
| P11 | Gate main deduplication | Before main CI is reduced, the owner MUST verify all pilot acceptance criteria below. | Premature removal erases the fallback while the queue is unproven. |
| P12 | Safe release admission | When a release requests main integration, its workflow MUST distinguish queue admission from a completed merge and reject a changed PR head. | Existing success messages may imply a merge that has not happened. |
| P13 | Attribute queue operations | Once candidates run, the monitoring owners MUST classify queue failures and costs separately from PR, main maintenance, nightly, and publication runs. | Main-only intake would miss failures moved before landing. |
| P14 | Recover without bypass | When queue infrastructure prevents progress, operators MUST restore full main validation before disabling the queue requirement. | Rollback must not leave neither pre- nor post-merge coverage. |

## Spec changes

None. No spec requirement is added, amended, removed, or implemented by this
tooling migration. Plan IDs stay in this plan, not code backlinks. Ref is not
required for this plan under the repository's CI/tooling exception.

## Proofs and acceptance criteria

Tests below are proposed, not claimed to have run. Use existing Python unittest
fixtures and gate-shell execution helpers, not YAML string snapshots alone. Every
new regression test must fail against the old/weakened behavior and then pass;
retain all independent existing assertions rather than deleting them for the new
event. Regenerate expected generated files rather than hand-editing `ci.yml`.

| Proof | Requirements | Inputs, expected result, and regression caught |
| --- | --- | --- |
| V1: `test_merge_group_selects_main_policy` in `dev/tests/test_ci_select.py` | P1, P4 | Valid queue payload with no PR metadata: exact current main suite set, all blocking, four WASM partitions, full iOS/API23, no examples/RN devices/docs-site. Compare against an explicit policy expectation plus existing main fixture, so a shared eligibility bug cannot make both pass. Old selector misclassifies this event as PR. |
| V2: `test_merge_group_identity_fails_closed` plus CLI fixture | P2, P9 | Temporary Git repo with base, two queued changes, and later moving main; matching SHA records candidate tree, not new main. Wrong base, missing head, mismatched checkout, unknown event fail nonzero. Empty/many changed paths do not affect queue suite selection. Existing tests exercise PR metadata, not queue identity. |
| V3: extend executable gate tests in `test_ci_select.py` | P3 | Run generated gate shell with detection failed/cancelled, selected job skipped/absent/failed/cancelled, failed product, invalid selection JSON: nonzero. Successful selected jobs with unrelated skipped/advisory failure: zero. Execute both PR and queue selections; deliberately break a `needs`/always condition to prove wiring check fails. |
| V4: extend `test_ci_suites.py` and `test_ci_budget.py` | P1, P3, P4, P8 | Parse generated workflow: queue trigger type present, no top-level path filter; main tier still unconditional; draft/subset names distinct; existing PR label/deletion/full/neutral tests unchanged; different queue SHAs/PR/schedule/main keys cannot collide. Test event expressions with the existing expression harness where available, not substrings only. |
| V5: extend `dev/tests/test_kache_env.py` | P5, P6 | Expand all local workflow/action callers under a queue event with all secret names populated by dummy values: zero queue writer environments/tokens/permissions, false product token flag, both setup modes `ci`, Kache local, Gradle read-only, Codecov step excluded. Inject writer mode/token or secret inheritance and observe rejection. Existing event matrix lacks merge_group. |
| V6: retain/extend `dev/tests/test_warm_sets.py`, `test_ci_budget.py`, `test_release_push.py` | P6, P7, P11 | Cutover push selects exactly four maintenance suites; Kache/Gradle guards require protected main, same workflow/job identity; RN isolation and closure/deploy dependencies survive. Reject writer on queue/tag/unprotected push. Existing warm/pin assertions keep post-merge behavior. |
| V7: mocked CLI release test in `dev/tests/test_release_push.py` | P12 | Queue capability selects SHA-guarded enqueue, no branch deletion/admin/implicit auto-merge; changed head/unmet checks reject; dry-run/no-merge mints no token and does not admit; a base without queue preserves its merge path. Fake CLI records GraphQL arguments/results. Token wiring test requires the isolated App job, not GITHUB_TOKEN; manual pilot proves subsequent event delivery. |
| V8: extend `dev/tests/test_ci_metrics.py` | P9, P13 | Queue run is not a main push or a single PR; recreated candidate has distinct SHA/attempt/cost; missing historical payload is unknown, not a fabricated PR association. Cache substitution is reported separately from fresh execution. |
| V9: manual admin rehearsal and pilot | P5–P14 | Checklist below, including environment rejection and two-PR invalidation. Static fixtures cannot prove GitHub dispatch, protection enforcement, runner provisioning, or token scope. |

Local implementation verification, on an approved full build machine through Nix:

```sh
dev/nix-shell 'python3.11 dev/ci-suites generate'
dev/nix-shell 'python3.11 dev/ci-suites check'
dev/nix-shell 'python3.11 -m unittest discover -s dev/tests -p "test_ci*.py"'
dev/nix-shell 'just lint-config'
dev/nix-shell 'just test dev'
dev/nix-shell 'just docs check'
dev/nix-shell 'just docs check-external'
```

The full dev suite covers cache/warm/release fixtures beyond the focused CI tests.
Run docs checks with current site artifacts per `apps/docs/VALIDATION.md`; a source
check is not a full-site pass. No builds or workflow dispatches are part of this
planning task.

### Admin activation, pilot, and rollback

1. **Before activation (Nick/admin):** export the current effective main rules,
   status-check app bindings, bypass actors, merge method, and relevant environment
   policies. Verify public-org queue availability in Settings → Rules → Rulesets →
   `main`; confirm no conflicting classic rule. Do not change release rules or
   review requirements. Confirm all privileged secrets are environment-only and
   queue refs cannot enter those environments; inspect runner isolation as above.
2. **Land compatibility first:** merge the approved implementation PRs only after
   their PR checks pass. Keep full main CI and both required context names. An
   unused `merge_group` trigger is safe to land before the queue rule. Static
   fixture/manual policy tests are the pre-activation shadow; `workflow_dispatch`
   cannot reproduce GitHub's merge-group event or credentials, so do not label it
   an end-to-end queue proof. No queue event is expected until real admission.
3. **Controlled activation (separate explicit authorization):** add Require merge
   queue to the existing main ruleset with the settings above. Keep required
   `Lint`/`Test` bound to GitHub Actions and current squash/review/force-push rules.
   Do not add individual conditional checks, deployments, a new bypass actor, or
   strict-update requirements. Verify actual run/check names once on a real queued
   approved PR; a missing context blocks, never silently waive it.
4. **Manual negative rehearsal:** with approved disposable/test PRs or an approved
   isolated test repository, demonstrate a failing selected suite blocks/ejects,
   an unknown/missing trigger cannot admit, base movement invalidates evidence,
   and two entries produce a combined candidate/recreated SHA after removing the
   first. Verify fork-origin code receives no trusted environment/credentials,
   no queue deploy/publish run starts, and same-run artifacts belong to the event
   SHA. Do not intentionally land a failing PR on main. The operator must approve
   test PR admission; this plan does not authorize it.
5. **Pilot acceptance:** retain full main CI for at least **20 accepted candidates
   and seven calendar days**, whichever takes longer. Include at least one
   Rust/backend, one SDK/platform, and one prose-only candidate. Require zero
   unexplained missing selected checks, no candidate writes to trusted stores,
   no unintended examples/RN expansion, verified main deployment/warm behavior,
   and an accounted outcome for every queue failure/timeout. Compare suite policy
   and source trees rather than identical check counts (tokenless products differ).
   Record queue wait, critical path p50/p95, timeouts, invalidation/retry counts,
   runner-class minutes per accepted merge and rejected candidate, and cache reuse.
   Nick approves cutover only after examining the results; 20 successes alone do
   not prove cost savings or absence of flakiness.
6. **Cutover:** land the separate main-maintenance selection PR, changing only push
   selection/gate labels as planned. Keep schedule, PR, merge-group, publication,
   and manual release-candidate coverage. Verify the next accepted merge starts
   four maintenance suites plus existing publication/warm workflows. Do not
   disable push CI wholesale.
7. **Rollback:** before disabling the merge-queue rule, restore the full main
   selection via a reviewed rollback PR while the queue still works. If the queue
   cannot run that PR, Nick/admin must explicitly authorize that one emergency
   rule change/merge; immediately restore full main validation and inspect its
   result before normal merges resume. Preserve `Lint`/`Test` requirements and all
   trusted guards throughout. Pause admissions while changing controls, record
   stranded entries and re-entry instructions, and never use rollback to grant
   queue code trusted-cache or deployment access.

## Tasks

### Task 1: Add candidate semantics and preserve gates

Implement explicit queue event handling without changing other event policies.

- Requirements: P1–P4, P8–P9.
- Files: `dev/ci-select`, `dev/ci-suites`, `.github/ci-suites.yml`, generated
  `.github/ci-suites.json`, generated `.github/workflows/ci.yml`,
  `dev/tests/test_ci_select.py`, `dev/tests/test_ci_suites.py`,
  `dev/tests/test_ci_budget.py`, `.github/CI.md`.
- Decisions: unconditional current main suite set, exact event SHA, unchanged
  gate names, no batched path/PR-label lookup, SHA-specific queue concurrency.
- Consumes: GitHub event JSON `merge_group.{base_ref,base_sha,head_ref,head_sha}`
  (strings), event action and checkout SHA. Produces: existing `lint_suites`,
  `test_suites`, `advisory_suites`, `products` JSON arrays of suite/group strings,
  plus the candidate identity job summary; no new SDK interface.
- Verification: V1–V4; generator check, focused unittest command, lint/dev suite.
- Depends on: none; ships atomically with Task 2 before any queue activation.
- Risk: **frontier** (event identity and cancellation).

### Task 2: Make candidate execution credentialless

Remove secret inheritance, preserve tokenless product fallbacks, and explicitly
exclude queue external publication without changing PR/nightly cache policy.

- Requirements: P5–P7.
- Files: `dev/ci-suites` secret maps; `.github/ci-suites.yml`/generated files;
  `.github/workflows/test-workspace.yml` for the Codecov condition;
  `.github/actions/setup-nix/README.md`; `dev/tests/test_kache_env.py`.
  Add optional reusable-secret declarations in `test-workspace.yml`,
  `test-wasm.yml`, `test-node-sdk.yml`, `test-browser-sdk.yml`,
  `test-sdk-platform.yml`, `test-ios.yml`, `test-android.yml`, and
  `test-example-android.yml`. Audit `check-rust.yml` and
  `docs-rust-reference.yml` protected-writer conditions. No new cache service
  or cache mode; the new manifest secret-map mode is not a cache policy.
- Decisions: empty queue long-lived secret mappings, `PR_CACHE_TOKEN=false`,
  no prebuild products, local Kache, Gradle restore only, reports retained.
- Consumes: event identity from Task 1. Produces: generated explicit secret
  allowlists with queue values empty and existing non-queue values preserved.
- Verification: V5–V6, expanded caller-expression matrix and dev tests. Admin
  environment/runner checks in Task 5 are a separate required proof, not simulated.
- Depends on: Task 1; same PR because an event-only intermediate is unsafe.
- Risk: **frontier** (credentials/cache trust).

### Task 3: Update release integration and operational evidence

Make admission semantics and monitoring correct before main validation moves.

- Requirements: P9, P12–P13.
- Files: `.github/workflows/release.yml`, `.github/RELEASING.md`,
  `docs/create-a-release.md`, `dev/tests/test_release_push.py`, `dev/ci/metrics`,
  `dev/tests/test_ci_metrics.py`, `.github/CI.md`.
- Decisions: no bypass/auto-merge, pinned release PR head, report queued versus
  merged; missing historical candidate membership remains unknown.
- Consumes: Task 1 candidate identity. Produces: event-aware metrics and release
  summary wording with PR URL/head and admission outcome, not a false merge SHA.
- Verification: V7–V8; mock CLI and metrics fixtures; lint/dev suite; admin
  validates actual CLI admission behavior during the authorized pilot.
- Depends on: Tasks 1–2.
- Risk: **frontier** (merge authority); metrics-only edits are standard.

### Task 4: Prepare main-maintenance cutover

Preserve writer consumers while eliminating the redundant pure validation pass.

- Requirements: P4, P7, P11, P14.
- Files: `dev/ci-select`, `dev/ci-suites`, `.github/ci-suites.yml`, generated
  `.github/ci-suites.json`/`.github/workflows/ci.yml`,
  `dev/tests/test_ci_budget.py`, `dev/tests/test_ci_select.py`,
  `dev/tests/test_ci_suites.py`, `dev/tests/test_kache_env.py`,
  `.github/CI.md`, `.github/actions/setup-nix/README.md`.
- Decisions: add an explicit `post_merge` suite eligibility for only
  `check_rust`, `docs_rust`, `test_android`, `test_workspace`; push maps to it,
  queue stays `merge`. Draft/manual subset logic remains unchanged. Keep full
  scheduled/manual semantics, main workflow/job cache identity, maintenance gate
  suffixes, and protected-main writer expressions. Do not extract Gradle cache
  warming into a new workflow/key or remove tests from its warming command.
- Consumes: accepted pilot record. Produces: push-only maintenance policy and
  documented one-PR revert restoring full main validation.
- Verification: V4/V6 and generation/lint/dev checks; manual next-main observation.
- Depends on: Task 5 acceptance; prepare if desired, **do not land** before sign-off.
- Risk: **frontier** (coverage loss during transition).

### Task 5: Activate, measure, and hand off operations

Nick/admin activates the queue only after compatibility and trust proofs; standing
owners continue their existing read-only roles with event-aware intake.

- Requirements: P10–P14.
- Areas: GitHub main ruleset/settings, environment/runner verification, authorized
  pilot; Captain-owned operational records, not product code.
- Decisions: activation checklist and defaults above; no new polling service,
  credential provisioning, workflow dispatch, or settings mutation by this plan.
- Consumes: merged Tasks 1–3 and admin verification. Produces: settings snapshot,
  pilot acceptance record/cost cohorts, rollback record, then cutover authorization.
- LIBX-5 follow-up: add completed `merge_group` CI failures/timeouts to intake in
  addition to main failures; main-only `head_branches: main` will miss queue refs.
  Filter by event and queue base, not just a brittle ref prefix. Deduplicate by
  workflow run/attempt/candidate SHA and root cause; candidate ejection is not a
  main regression. Keep nightly/deploy failures identifiable and its role read-only.
- LIBX-6 follow-up: separate pre/post-#4840, shadow queue, cutover queue, maintenance,
  nightly, runner and cache-mode cohorts; keep historic PR/main evidence and add
  queue-to-landed-tree attribution. Track invalidated/orphan work and cost per
  accepted merge. A failure prevented by the queue is not an escaped-main failure;
  do not change thresholds or infer invoice savings.
- LIBX-7 follow-up: distinguish ready for review, eligible for queue, queued,
  ejected, and actually merged. Preserve exact-head/push-actor and specific user
  authorization checks; recheck all dependent stack entries. Enqueue is a future
  merge authorization, not a harmless read. Keep this owner read-only.
- Captain updates the corresponding automation event filters/skills only after
  approval; existing main alerts remain. Verify one real queue failure delivery
  during the authorized rehearsal without dispatching duplicate repair owners.
- Verification: V9, live check-run SHA/context/app reads and pilot measurements;
  no claim of activation from static tests alone.
- Depends on: Tasks 1–3; gates Task 4.

## Delivery

| PR / step | Tasks | Base | Can pass alone / ownership |
| --- | --- | --- | --- |
| Plan PR (this document only) | None implemented | `main` | Docs-only proposal; Nick approval required |
| Compatibility PR | 1–2 | `origin/main` | Must ship together; ordinary PR/main checks remain; CI implementer |
| Operations PR | 3 | `main` after compatibility merges | Can pass with existing main behavior; same implementer |
| Admin activation/pilot | 5 | Not a PR | Nick/admin, Captain and existing monitoring owners |
| Cutover PR | 4 | `main` after pilot sign-off | Tests must support both policies; same implementer; retain revert path |

These are sequential PRs, not a concurrent stack: no stack tool is needed when
each parent lands before the next branches from `origin/main`. If execution
chooses to prepare a dependent stack, use `gh stack` under its skill and do not
submit/merge the cutover layer before pilot authorization. One implementation
lane owns shared selector/generator/CI files. Admin/monitoring preparation can
proceed alongside code because it reads evidence and prepares its own records;
activation and automation edits depend on the finalized event contract.

Models: frontier implementer `openai/gpt-6-astra`, high; standard work
`openai/gpt-6.1-sol`, high; independent plan reviewer `openai/gpt-6.1-sol`, max
(Codex frontier reviewer tier); chores `openai/gpt-6-luna`, high. No execution
orchestration is invoked by this plan.

## Research sources and verification limits

Official GitHub sources read on 2026-10-30:

- [Managing a merge queue](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue): coupled PR/queue contexts, temporary candidates, settings, invalidation, failures and queue jumping.
- [Events that trigger workflows](https://docs.github.com/en/actions/reference/events-that-trigger-workflows#merge_group): `checks_requested`, candidate SHA/ref, separate event trigger.
- [Deploying code](https://docs.github.com/en/pull-requests/concepts/deploying-code): public-organization/private-Enterprise availability.
- [Available rules for rulesets](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets#require-merge-queue): repository queue rule and settings.
- [Merging stacked pull requests](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/merging-stacked-pull-requests): stack admission/ejection order and size exceptions.
- [GitHub CLI merge behavior](https://cli.github.com/manual/gh_pr_merge) and [GraphQL pull requests](https://docs.github.com/en/graphql/reference/pulls): implicit auto-merge behavior, queue capability and SHA-guarded admission.
- [Triggering a workflow from a workflow](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow): GITHUB_TOKEN anti-recursion and App-token alternative.

Repository evidence: manifest/selector/generator and all workflow job graphs at
the baseline above; setup cache policy, RN exchange, release source isolation,
existing CI/cache/warm tests, and Captain LIBX-3/LIBX-6 reports. Recheck against
main before execution; later changes are not implicitly incorporated.

Not verified by planning: a live merge-group event, fork queue run, actual required
check enforcement, environment/secret/IAM configuration, runner isolation, full
site validation artifacts, queue latency/cost, post-#4840 savings, or deployment
behavior under queue landing. No workflows/settings/runtime code were changed,
no CI was dispatched, and no PR was enqueued or merged to produce this plan.
