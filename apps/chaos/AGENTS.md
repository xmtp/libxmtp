# Chaos suite

`xchaos` runs real client processes against this worktree's self-hosted backend.
It is outside `default-members` and CI.

## Run gate

Deliver this implementation as one PR on the approved branch. Do not start an
actual Murmur run until the user approves that PR and small local runs pass
without immediate failures. Plan approval does not approve a Murmur run.
Record the PR approval and local results before the run. A 50-round baseline
must pass before a fault run counts.

## Commands and evidence

Run `just chaos` recipes through `dev/nix-shell --shell rust` from the repository
root. The recipes select this worktree's backend ports and set
`XMTP_NO_PANIC_ON_DB_LOCK=1`. The suite owns Toxiproxy while it runs. Do not
run another proxy test, chaos run, or backend shutdown in this worktree at the
same time.

Never open raw logs, JSONL ledgers, population secrets, or database files.
Use only the bounded `just chaos status` and `just chaos inspect` commands for
run evidence. Do not commit or upload `.chaos/`; it contains private wallets
and database keys. Keep failure evidence until the user reviews it.

See [the chaos README](README.md) for local schedules, the Murmur run sequence,
exit codes, bounded inspection, and trace queries.
