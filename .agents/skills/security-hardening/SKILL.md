---
name: security-hardening
description: Run a requested, multi-model security hardening audit of libxmtp in Docker Sandboxes through finding triage and a remediation plan. Do not use for routine code review or implement fixes.
---

# Security hardening

Run this skill only when the user invokes it. The host agent coordinates the
audit; all discovery and exploit work runs in the isolated `dev/sbx` VM. Stop
after a reviewed remediation plan. Fixes belong in separate sessions.

Read [the sandbox guide](../../../dev/sbx/README.md),
[the spec index](../../../docs/specs/README.md), and
[the prompts and records](references/prompts-and-records.md). Read the
[guest lane runner](references/lanes.md) before dispatch. Read the
`code-factory:calling-claude` and `code-factory:calling-codex` skills when
available. A Codex host uses native subagents for its own work; the user asked
for headless Claude and Codex CLI sessions **inside** the VM. Do not assume the
host and guest have the same installed plugins or authentication.

## 0. Design the run with the user

Ask once for the audit commit, available providers and exact models, access
programs, a cost limit, desired concurrency, concern areas, previous
`NON_ISSUES.md`, and any scanner reports to import. Offer a practical default
matrix if the user has no preference. Ask for the path to a host-only `.env`
file with mode `0600`; never ask the user to paste keys into the chat. Build a
run manifest with the commit SHA, provider/model/effort for each lane, prompt filenames, run
limits, and review checkpoints. Write the exact prompt files, show them to the
user, and incorporate their edits before launching paid lanes.

Use `dev/sbx/sandbox start` from the host. The wrapper checks that this is the
unmounted VM, that the Docker MCP catalog is empty, and that host SSH-agent
forwarding is disabled. Keep the `.env` on the host. For API keys in it, use
[`scripts/provision.py`](scripts/provision.py) after startup. It reads a
restricted dotenv file and sends selected keys to `sbx` through stdin, never
as arguments or guest files. Native OpenAI/Anthropic keys use Docker's native
service secret path, which the current kits can expose in guest environment
variables. Offer subscription OAuth if the user needs stronger isolation.
For example, select only the providers the user chose:

```sh
dev/nix-shell 'python3.11 .agents/skills/security-hardening/scripts/provision.py --env-file "$HOME/.config/libxmtp/security-hardening.env" --native openai:OPENAI_API_KEY --custom api.morphllm.com:sbx-morph-libxmtp:MORPH_API_KEY'
```

The dotenv file accepts `NAME=value` lines and comments, with no shell
evaluation. The script checks file ownership and permissions before use.
Record only key names, proxy placeholders, and provider endpoints in the
manifest. Verify each selected CLI works in the VM before the full matrix; a
refusal or missing access is a blocked lane, not a reason to retry repeatedly.
Use the login recipes in `dev/sbx/README.md` for native CLI accounts. A
subscription login may need a one-time user action inside the VM.

## 1. Discover in parallel

Use SSH to work in `~/libxmtp` inside `libxmtp-claude.sbx`. Verify the requested
commit exists there and record its full SHA. Keep run files under
`~/security-hardening/<run-id>/`. Discovery lanes share one detached worktree
at that SHA. The code checkout is for reading; each lane writes reports in its
own run directory. Check the shared checkout for source changes after the lanes
finish and retain any unexpected diff as evidence. Copy only prompts and public
context into the VM; do not mount host folders or copy the host `.env`.
Use the guest lane runner for the approved
prompt matrix and bounded concurrency. Each lane writes its prompt,
model, effort, access mode, start/end time, CLI session ID, output stream,
errors, and findings to its own directory. Require no Git push or external
mutation. Check Claude's final `result` event and Codex's final turn state,
then check that the expected findings files exist. A zero process exit code
alone is not a pass. Resume or stop a lane using its saved session ID; keep
the host turn active while its child processes run. Report progress and blocked
lanes without silently replacing models or prompts.

The prompt pack includes spec design, broad code, dependency, PR-history, and
user concern lanes. Use installed audit tools only when the user selects them.
Apply repo rules in the guest, including Nix commands and approved specs.

## 2. Deduplicate

The **host agent** owns deduplication. Fetch the raw lane and selected scanner
reports from the VM, verify their inventory and hashes, and normalize them into
`findings/F-001.md`, etc. Merge by root cause, retain every source and
scenario, and do not discard or rescore at this stage. Keep a source-to-finding
map so every candidate is accounted for. Use native host subagents for
independent coverage and semantic checks. Run an adversarial review of the
complete merged list, fix material routing or fidelity errors, and recheck the
source map before offering the list for user review. If the user requests a
review before validation, publish the combined list and pause for feedback.
Keep the raw reports and mapping outside the validation branch. Copy the
reviewed normalized findings into the VM and commit them on a local branch at
the audit SHA so validation lanes share one fixed base. Do not send imported
scanner reports to another provider without the user's authorization.

## 3. Validate

Create a pool of detached worktrees, capped at eight active slots unless the
user chooses another limit. Assign one validation agent per finding or small
group with distinct file ownership. Give each agent only its own finding files
to edit and its own repro files. Use the guest lane runner based on the local
deduplication commit. It saves each agent's edits in a local Git ref, records
test output, and resets the slot before assigning the next agent. Check saved
refs and results before integrating any edits. Use the status definitions in
the prompt reference: `CONFIRMED`, `NARROWED`, `REFUTED`, `UNPROVEN`, or
`BLOCKED`. A failed exploit is not a
refutation. Check each lane's result, then copy only its assigned finding and
repro files from its saved ref into the integration worktree. Build `report.md`
from the integrated finding files. Do not push.

**Pause after validation.** Show the status summary and unresolved limits.
Let the user choose another model to clean up the documents, adversarially
review findings or refutations, repeat selected tests, or proceed. Run each
extra pass in its own worktree and keep its critique and edits attributable.
Rebuild the report after accepted changes.

## 4. Review with the user

Assemble a review document from the VM's `report.md`, finding files, and
minimal repro code using SSH. Use a Codex Space Page or Claude Artifact if the
host can create one; otherwise present a local Markdown document. For a Space
Page, follow `pages:write-page` if available and create the draft from the
fetched Markdown. Include links or paths back to full VM evidence and make
uncertain findings visible.
Review findings with the user. Record rejected or expected behavior in
`NON_ISSUES.md` with reasons; do not silently delete records. If expected
behavior exposes a spec or threat-model gap, call it out.

**Pause during triage.** The user can request another model's document cleanup
or adversarial review before finalizing the finding set. Keep the original
evidence and the reviewer's changes distinct. Continue when the user has
selected the findings to plan.

## 5. Plan, then stop

Group retained `CONFIRMED` and `NARROWED` findings by root cause and code
area. Use existing code patterns where possible and sequence shared fixes
before local fixes. Put relevant repro snippets and test paths into the review
document, then create a remediation plan with validation steps and a complete
finding-to-plan mapping. List `UNPROVEN` and `BLOCKED` findings separately for
follow-up. If a proposed fix changes a spec requirement, the repo requires an
approved Ref plan before implementation. Present the plan for review. Do not
implement fixes in this run.
