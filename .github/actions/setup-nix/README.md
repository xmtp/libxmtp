# CI compiler cache

`setup-nix` uses pinned Kache 1.0.0 and its private runtime directory.
GitHub cache archives are disabled for the compiler store. Nix/Crane, Cachix,
pnpm, Windows rust-cache, Gradle, and Docker keep their existing caches.

Without either access key field, Kache uses a private local store. A partial
pair is an error. Complete credentials require a bucket and build scope before
the action starts its S3 daemon. Both modes keep the 10 GiB local limit and
the current compiler wrapper and flags. The local limit does not cap S3.
The job summary reports the backend and whether remote writes are enabled.

## S3 pilot

The first callers are native Clippy in `check-rust.yml` and the Darwin check in
`test-bindings-check.yml`. Their stable scopes include the build variant,
runner OS and architecture. They use `libxmtp/trusted`, prefetch expensive
outputs, and do not pull the whole bucket at startup.

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

The pilot jobs select this environment only on a protected branch push to
`main` or `self-hosted`. Other events select `kache-s3-reader`, which needs no
secrets, approval, or branch restriction. GitHub can create that empty reader
environment when the jobs first reference it. Pull request runs get only
repository reader secrets; fork and Dependabot runs can build local-only.

`kache-readonly` defaults to `true`. An audited writer caller sets it to
`false`; the write helper still requires S3, the push event, the exact branch,
and branch protection. The Kache action receives `save-cache: false` for all
reader jobs. It writes read-only mode to its daemon config before startup.
Provider IAM permissions are the access boundary; the save flag is a second
check.

Existing deployment jobs keep their deployment environment. Release jobs that
build a selected ref and native backend acceptance remain readers when added
to S3 later. Only these two pilot callers receive credentials in this PR.

## Acceptance and recovery

Once secrets are available, use a protected branch push to publish eligible
library outputs. A manual run cannot seed Kache 1.0's remote. In a fresh reader
job at the same source, check the effective S3 remote and restored outputs.
Repeat on Linux and macOS. A changed source must compile. A missing or
unavailable entry must also compile normally. Prove that reader credentials
cannot upload and pull request jobs cannot obtain the writer keys.

The pilot workflows retain redacted JSON cache reports for seven days as
`kache-native-clippy-<OS>-<ARCH>` and
`kache-bindings-check-<TARGET>-<OS>-<ARCH>`. The report covers the job's compiler
activity before the Kache action's final post step. Use it with the action's
final job summary to compare hits, misses, and remote transfers. A first
writer run can be cold. A later reader run must reuse matching entries before
the wider routing change is accepted.
Report collection and artifact service errors do not change the compiler
job's result. If a report is missing, cache-reuse acceptance still needs its
evidence before the wider routing PR can proceed.

Keep the other callers local-only until these checks pass. Then route their
reader or writer settings in a separate PR with a stable scope for each build
variant. Do not use commit IDs or run IDs in the scope.

To stop S3 use, remove both credential fields at both GitHub scopes. The jobs
then use local-only Kache. Removing only the bucket while keeping credentials
is a configuration error. For key rotation, follow the two-apply process in
the infrastructure guide, then update the GitHub pair before deleting the old
key.

Check changes with `dev/nix-shell 'just lint-config'` and
`dev/nix-shell 'just agent-test'`. The fixtures use fake credentials and never
start Kache or access S3.
