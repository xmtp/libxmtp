# GitHub workflows

Validate workflow edits with `just lint-config`.

- `test-ios.yml` destroys its Fly backend during cleanup. Do not rerun only failed iOS test jobs after cleanup: they reuse the deleted backend URL. Rerun the deployment and dependent jobs, or the full workflow.
