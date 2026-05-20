# dev/drivers — xdbg cross-test harness

Python drivers that build multiple xdbg versions and exercise them
against a shared backend to catch cross-version MLS regressions.

## What runs

- **`cross-version-test`** — N xdbg versions sharing one `XVT_DB_ROOT`,
  each runs `healthcheck`. Catches regressions where a new version
  can't read state written by an older one.
- **`cross-talk-test`** — same N versions, each under
  `--strict-versioning` so identities are partitioned per version. One
  bootstraps a group, the rest join + sync + healthcheck. Catches
  wire-level MLS interop breaks across versions in one live group.
- **`xdbg_driver_lib_py`** — shared module (git, version picking, xdbg
  invocation, summary rendering). Used by both drivers.

## Plan

Both fetch `release/*` branches + recent nightly tags, then
`pick_versions(sample_size)` selects the last two stable releases +
HEAD + N most-recent nightlies. First required entry is the `creator`;
rest are `sender`s.

## Run

```bash
nix run .#cross-talk-test -- run
nix run .#cross-version-test -- run --profile nightly
```

Each writes `summary.json` to `$XVT_OUT_DIR` (or
`$TMPDIR/{ctt,xvt}-out-$PID`) and appends a step summary to
`$GITHUB_STEP_SUMMARY` when set. Non-zero exit on required-version
failure; nightly failures are soft in `--profile nightly`.

## Develop

Edit any `.py` here and re-`nix run` — sources are vendored into the
derivation each build. No pip install, no venv.

```bash
nix fmt              # ruff format + check (matches CI)
```
