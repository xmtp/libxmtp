---
name: smoke-check
description: Run a local libxmtp smoke check with web chat and two Node SDK bots, only when the user explicitly requests this skill or a smoke check.
---

# Smoke check

Use this skill only when the user explicitly requests it or asks for a smoke
check. Do not run it as routine validation after other changes.

Use the local Node SDK and web chat to check real message delivery. The agent
must have a browser tool. Run all web chat steps through the browser UI.
Use the built-in browser when the user requests it.

## Setup

Read the repository rules. Use the checkout the user selects. Create a worktree
from `origin/self-hosted` only when the user requests one. Record which processes,
containers, and tabs this run starts so cleanup can stop the correct resources.

Run these commands from the repository root:

```bash
dev/nix-shell 'just install-js'
dev/nix-shell 'just backend status'
# Start the stack if it is not already running.
dev/nix-shell 'just backend up'
dev/nix-shell 'just backend status'
dev/nix-shell 'just js sdk-products'
```

Read the backend URL from the status output. Each worktree has its own ports.
Do not assume port 5050. Check inherited `XMTP_BACKEND_URL` values: the environment
loader preserves an existing value. Both bots and web chat must use the same
local backend.

The first SDK build can compile the native library, SDK generator, WebAssembly,
and pure codecs. Let the build finish. Check compiler progress if output is quiet.
Give progress updates. Do not start another generation against the same output
folders while it is running.

## Start the two bots

The skill includes [ping-bot.mjs](scripts/ping-bot.mjs),
[bleep-bot.mjs](scripts/bleep-bot.mjs), and their shared
[run-bot.mjs](scripts/run-bot.mjs). Use the bundled scripts in place;
they load the SDK and `viem` from the selected repository. No separate package
installation in the skill folder is needed.

Set `XMTP_SMOKE_SKILL` to the folder that contains this `SKILL.md`. In separate
terminal sessions, run:

```bash
export XMTP_SMOKE_SKILL="$PWD/.agents/skills/smoke-check"
dev/nix-shell 'dev/worktree-env && . dev/docker/load-env && node "$XMTP_SMOKE_SKILL/scripts/ping-bot.mjs" "$PWD"'
dev/nix-shell 'dev/worktree-env && . dev/docker/load-env && node "$XMTP_SMOKE_SKILL/scripts/bleep-bot.mjs" "$PWD"'
```

Wait for each bot's `ready` event. Record its inbox ID, backend URL, run ID,
log path, and process handle. Each script prints its inbox ID on startup.

The ping bot replies to exact plain text `ping` with `pong`. The bleep bot replies
to exact plain text `bleep` with `bloop`. They ignore all other text, nontext
content, and their own messages. Responses use `message.reply`, so web chat
shows threaded replies. Logs include the request ID and reply ID.

The current `self-hosted` SDK has no `streamAllMessages` method. Its all-message
API is `MessageStream.open(client, {})`. The scripts use that API and await
`ready()`. They do not sync or replace a reader for each message or new group.
Successful loop iterations allow the SDK to acknowledge the previous item.

Bot state and JSON logs stay under `target/smoke-check/`, separated by backend
origin and bot. A restart against the same backend uses the same inbox ID.
Keep wallet keys and databases out of source control and reports. Logs append
across restarts; use `runId` to select one process run.

Start web chat in a third terminal:

```bash
dev/nix-shell 'just web-chat dev'
```

If products were just generated from this checkout, set
`XMTP_SDK_GENERATED_DIR` to its absolute `target/sdk-generated` path before
these commands to reuse that input. Keep the package stager's validation.
Read the frontend URL from Vite output; its port can change.

## Browser checks

Open the frontend URL. Select an ephemeral wallet and connect to the exact
backend URL used by the bots. Check the connected backend label. Dismiss the
app's informational notice if it appears.

Use the app's navigation menu for new DMs and groups. A direct navigation that
reloads the page can disconnect the client. In the observed run, a reload also
left an app session lock. If this happens for this test account, use
`Disconnect other session`, close the takeover notice, and connect again.
Do not take over an unrelated user session.

Run these checks. Wait for each expected reply before sending the next trigger.
Use a bounded wait, such as 15 seconds. On failure, save the visible state and
bot logs. Diagnose the failure; do not report a pass from startup alone.

| Check | Browser action | Required result |
| --- | --- | --- |
| Ping DM | Create a DM with the ping inbox ID. Send `ping` three times. | One `pong` reply for each request, from the ping bot. |
| Ping exact match | Send `hello`, `Ping`, and `ping?`, then `ping`. | Only the last request gets a reply. |
| Bleep DM | Create a DM with the bleep inbox ID. Send `bleep` three times. | One `bloop` reply for each request, from the bleep bot. |
| Group membership | Create a named group. Add both inbox IDs through the Members section. | The browser account and both bots are members. |
| Group delivery | Send `ping`, `bleep`, `ping`, and `bleep`. | Each trigger gets one reply from the correct bot. |
| Group exact match | Send `hello`, `Ping`, `Bleep`, `ping?`, and `bleep?`, then `ping` and `bleep`. | Only the last two messages get replies. |
| Bot-to-bot receipt | Read both bots' group logs. | The ping bot receives the bleep bot's `bloop` reply IDs. The bleep bot receives the ping bot's `pong` reply IDs. Neither responds to those replies. |

Check sender IDs and request/reply IDs in the JSON logs. The later valid trigger
is a marker: its reply shows that the bot processed preceding ignored messages.
Verify that those preceding request IDs have no `replied` records. This avoids
relying only on a short period with no visible response.

Web chat uses a virtual message list. Old DOM nodes can disappear as new messages
arrive. Do not use a count of all current DOM matches as the full message count.
Use the UI to verify visible replies, and logs to check exact totals and IDs.
Do not create clients or send messages through browser page evaluation.

## Evidence and cleanup

Save a screenshot that shows both group replies, the group membership count,
and the backend label. Save the relevant bot JSON logs and a short result report.
Report each check as passed, failed, or not run. Include the tested commit,
backend URL, bot inbox IDs, group ID, and any failure or workaround.

When changing these scripts or the test procedure, prove that the relevant check
can fail. For example, copy the scripts folder to a test-only directory. Change
the comparison in its `run-bot.mjs` to use lowercase text.
Stop the correct ping process, run the copy with the same state, and send `Ping`.
The incorrect `pong` must fail the exact-match check. Stop the copy, remove it,
restart the correct bot, and repeat the ignored-message check followed by a valid
trigger. Keep mutation logs separate in the report. Do not leave the incorrect
bot running or alter the shared script for other runs.

Unless the user asks to leave the test running, stop the bots and frontend, then
close the test tab. Leave a pre-existing stack running. If the check started
extra services in that stack, stop only those services. For a worktree stack
this run created, use:

```bash
dev/nix-shell 'just backend release'
```

Do this before removing the worktree so its containers and port claim are gone.
Preserve useful evidence outside the worktree first: managed worktree archives
do not preserve ignored files. Use the app's archive tool for an app-managed
worktree. Verify that the checkout is removed and the stack is stopped. Keep
pre-existing backend stacks and user checkouts unless the user asks to remove them.

To check the bot matching contract after a script change, run:

```bash
dev/nix-shell 'node --test .agents/skills/smoke-check/scripts/run-bot.test.mjs'
```

These tests check bot matching and reply errors. They do not replace the live
browser and backend checks.
