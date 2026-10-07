# GitHub workflows

Validate workflow edits with `dev/nix-shell 'just lint-config'`.

Mobile release tags use `GH_APP_ID` and `GH_APP_PK` for the release App.
The App installation must grant Contents write and Workflows write.
App tokens are available only in isolated permission and tag push jobs.
These jobs do not run SDK or release tools. The tag push job imports a Git
bundle into a new bare repository.
Android tags must point to `github.sha`. iOS can use one direct release commit
that changes only the iOS release files. The push job checks this before it
creates its token.
`just lint-config` checks the tag transfer with a local Git HTTP remote.

- Do not enable full Nix build logs by default in CI. For explicit debugging, run `nix log <drv-path>` or add `--print-build-logs` to a manual `nix build` command.
- Pass JavaScript shard flags directly to the `just` recipe. An extra `--` is forwarded to Vitest and prevents sharding.

- The iOS jobs and the Swift job in `test-sdk.yml` use disposable native
  services through `dev/nix-shell 'just backend ci COMMAND'`. Each job creates
  its own database and S3 bucket. Failed-job-only reruns do not need a
  deployment job.
- Keep the Swift job filters (`ios`, `ios_direct`, `sdk_swift`) current when
  native setup inputs change.
  `test-native-backend.yml` checks wrapper cleanup and the real S3 contract.
  It also checks the owned loopback listeners and metrics endpoint. Backend
  source and build-input changes select this job, and it gates aggregate
  `Test`. The native acceptance job has no cache-write token.
  Service logs are retained for 7 days.
- `test-sdk.yml` gates aggregate `Test`; `test-ios` and `test-android` do not.
  The Swift seam proofs (`just ios test-seams`) and the Swift and Kotlin
  consumer checks run in `test-sdk.yml`. The `sdk` and `sdk_swift` filters
  list their paths under `sdks/`. `test-ios` runs `just ios test skip-seams`,
  so its macOS run does not repeat the seam proofs.
