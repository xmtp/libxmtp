# GitHub workflows

Validate workflow edits with `just lint-config`.

- Keep `--print-build-logs` on the workspace test build. Failed Nix builds must expose the full test summary in CI.
- Pass JavaScript shard flags directly to the `just` recipe. An extra `--` is forwarded to Vitest and prevents sharding.

- `test-ios.yml` destroys its Fly backend during cleanup. Do not rerun only failed iOS test jobs after cleanup: they reuse the deleted backend URL. Rerun the deployment and dependent jobs, or the full workflow.
