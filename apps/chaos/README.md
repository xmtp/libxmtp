# Local chaos suite

`xchaos` checks whether real libxmtp clients converge after concurrent
operations and temporary faults. Each installation runs in a child process.
The suite uses this worktree's backend and one Toxiproxy proxy per process.
One additional process shares a database; only its stream lease holder streams.

Each seeded round runs concurrent operations, clears faults, and checks
barrier obligations, membership, group state, and message delivery. A seed
fixes the schedule. Operating system timing can change the actual interleaving.
Each run creates fresh inboxes, even when it repeats a schedule seed. This keeps
old installations on the backend from entering the new run's membership.

## Start with small local checks

```sh
dev/nix-shell --shell rust 'just backend up'
dev/nix-shell --shell rust 'just chaos test'
dev/nix-shell --shell rust 'just chaos baseline --rounds 3 --seed 1337'
dev/nix-shell --shell rust 'just chaos run --rounds 3 --seed 1339 --faults network'
dev/nix-shell --shell rust 'just chaos status'
```

Do not run other proxy tests at the same time. The recipes select the correct
worktree ports and set `XMTP_NO_PANIC_ON_DB_LOCK=1` for child processes.

The implementation is delivered as one PR. Actual Murmur runs require the
user's approval of that PR and successful small local runs first. Follow the
[agent runbook](AGENTS.md) after those conditions are met.

## Commands and options

| Command | Purpose |
| --- | --- |
| `just chaos build` | Build the supervisor and child command |
| `just chaos test` | Run the app tests without automatic retries |
| `just chaos baseline` | Run 50 rounds without faults |
| `just chaos run` | Run a bounded schedule with faults |
| `just chaos soak --hours 8` | Run a sustained schedule after approval |
| `just chaos status` | Show bounded progress |
| `just chaos inspect [bundle]` | Show the newest or selected failure bundle |

Wrap each command with `dev/nix-shell --shell rust '...'`.
Runs accept `--rounds`, `--hours`, `--seed`, `--faults`, `--strict`, and
`--directory`. Fault sets are `none`, `network`, `disk`, `crash`, and `all`.
Use a comma-separated list to combine sets, such as `--faults network,disk`.
Baseline always disables faults.

Network faults include disconnects, latency, bandwidth limits, timeouts,
byte limits, resets, slicing, and backend pauses. Disk faults include failed
queries, errors after a call runs, and connection loss. Process faults kill
and restart a child. Errors after a call runs are recorded as possible
completed work; they do not prove that a transaction committed.

## Results

| Result | Meaning |
| --- | --- |
| `PASS` | The round's checks passed |
| `FORK` | Installations that owe membership disagree at the checkpoint |
| `BRICK` | A required join, checkpoint, or message delivery did not complete |
| `STALL` | A pending obligation has a recorded blocking cause |
| `WARN` | A client reports a fork flag without observed state disagreement |
| `HARNESS` | The harness or a required service failed |

Exit codes are 0 for completion, 2 for a violation, 3 for a harness error,
and 130 for interruption. Healthy stdout contains the seed and one summary
line per round. Status includes contention counters.

On a violation, the supervisor stops writers before it copies databases.
Use `just chaos inspect [bundle] --group GROUP_ID` for group evidence or
`just chaos inspect [bundle] --db DATABASE_NAME` for a database record.
Inspection hides keys and caps output. Do not read raw logs or databases.

Run data under `.chaos/` is private and ignored by Git. Healthy runs retain
two rounds of ledgers and have limits on file count and total bytes.
The suite is outside the workspace's default members and CI.

## Traces and fork warnings

Child processes use `xmtp_logging` and export SDK spans to the worktree's Tempo.
The recipes select its OTLP port. `OTEL_EXPORTER_OTLP_ENDPOINT` can override it.
Export uses service name `xmtp-chaos` and sample ratio `1.0`. Normal child exit
flushes spans. A forced process kill can lose spans that have not been exported.

Use `just backend status` to find Grafana. In Explore, select Tempo and search:

```traceql
{ resource.service.name = "xmtp-chaos" }
```

Narrow the search with `resource.xmtp.chaos.run` and
`resource.xmtp.chaos.instance`. A `diagnostic.epoch_mismatch` span records the
message epoch, local epoch, sequence, and error code that set `maybe_forked`.
`chaos.command` spans identify commands without exporting their payloads.
SDK calls propagate trace context to the backend.
`chaos.stream` spans record errors and EOF. The harness reopens a completed
stream after recovery and waits within the SDK's lease duration for a previous
owner to expire. It does not reset a live stream to hide a delivery stall.

After a run stops, use `just chaos inspect --forks` to compare saved state and
commit history. Add `--group GROUP_ID` to narrow the output. This command reads
the encrypted databases through read-only connections. It does not start clients.
A row cap or missing evidence is reported as inconclusive.
