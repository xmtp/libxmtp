# GitHub workflows

Validate workflow edits with `just lint-config`.

- Do not enable full Nix build logs by default in CI. For explicit debugging, run `nix log <drv-path>` or add `--print-build-logs` to a manual `nix build` command.
- Pass JavaScript shard flags directly to the `just` recipe. An extra `--` is forwarded to Vitest and prevents sharding.

- `test-ios.yml` destroys its Fly backend during cleanup. Do not rerun only failed iOS test jobs after cleanup: they reuse the deleted backend URL. Rerun the deployment and dependent jobs, or the full workflow.
