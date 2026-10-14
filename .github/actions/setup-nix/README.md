# Setup Nix

## Nix builds

`setup-nix` sets `max-jobs = auto` and `cores = 0` on Linux and macOS: Nix
builds one derivation per CPU at once, and each build may use all cores. Nix
itself defaults to `max-jobs = 1`. Do not add a cap without evidence. A job
gets a lower `max-jobs`, for that job alone, only when a failed run shows a
kernel out-of-memory record, or when a measurement shows the job slower with
`auto`. On Linux, a post step prints the kernel records that match
`Out of memory: Killed process` or `oom-kill` when the job ends, also after a
failure, and adds a warning when it finds one. On macOS, check memory pressure.

The Nix store is not persisted between jobs. Each job substitutes from Cachix
and cache.nixos.org, and Nix checks the signature of every path it adds.

## Cache modes

Two Cachix caches hold CI outputs:

- `xmtp` is trusted. Release, deploys, publish jobs, the post-merge workflow, and
  developers substitute from it. Only jobs in mode `trusted` write it.
- `xmtp-pr` (public key
  `xmtp-pr.cachix.org-1:QV97uslBSzm8N6XoUKzyifkel14qZVsq1kzZVkgJ70g=`) is
  CI-only. Pull request jobs and the other merge-run jobs write it, so its
  content can come from unreviewed code.

**The rule:** a job that reads a CI-only store writes no trusted store
(Cachix `xmtp`, Kache S3, the saved pnpm store). Each caller states its mode
with the `cache-mode` input:

| Mode | Reads | Writes | Token input | Kache | Used by |
| --- | --- | --- | --- | --- | --- |
| `read` (default) | `xmtp` | nothing | none | read-only | release, publish, and deploy jobs, the post-merge closure and docs jobs, schedules, manual tools |
| `ci` | `xmtp`, `xmtp-pr` | `xmtp-pr` | `cachix-pr-auth-token` (`CACHIX_PR_AUTH_TOKEN`) | read-only | pull request and merge-run jobs |
| `trusted` | `xmtp` | `xmtp`, Kache S3 | `cachix-auth-token` (`CACHIX_AUTH_TOKEN` of the writer environment) | writes when `kache-readonly: "false"` | the post-merge build, and writer jobs on a protected push to trunk |

`dev/kache-ci-policy` runs before any cache is configured and fails the job
when:

- the mode is not `read`, `ci`, or `trusted`;
- the mode is `trusted` and the event is not a protected push to `main` or
  `self-hosted`;
- a `read` or `trusted` job receives the `xmtp-pr` token;
- a `read` job receives the trusted token.

A `ci` job ignores `cachix-auth-token`. Fork and Dependabot runs have no
`xmtp-pr` token: they read both caches and push nothing.

A writer job selects the writer environment `kache-s3-writer` exactly when
its mode is `trusted`, with the same protected-push expression in both
places:

```yaml
environment:
  name: ${{ <protected push> && 'kache-s3-writer' || '' }}
...
cache-mode: ${{ <protected push> && 'trusted' || 'ci' }}
cachix-auth-token: ${{ <protected push> && secrets.CACHIX_AUTH_TOKEN || '' }}
cachix-pr-auth-token: ${{ !(<protected push>) && secrets.CACHIX_PR_AUTH_TOKEN || '' }}
```

Only a job that resolves to `trusted` receives the trusted token. The writer
environment's branch policy (`main`, `self-hosted`) also keeps it from pull
request jobs once the repository-level `CACHIX_AUTH_TOKEN` is deleted.
`setup-js` takes the same `cache-mode`, and every job passes the same mode to
both actions: `read` restores no pnpm store (pnpm then downloads each tarball
and checks it against the lockfile), `ci` restores only, and `trusted` also
saves `pnpm-*` entries. `dev/agents/test_kache_env.py` evaluates each
caller's mode, environment, and tokens for pull request, trunk push,
schedule, and dispatch events, and checks these rules.

Merge-run jobs in mode `ci` reuse what the pull request's last run pushed to
`xmtp-pr` when the merge tree equals the tree that run tested, and they hand
off inside the run through `xmtp-pr`.

### Repair a suspected poisoned `xmtp-pr`

A pull request author can upload a crafted output for any path that
`xmtp-pr` does not hold yet. Release, deploys, and the post-merge workflow never
read `xmtp-pr`, so shipped bytes stay safe; CI jobs in mode `ci` substitute
the crafted output until it leaves the cache. Nobody knows which path is
crafted, so deleting one entry is not enough, and `xmtp-pr` holds only
results that CI can build again. An admin clears the whole cache with a
personal token, then reruns the affected merge run:

```bash
curl -fsS -X POST -H "Authorization: Bearer $CACHIX_PERSONAL_TOKEN" \
  https://app.cachix.org/api/v1/cache/xmtp-pr/clear
```

