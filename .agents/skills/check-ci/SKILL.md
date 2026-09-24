---
name: check-ci
description: Use when checking GitHub Actions results for a PR, investigating a failing job, or waiting on a run - covers compact status commands, log filtering, and how to wait without burning context
---

# Checking CI in libxmtp

Raw CI output is the most expensive thing an agent can read. A single unfiltered
job log is 10-12k tokens, and every token stays in context for the rest of the
session. Use these recipes instead.

## Status of a PR

```bash
dev/nix-shell 'just ci-status 4096'
```

Failures first with job URLs, then one summary line:

```text
FAILED:
  test-node-sdk / Test (node-sdk) shard 2  https://github.com/.../job/103436317046
RUNNING: 1 job(s)
SUMMARY: 38 ok, 3 failed, 6 skipped, 1 running
```

Duplicates are collapsed to the latest attempt per job, so a re-run does not
show up twice.

## Why a job failed

Take the job id from the `ci-status` URL:

```bash
dev/nix-shell 'just ci-failures 103436317046'
```

Returns only failure markers - test names, assertions, panics, `rustc` errors,
the failing recipe. Works for Rust and JS jobs. Typically 5-10 lines.

If the job records annotations, they are cheaper still and include file and
line:

```bash
dev/nix-shell 'just ci-annotations 103435038571'
```

## Waiting

Block. Do not poll.

```bash
gh run watch <run-id> --exit-status
```

A `sleep`-and-recheck loop costs one full-context model call per tick. In the
2026-09-10 session, 286 such polls consumed roughly 37M input tokens, and 40-65%
of what they returned was unchanged "queued" or "in progress" text.

If you must loop, back off: 60s, then 120s, then 300s.

## Rules

- Never run `gh api .../logs` without a filter. That is the 10-12k token read
  these recipes exist to replace.
- Never open a browser for CI. Everything is in `gh`; the Blacksmith UI shows
  nothing `just ci-annotations` does not.
- Check the plan's `## Findings` section before investigating a failure. These
  tests are flaky here and have each been re-diagnosed several times by
  different agents: `testCanSuccessfullyThreadDms`, `testNetworkDebugInformation`,
  `testCanStreamGroupMessages`.
- Record the verdict in `## Findings` when you finish, so the next agent does
  not repeat the work.
- "Re-run failed jobs" tests the same old merge commit again. When the fix
  landed on the base branch after that commit, rebase the PR branch and push.
  Do not re-run.
