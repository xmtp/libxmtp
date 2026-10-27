# Guest lane runner

Use [`guest_lanes.py`](../scripts/guest_lanes.py) for parallel discovery,
validation, and optional model passes. It runs **inside** the `sbx` VM. The
host agent prepares the manifest and prompts after user review, then copies
only those non-secret files and this script over SSH. Keep each phase in a
separate run directory. Do not copy the host credential file.

Example `manifest.json` inside `~/security-hardening/<run-id>/`:

```json
{
  "repo": "/home/agent/libxmtp",
  "base": "<full-40-character-audit-commit>",
  "concurrency": 4,
  "lanes": [
    {
      "id": "spec-claude",
      "cli": "claude",
      "provider": "anthropic",
      "model": "opus",
      "effort": "xhigh",
      "max_budget_usd": 20,
      "prompt": "prompts/spec-claude.md"
    },
    {
      "id": "code-codex",
      "cli": "codex",
      "provider": "openai",
      "model": "gpt-6.1-sol",
      "effort": "xhigh",
      "cyber_access": "daybreak_blue",
      "prompt": "prompts/code-codex.md"
    }
  ]
}
```

For an Anthropic-compatible third-party Claude provider, add a `settings`
path to the lane. That JSON file may contain only the provider URL, model
aliases, context limit, and `sbx-*` placeholder. Use the pattern in
`dev/sbx/README.md`. Do not include a real key. A Claude subscription lane
needs no settings file. Select `cyber_access` only when the user has that
access and wants it for this lane.

After verifying the guest has the exact commit, copy the reviewed run files.
From the guest run directory:

```sh
python3 guest_lanes.py init "$PWD"
python3 guest_lanes.py start "$PWD"
python3 guest_lanes.py status "$PWD"
```

`init` creates detached worktrees at the manifest's base commit. `start`
launches a background supervisor in the VM, up to `concurrency` lanes at
once. The supervisor survives the SSH connection; the host polls `status`
over SSH and stays on the task until all lanes finish or need attention.
Claude's optional `max_budget_usd` limits one lane; Codex has no matching
runner limit, so the host tracks its usage against the user's run budget.
It never retries a failed or vanished lane automatically. Use
`python3 guest_lanes.py stop "$PWD" <lane-id>` to stop a running lane.
After a restart, inspect `status` and the files under `lanes/<id>/` before
deciding whether to resume the CLI session or create a new lane ID.
If a Claude `result` event exists while its lane still runs, stop that lane
after a brief grace period and inspect the final event; a child process may
have kept the CLI alive. For a manual continuation, use Claude `--resume`
with the saved UUID or `codex exec resume` with the saved thread ID, the same
worktree and permissions, and a new numbered prompt and output file. Record
that continuation in the run log.

Each lane has `session.json`, `events.jsonl`, `stderr.log`, and then
`result.json`. `done` requires a successful process exit and a successful
Claude `result` or Codex `turn.completed` event. `failed` and `died` require
inspection. Read the final event and the actual findings/repro files. The
runner does not judge the report's quality or merge worktree changes.

The runner uses Claude's headless write mode because the outer VM is the
isolation boundary. Codex uses `workspace-write` with access to the run
directory and repository Git metadata. Keep guest network policy scoped to
the selected provider and build dependencies. Do not change a lane to
unsandboxed Codex mode without the user's approval for that run.
