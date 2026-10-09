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
The 13 stable scope families follow the outer Cargo build variant. The action
adds runner OS and architecture. All callers use `libxmtp/trusted`, prefetch expensive outputs, and do not pull the
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

Audited compiler jobs select this environment only on a protected branch push
to `main` or `self-hosted`. Other events use an empty environment name and
select no GitHub environment. Same-repository pull request runs use repository
reader secrets. Fork and Dependabot runs have no reader secrets and build
local-only. No reader environment is needed.

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
The [pinned action guidance](https://github.com/kunobi-ninja/kache-action/blob/1a33fb2ff51be23eb9e87abeae6edb65be78f71c/README.md#manifest-vs-shards)
separates build variants to keep their prefetch lists useful.
Scopes follow outer Cargo build families. The SDK bridge proof and JavaScript
SDK product builds share `sdk-products-debug`. The browser platform proof uses
`sdk-browser-conformance` because it builds debug panic-test and release
pure-codec fixtures with conformance features. The older SDK facade job uses
`sdk-products-tests-conformance` because it combines product builds, Rust unit
tests, and those browser fixtures. Node-only SDK builds share `sdk-native-debug`. Android staging and native Node release
builds use separate release scopes. Default SDK tests, backend tests, native
Clippy, WASM Clippy, Cargo check, and Rust docs retain distinct scopes.

A scope selects prefetch manifests and dependency shard indexes. It does not
partition compiled artifacts: all callers share matching artifacts under
`libxmtp/trusted`. Reader-only SDK release and deployment jobs use the same
debug build scopes as their writer jobs. An npm release still uses a debug
Cargo build when its SDK generation command does not request `--profile release`.

Jobs with no outer Cargo compilation share `nix-only`. This includes the
Unix Node platform matrix and the iOS and Android jobs that build through Nix.
They have no S3 compiler working set to separate by target. Nix-only jobs keep
Nix/Crane and Cachix caching; S3 cannot cache output inside Nix derivations.
The action appends runner OS and architecture to every scope. Node and browser
test shards share the build scope without adding shard numbers.

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
| `build-sdk-node-platforms.yml / unix` | `nix-only` | Reader |
| `build-sdk-node-platforms.yml / assemble` | `sdk-native-release` | Reader |
| `check-rust.yml / native` | `rust-clippy-native` | Protected push writer; otherwise reader |
| `check-rust.yml / wasm` | `rust-clippy-wasm` | Protected push writer; otherwise reader |
| `check-sdk-products.yml / generated` | `sdk-products-debug` | Protected push writer; otherwise reader |
| `check-sdk-unit.yml / unit` | `sdk-tests-debug` | Protected push writer; otherwise reader |
| `check-types.yml / check` | `sdk-products-debug` | Protected push writer; otherwise reader |
| `deploy-docs.yml / swift` | `nix-only` | Protected push writer; otherwise reader |
| `deploy-docs.yml / kotlin` | `nix-only` | Protected push writer; otherwise reader |
| `deploy-docs.yml / site` | `sdk-products-debug` | Protected push writer; otherwise reader |
| `deploy-docs.yml / compose` | `nix-only` | Reader |
| `deploy-web-chat.yml / deploy` | `sdk-products-debug` | Reader |
| `docs-quality.yml / quality` | `nix-only` | Reader |
| `docs-rust-reference.yml / rust` | `rust-reference` | Protected push writer; otherwise reader |
| `fh-cache.yml / build` | `nix-only` | Reader |
| `lint-android.yml / lint` | `nix-only` | Reader |
| `lint-config.yml / lint` | `nix-only` | Reader |
| `lint-ios.yml / lint` | `nix-only` | Reader |
| `lint-js.yml / lint` | `nix-only` | Reader |
| `lint-proto.yml / lint` | `nix-only` | Reader |
| `lint-workspace.yml / lint` | `nix-only` | Reader |
| `manual-sdk-recovery.yml / recovery` | `sdk-native-debug` | Reader |
| `release-agent-sdk.yml / build` | `sdk-native-debug` | Reader |
| `release-android.yml / publish` | `nix-only` | Reader |
| `release-browser-sdk.yml / build` | `sdk-products-debug` | Reader |
| `release-cli.yml / build` | `sdk-native-debug` | Reader |
| `release-ios.yml / build-and-package` | `nix-only` | Reader |
| `test-agent-sdk.yml / test` | `sdk-native-debug` | Protected push writer; otherwise reader |
| `test-android.yml / min-sdk-smoke` | `nix-only` | Protected push writer; otherwise reader |
| `test-android.yml / unit-tests` | `nix-only` | Protected push writer; otherwise reader |
| `test-android.yml / integration-tests` | `nix-only` | Protected push writer; otherwise reader |
| `test-bindings-check.yml / check-swift` | `bindings-check-${{ matrix.target }}` | Protected push writer; otherwise reader |
| `test-bindings-check.yml / check-android` | `nix-only` | Protected push writer; otherwise reader |
| `test-browser-sdk.yml / test` | `sdk-products-debug` | Protected push writer; otherwise reader |
| `test-ios.yml / tests` | `nix-only` | Protected push writer; otherwise reader |
| `test-native-backend.yml / native` | `backend-tests-debug` | Reader |
| `test-node-sdk.yml / test` | `sdk-native-debug` | Protected push writer; otherwise reader |
| `test-sdk-platform.yml / proof` | `sdk-products-debug` (bridge), `sdk-browser-conformance` (browser) | Protected push writer; otherwise reader |
| `test-sdk-staging.yml / android-stage` | `sdk-android-release` | Protected push writer; otherwise reader |
| `test-sdk.yml / sdk` | `sdk-products-tests-conformance` | Protected push writer; otherwise reader |
| `test-sdk.yml / android-stage` | `sdk-android-release` | Protected push writer; otherwise reader |
| `test-swift-lifecycle.yml / swift` | `nix-only` | Protected push writer; otherwise reader |