What a clear keeps (name, key, tokens, settings) is not verified; ask Cachix
support first (Task 1 step 10 of the Nix build plan asks the same for `xmtp`).
The Gradle cache is not part of `setup-nix`. Android CI jobs in mode `ci` add
`gradle/actions/setup-gradle` after it: protected trunk pushes write that
CI-only cache, and every other run only reads it. Release, deploy, benchmark,
and trusted jobs do not restore it (`CacheModeWiringTests`).

## CI compiler cache

`setup-nix` uses pinned Kache 1.0.0 and its private runtime directory.
GitHub cache archives are disabled for the compiler store. Nix/Crane, Cachix,
pnpm, Windows rust-cache, Gradle, and Docker keep their existing caches.

Without either access key field, Kache uses a private local store. A partial
pair is an error. Complete credentials require a bucket and build scope before
the action starts its S3 daemon. Both modes keep the 10 GiB local limit and
the current compiler wrapper and flags. The local limit does not cap S3.
The job summary reports the backend and whether remote writes are enabled.

## S3 coverage

Kache is off by default (`kache: "false"`). Only jobs that run cargo outside
Nix set `kache: "true"`; Nix derivations cannot use Kache, so a job that only
runs `nix build` gains nothing from it. The 21 enabled callers in the table
below use S3 when the reader key pair is available. Every other caller passes
no Kache input and selects no writer environment, except the `post-merge.yml` build
job: it runs only on pushes to trunk and selects the writer environment for
the trusted Cachix token. Kache writes also need mode `trusted` (see "Cache
modes"). The 11 stable scope families follow the outer Cargo
build variant. The action adds runner OS and
architecture. All callers use `libxmtp/trusted`, prefetch expensive outputs,
and do not pull the whole bucket at startup.

The resources are managed by
[infrastructure PR #819](https://github.com/xmtplabs/infrastructure/pull/819)
in `plans/dev`, region `us-east-2`. After its Terraform Cloud apply, retrieve
`ci/libxmtp/kache/reader` and `ci/libxmtp/kache/writer` from AWS Secrets Manager.
Each contains `access_key_id` and `secret_access_key`.

Set these repository variables:

| Variable | Value |
| --- | --- |
| `KACHE_S3_BUCKET` | Bucket from the `libxmtp_kache` Terraform output |
| `KACHE_S3_REGION` | `us-east-2` |
| `KACHE_S3_ENDPOINT` | Optional. Leave unset for AWS |

The workflows default an unset endpoint to an empty action input. Kache then
uses the AWS endpoint for the bucket's region. Set this variable only for a
custom S3-compatible endpoint.

Use the reader pair as repository secrets:

- `KACHE_S3_ACCESS_KEY_ID`
- `KACHE_S3_SECRET_ACCESS_KEY`

Create environment `kache-s3-writer`. Set deployment branches and tags to
**Selected branches and tags** and add only the exact branches `main` and
`self-hosted`, with no tag or wildcard rule. Each writer branch must also be
protected. Configure those rules before adding writer keys. Use the writer
pair as environment secrets with the same two names. GitHub substitutes them
for the repository reader secrets in jobs that select the writer environment.
Set both fields of each pair together. Do not put writer keys at repository
or organization scope.

Audited compiler jobs select this environment only on a protected branch push
to `main` or `self-hosted`. Other events use an empty environment name and
select no GitHub environment. Same-repository pull request runs use repository
reader secrets. Fork and Dependabot runs have no reader secrets and build
local-only. No reader environment is needed.

`kache-readonly` defaults to `true`. An audited writer caller sets it to
`false`; the write helper still requires S3, mode `trusted`, the push event,
the exact branch, and branch protection. The Kache action receives `save-cache: false` for all
reader jobs. It writes read-only mode to its daemon config before startup.
Provider IAM permissions are the access boundary; the save flag is a second
check.

Existing deployment jobs keep their deployment environment and URL. The
`web-chat` environment must contain no writer keys under the two Kache secret
names. Leave those environment secrets absent to use repository reader keys.
Release jobs, jobs that build a selected ref, manual recovery, native
backend acceptance, and the docs site job use `kache-readonly: true` and never
select the writer environment. Source lint, docs quality and composition, Nix output warming,
and the iOS and Android test jobs leave Kache off and select no writer
environment, because they run no cargo outside Nix.
Native backend acceptance receives only the Kache pair through an explicit
secret map. Its Cachix and attachment S3 settings stay unchanged.

## Acceptance and recovery

Once secrets are available, use a protected branch push to publish eligible
library outputs. A manual run cannot seed Kache 1.0's remote. In a fresh reader
job at the same source, check the effective S3 remote and restored outputs.
Repeat on Linux and macOS. A changed source must compile. A missing or
unavailable entry must also compile normally. Prove that reader credentials
cannot upload and pull request jobs cannot obtain the writer keys.

The native Clippy and Darwin SDK pilots have verified S3 reader reuse. New
writer scopes are seeded by the first protected branch push after this rollout
merges. PR runs before that push can read matching objects from the shared
prefix, but their new scope manifests can be empty. Use the pinned action
summary to check effective read-only mode, cache hits, transfer bytes, and
errors. Reader runs must report no uploads. The two pilot workflows retain
redacted JSON reports when the reader-evidence change is present.

Do not use commit IDs, run IDs, branch names, or shard numbers in a scope.
The [pinned action guidance](https://github.com/kunobi-ninja/kache-action/blob/1a33fb2ff51be23eb9e87abeae6edb65be78f71c/README.md#manifest-vs-shards)
separates build variants to keep their prefetch lists useful.
Scopes follow outer Cargo build families. JavaScript SDK product builds share
`sdk-products-debug`. The browser platform proof, which also runs the bridge
tests, uses `sdk-browser-conformance` because it builds debug panic-test and
release pure-codec fixtures with conformance features. The older SDK facade job uses
`sdk-products-tests-conformance` because it combines product builds, Rust unit
tests, and those browser fixtures. Node-only SDK builds share `sdk-native-debug`. Android staging and native Node release
builds use separate release scopes. Default SDK tests, backend tests, native
Clippy, WASM Clippy, Cargo check, and Rust docs retain distinct scopes.

A scope selects prefetch manifests and dependency shard indexes. It does not
partition compiled artifacts: all callers share matching artifacts under
`libxmtp/trusted`. Reader-only SDK release and deployment jobs use the same
debug build scopes as their writer jobs. An npm release still uses a debug
Cargo build when its SDK generation command does not request `--profile release`.

Jobs with no outer Cargo compilation leave Kache off. This includes the
Unix Node platform matrix, the iOS and Android test jobs, and the lint jobs
that build through Nix. They keep Nix/Crane and Cachix caching. Node and
browser test shards share the build scope without adding shard numbers.

To stop S3 use, remove both credential fields at both GitHub scopes. The jobs
then use local-only Kache. Removing only the bucket while keeping credentials
is a configuration error. For key rotation, follow the two-apply process in
the infrastructure guide, then update the GitHub pair before deleting the old
key.

Check changes with `dev/nix-shell 'just lint-config'` and
`dev/nix-shell 'just agent-test'`. The fixtures use fake credentials and never
start Kache or access S3.

## Caller scopes

The action appends runner OS and architecture to each scope below.
"Protected push writer" requires all event and branch checks above. Every
other event is a reader. `dev/agents/test_kache_env.py` checks that this table
and its `KACHE_CALLERS` list name exactly the callers with `kache: "true"`.
A job that runs cargo outside Nix belongs in both; remove it only when it
stops running cargo.

| Workflow / job | Scope | Access |
| --- | --- | --- |
| `build-sdk-node-platforms.yml / assemble` | `sdk-native-release` | Reader |
| `check-rust.yml / native` | `rust-clippy-native` | Protected push writer; otherwise reader |
| `check-rust.yml / wasm` | `rust-clippy-wasm` | Protected push writer; otherwise reader |
| `check-sdk-products.yml / generated` | `sdk-products-debug` | Protected push writer; otherwise reader |
| `check-sdk-unit.yml / unit` | `sdk-tests-debug` | Protected push writer; otherwise reader |
| `check-types.yml / check` | `sdk-products-debug` | Protected push writer; otherwise reader |
| `deploy-docs.yml / site` | `sdk-products-debug` | Reader |
| `deploy-web-chat.yml / deploy` | `sdk-products-debug` | Reader |
| `docs-rust-reference.yml / rust` | `rust-reference` | Protected push writer; otherwise reader |
| `manual-sdk-recovery.yml / recovery` | `sdk-native-debug` | Reader |
| `release-agent-sdk.yml / build` | `sdk-native-debug` | Reader |
| `release-browser-sdk.yml / build` | `sdk-products-debug` | Reader |
| `release-cli.yml / build` | `sdk-native-debug` | Reader |
| `test-agent-sdk.yml / test` | `sdk-native-debug` | Protected push writer; otherwise reader |
| `test-android.yml / android-stage` | `sdk-android-release` | Protected push writer; otherwise reader |
| `test-backend.yml / native` | `backend-tests-debug` | Reader |
| `test-browser-sdk.yml / test` | `sdk-products-debug` | Protected push writer; otherwise reader |
| `test-node-sdk.yml / test` | `sdk-native-debug` | Protected push writer; otherwise reader |
| `test-sdk-platform.yml / proof` | `sdk-browser-conformance` | Protected push writer; otherwise reader |
| `test-sdk.yml / sdk` | `sdk-products-tests-conformance` | Protected push writer; otherwise reader |
| `test-sdk.yml / android-stage` | `sdk-android-release` | Protected push writer; otherwise reader |
