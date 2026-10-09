# CI compiler cache

`setup-nix` uses pinned Kache 1.0.0 and its private runtime directory.
GitHub cache archives are disabled for the compiler store. Nix/Crane, Cachix,
pnpm, Windows rust-cache, Gradle, and Docker keep their existing caches.

Without either access key field, Kache uses a private local store. A partial
pair is an error. Complete credentials require a bucket and build scope before
the action starts its S3 daemon. Both modes keep the 10 GiB local limit and
the current compiler wrapper and flags. The local limit does not cap S3.
The job summary reports the backend and whether remote writes are enabled.

## S3 coverage

All 42 enabled `setup-nix` callers use S3 when the reader key pair is
available. This includes callers that use the default `kache: true` setting.
The nine callers with `kache: false` stay disabled and receive no S3 inputs.
Stable scopes include the build variant, runner OS and architecture. All
callers use `libxmtp/trusted`, prefetch expensive outputs, and do not pull the
whole bucket at startup.

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

Audited compiler jobs select this environment only on a protected branch push to
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

Existing deployment jobs keep their deployment environment and URL. The
`web-chat` environment must contain no writer keys under the two Kache secret
names. Leave those environment secrets absent to use repository reader keys.
Release jobs, jobs that build a selected ref, manual recovery, and native
backend acceptance use `kache-readonly: true` and never select the writer
environment. Source lint, docs quality and composition, and Nix output warming
also remain readers because they do not need outer Cargo cache writes.
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
Node and browser test shards share a scope because they compile the same
products. Node platform scopes include the matrix target. SDK platform proof
scopes use the validated `bridge` or `browser` input. Nix-only jobs keep
Nix/Crane and Cachix caching; S3 cannot cache output inside Nix derivations.

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
“Protected push writer” requires all event and branch checks above. Every
other event is a reader. No new bucket, key pair, or repository variable is
needed for this rollout.

| Workflow / job | Scope | Access |
| --- | --- | --- |
| `build-sdk-node-platforms.yml / unix` | `sdk-node-${{ matrix.target }}` | Reader |
| `build-sdk-node-platforms.yml / assemble` | `sdk-node-assemble` | Reader |
| `check-rust.yml / native` | `rust-clippy-native` | Protected push writer; otherwise reader |
| `check-rust.yml / wasm` | `rust-clippy-wasm` | Protected push writer; otherwise reader |
| `check-sdk-products.yml / generated` | `sdk-products` | Protected push writer; otherwise reader |
| `check-sdk-unit.yml / unit` | `sdk-unit` | Protected push writer; otherwise reader |
| `check-types.yml / check` | `sdk-types` | Protected push writer; otherwise reader |
| `deploy-docs.yml / swift` | `docs-swift` | Protected push writer; otherwise reader |
| `deploy-docs.yml / kotlin` | `docs-kotlin` | Protected push writer; otherwise reader |
| `deploy-docs.yml / site` | `docs-site` | Protected push writer; otherwise reader |
| `deploy-docs.yml / compose` | `docs-compose` | Reader |
| `deploy-web-chat.yml / deploy` | `web-chat-deploy` | Reader |
| `docs-quality.yml / quality` | `docs-quality` | Reader |
| `docs-rust-reference.yml / rust` | `rust-reference` | Protected push writer; otherwise reader |
| `fh-cache.yml / build` | `nix-all-outputs` | Reader |
| `lint-android.yml / lint` | `android-lint` | Reader |
| `lint-config.yml / lint` | `config-lint` | Reader |
| `lint-ios.yml / lint` | `ios-lint` | Reader |
| `lint-js.yml / lint` | `js-lint` | Reader |
| `lint-proto.yml / lint` | `proto-lint` | Reader |
| `lint-workspace.yml / lint` | `rust-source-lint` | Reader |
| `manual-sdk-recovery.yml / recovery` | `sdk-recovery` | Reader |
| `release-agent-sdk.yml / build` | `release-agent-sdk` | Reader |
| `release-android.yml / publish` | `release-android` | Reader |
| `release-browser-sdk.yml / build` | `release-browser-sdk` | Reader |
| `release-cli.yml / build` | `release-cli` | Reader |
| `release-ios.yml / build-and-package` | `release-ios` | Reader |
| `test-agent-sdk.yml / test` | `agent-sdk-test` | Protected push writer; otherwise reader |
| `test-android.yml / min-sdk-smoke` | `android-min-sdk` | Protected push writer; otherwise reader |
| `test-android.yml / unit-tests` | `android-unit` | Protected push writer; otherwise reader |
| `test-android.yml / integration-tests` | `android-integration` | Protected push writer; otherwise reader |
| `test-bindings-check.yml / check-swift` | `bindings-check-${{ matrix.target }}` | Protected push writer; otherwise reader |
| `test-bindings-check.yml / check-android` | `bindings-check-android` | Protected push writer; otherwise reader |
| `test-browser-sdk.yml / test` | `browser-sdk-test` | Protected push writer; otherwise reader |
| `test-ios.yml / tests` | `ios-test` | Protected push writer; otherwise reader |
| `test-native-backend.yml / native` | `native-backend-acceptance` | Reader |
| `test-node-sdk.yml / test` | `node-sdk-test` | Protected push writer; otherwise reader |
| `test-sdk-platform.yml / proof` | `sdk-platform-${{ inputs.target }}` | Protected push writer; otherwise reader |
| `test-sdk-staging.yml / android-stage` | `sdk-android-stage` | Protected push writer; otherwise reader |
| `test-sdk.yml / sdk` | `sdk-facade` | Protected push writer; otherwise reader |
| `test-sdk.yml / android-stage` | `sdk-android-stage` | Protected push writer; otherwise reader |
| `test-swift-lifecycle.yml / swift` | `swift-lifecycle` | Protected push writer; otherwise reader |
