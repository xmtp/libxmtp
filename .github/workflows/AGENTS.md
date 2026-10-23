# GitHub workflows

Validate edits with `dev/nix-shell 'just lint-config'` and
`dev/nix-shell 'just test dev'`.

- Edit `.github/ci-suites.yml`, then run
  `dev/nix-shell 'python3.11 dev/ci-suites generate'` and commit its generated
  JSON and `ci.yml`. Never edit `ci.yml` by hand. Read [CI selection](../CI.md)
  when changing filters, tiers, gates, runners, or the RN device chain.
- Preserve required gate names `Lint` and `Test` and failure on skipped selected
  blocking suites. Keep `native_backend`, `swift`, and example-app Nix path
  groups current when their inputs change.
- Keep advisory RN device jobs and warm sets out of non-RN post-merge dependencies.
- Every `setup-nix`/`setup-js` call declares the same cache mode. Read
  [cache modes and compiler scopes](../actions/setup-nix/README.md) before
  changing cache access. Writer credentials belong only to protected push jobs;
  keep them out of global `AWS_*` and Nix derivations.
- Pin release source SHA before SDK code runs. Release App tokens belong only
  in isolated permission/tag-push jobs that run no SDK tools. Preserve source
  branch admission, tag validation, and npm publish's own `release` environment
  and branch checks. Read [release wiring](../RELEASING.md) before editing it.
- Pass JS shard flags directly to Just, without an extra `--`.
- Keep full Nix build logs off by default. Scope `XMTP_NIX_BUILD_LOGS=1` to a
  debugging `dev/nix-shell` invocation, or use `nix log <drv-path>`.
  Retain failed Compose startup diagnostics from `$RUNNER_TEMP/backend-startup-logs`.
