# GitHub workflows

Validate workflow edits with `dev/nix-shell 'just lint-config'`.

- Do not enable full Nix build logs by default in CI. For explicit debugging, run `nix log <drv-path>` or add `--print-build-logs` to a manual `nix build` command.
- Pass JavaScript shard flags directly to the `just` recipe. An extra `--` is forwarded to Vitest and prevents sharding.

- The iOS and Swift conformance jobs use disposable native services through
  `dev/nix-shell 'just backend ci COMMAND'`. Each job creates its own database
  and S3 bucket. Failed-job-only reruns do not need a deployment job.
- Keep both Swift job filters current when native setup inputs change.
  `test-native-backend.yml` checks wrapper cleanup and the real S3 contract.
  Service logs are retained for 7 days.
