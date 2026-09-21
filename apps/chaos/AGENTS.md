# Chaos suite

`xchaos` is a local Rust app. It runs real client processes against this
worktree's self-hosted backend. It is outside `default-members` and CI.

## Run gate

This implementation is delivered as one PR on the approved branch.
Do not start an actual Murmur run until the user approves that PR and small
local runs pass. The first local runs must show no immediate failures.
A plan approval is not approval to start Murmur.

Before a Murmur session, record the PR approval and the results of the local
checks. Then run the 50-round baseline. It must pass before a fault run counts.

## Commands

Run commands from the repository root. Each command needs the Nix wrapper:

```sh
dev/nix-shell --shell rust 'just chaos build'
dev/nix-shell --shell rust 'just chaos test'
dev/nix-shell --shell rust 'just backend up'
dev/nix-shell --shell rust 'just backend status'
```

The recipes load this worktree's environment and set
`XMTP_NO_PANIC_ON_DB_LOCK=1` for the supervisor and its child processes.
Use the worktree's own stack. Never supply another worktree's ports.
The suite owns Toxiproxy while it runs. Do not run proxy tests, another chaos
run, or backend shutdown commands in the same worktree at that time.

Run a few short schedules before the PR is approved for Murmur:

```sh
dev/nix-shell --shell rust 'just chaos baseline --rounds 3 --seed 1337'
dev/nix-shell --shell rust 'just chaos baseline --rounds 3 --seed 1338'
dev/nix-shell --shell rust 'just chaos run --rounds 3 --seed 1339 --faults network'
dev/nix-shell --shell rust 'just chaos run --rounds 3 --seed 1340 --faults all'
```

A run that exits 2 or 3 has not passed. Inspect its evidence and fix the cause
before starting a longer run. Report which fault kinds the short runs covered;
a short run need not cover every fault kind.

## Murmur runbook

After the run gate is satisfied:

1. Start the worktree's backend and run
   `dev/nix-shell --shell rust 'just chaos baseline --rounds 50'`.
2. Confirm exit 0. Check the reported contention counters. A run with no
   contention gives weak evidence for concurrent commit handling.
3. Start `dev/nix-shell --shell rust 'just chaos soak --hours 8'` as a managed
   background process. Keep its process handle and exit status.
4. Poll `dev/nix-shell --shell rust 'just chaos status'` about every five
   minutes. Use the bounded output to report progress.
5. On exit 2, run `dev/nix-shell --shell rust 'just chaos inspect'`. Report
   the seed, round, verdict, group, affected installations, and bundle path.
6. On exit 3, report a harness or service error. Do not label it an SDK defect.
7. On interruption, wait for cleanup and child exit. Do not copy databases
   while any writer is active.

Exit codes: 0 finished, 2 invariant violation, 3 harness error, 130 interrupted.
A `STALL` reports an incomplete obligation with its cause. Repeated stalls can
escalate. A `WARN` alone does not stop a normal run; `--strict` enables the
stricter policy.

## Evidence and output

Never open raw logs, JSONL ledgers, population secrets, or database files.
Do not use `cat`, `tail`, `rg`, or a SQL tool on them. Use only the bounded
status and inspect commands:

```sh
dev/nix-shell --shell rust 'just chaos status --directory .chaos/RUN'
dev/nix-shell --shell rust 'just chaos inspect .chaos/RUN'
dev/nix-shell --shell rust 'just chaos inspect .chaos/RUN --group GROUP_ID'
dev/nix-shell --shell rust 'just chaos inspect .chaos/RUN --db DATABASE_NAME'
```

Inspection caps its input and output and hides keys. A capped result is not
permission to read the raw file. Narrow the group or database selector.
The run directory contains private wallets and database keys. Do not commit
or upload it. Keep failure evidence until the user has reviewed the result.

Use Tempo/Grafana to trace SDK operations. The children export service name
`xmtp-chaos`, with run and instance resource attributes. See the README for
TraceQL queries. Query only the relevant run and cap returned trace counts.
Use `just chaos inspect --forks [--group GROUP_ID]` on a stopped run to compare
the persisted evidence. Traces alone do not prove that MLS state agrees.
Follow the [programmatic trace guide](../../docs/querying-traces.md) for bounded
Tempo API commands and SDK/backend correlation checks.

Healthy runs retain two rounds of ledgers. The suite also limits file count
and total bytes. If a limit stops the suite, report a harness error; do not
remove evidence to conceal it.
