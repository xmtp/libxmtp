# XMTP Debug

### Debug your app on a self-hosted backend

Supported Features:

- Generate Identities
- Generate Groups
- Generate Messages
- Inspect Generated Local Identities/Groups
- Export Generated Identities/Groups to JSON
- Invite external members to generated groups
- Three Supported log formats (Human, JSON, and logfmt)
  - log formats can be used for debugging, JSON & logfmt formats may be used
    with tools like [hl](https://github.com/pamburus/hl) or
    [lnav](https://lnav.org/)

### Intro

XMTP Debug (`xdbg`) is a comprehensive testing and monitoring tool for the XMTP
network. It can generate load, run latency tests, and operate as a continuous
monitoring daemon via Docker.

### Commands

| Command | Description |
| --- | --- |
| `generate` | Create identities, groups, and messages on the network |
| `test` | Run latency and durable-stream validation scenarios |
| `inspect` | Inspect an inbox's groups, messages, or identity state |
| `query` | Query backend APIs (identity updates, key packages, commit logs) |
| `info` | Show information about local generated state |
| `export` | Export generated identities/groups to JSON |

### Examples

`--url` is required. Run `just backend up` before the local examples.

---

#### Generate

##### Generate 1000 random identities

```text
dev/nix-shell 'cargo xdbg --url http://127.0.0.1:5050 generate --entity identity --amount 1000'
```

##### Generate 100 random groups, inviting 50 random identities to each

```text
dev/nix-shell 'cargo xdbg --url http://127.0.0.1:5050 generate --entity group --amount 100 --invite 50'
```

##### Generate 20 messages

```text
dev/nix-shell 'cargo xdbg --url http://127.0.0.1:5050 generate --entity message --amount 20'
```

##### Generate 20 messages in a loop every 500 milliseconds

```text
dev/nix-shell 'cargo xdbg --url http://127.0.0.1:5050 generate --entity message --amount 20 --interval 500 --loop'
```

##### Generate 20 messages in a loop every 500 milliseconds, raising maximum size of each message

```text
dev/nix-shell 'cargo xdbg --url http://127.0.0.1:5050 generate --entity message --amount 20 --interval 500 --loop --max-message-size 1000'
```

#### Inspect

##### Inspect an InboxId

```text
dev/nix-shell 'cargo xdbg --url http://127.0.0.1:5050 inspect 1d8ec149b5670b1df0bbea0b9f2f0ba513eef805a02eafb37df3587fc23d89fe groups'
```

#### Info

##### Show information about local generated state

```text
dev/nix-shell 'cargo xdbg --url http://127.0.0.1:5050 info'
```

#### Export Identities to JSON

```text
dev/nix-shell 'cargo xdbg --url http://127.0.0.1:5050 export --entity identity | jq > identities.json'
```

#### Query

##### Get information about identity updates for an inbox id

```text
dev/nix-shell 'cargo xdbg --url http://127.0.0.1:5050 query identity 01a43cdd27b196472687262ed5783006eabc7c26db9e09630bc5004b8fc689dc'
```

##### Get information about key packages for multiple inboxes

```text
dev/nix-shell 'cargo xdbg --url http://127.0.0.1:5050 query fetch-key-packages d43e83f66ad7dbbe87add243806999d608bb0b6f7b88ba5efcaabdb532728309 01a43cdd27b196472687262ed5783006eabc7c26db9e09630bc5004b8fc689dc'
```

##### Get information about the query log for multiple groups (optionally skipping unspecified commits)

```text
dev/nix-shell 'cargo xdbg --url http://127.0.0.1:5050 query batch-query-commit-log e261da64fd225fc90034631945259cdf 0bc5493237d3399dddd3735a049ea237 --skip-unspecified'
```

#### Test

##### Measure message delivery latency (sender → receiver)

```text
dev/nix-shell 'cargo xdbg --url http://127.0.0.1:5050 test message-visibility --iterations 5'
```

##### Measure group sync latency after 20 messages

```text
dev/nix-shell 'cargo xdbg --url http://127.0.0.1:5050 test group-sync --iterations 3 --message-count 20'
```

##### Check durable streams with three independent peers

```text
dev/nix-shell 'cargo run -p xdbg -- --url http://127.0.0.1:5050 test durable-streams --iterations 3'
```

Each iteration creates three disk-backed peers on the local backend. It checks
concurrent valid commits, an invalid supported MLS envelope, stream reconnect,
an actual TCP outage for the third peer, and a clean client close/reopen from the
same database. The TCP outage uses a private loopback proxy. The other peers
commit and send while the proxy rejects connections. The same open reader must
recover after the proxy resumes. This does not test process crashes.
After each phase, all peers must
have the same epoch and epoch authenticator. Every peer must also decrypt one
new message from each sender exactly once.

The command fails on the first failed check or timeout. It retains its databases
and prints their directory. Use `--state-directory PATH` to choose the parent
directory. The command creates a new run directory and does not overwrite an
earlier run. It does not use existing xdbg identities or change backend
configuration. It requires an HTTP backend URL and publishes new test identities,
groups, and messages.

---

## Docker Image

A unified Docker image is published as `ghcr.io/xmtp/xdbg`. It packages the
`xdbg` binary with a monitoring entrypoint for continuous environment health
checks.

### Building locally

```bash
docker build -t xdbg:local -f apps/xmtp_debug/docker/Dockerfile .
```

### Running as a one-off CLI

```bash
docker run --rm xdbg:local xdbg --url http://backend:5050 generate --entity identity --amount 5
```

### Running as a monitoring daemon

The default entrypoint (`docker/entrypoint.sh`) loops indefinitely: it
generates identities, groups, and messages, then sleeps before repeating. This
is designed for ECS/Fargate deployment as a continuous health probe.

```bash
docker run -d \
  -e XMTP_BACKEND_URL=http://backend:5050 \
  -e XDBG_LOOP_PAUSE=300 \
  -e PUSHGATEWAY_URL=http://pushgateway:9091 \
  ghcr.io/xmtp/xdbg
```

| Variable | Default | Description |
| --- | --- | --- |
| `XMTP_BACKEND_URL` | Required | Self-hosted backend URL for the monitor |
| `XDBG_LOOP_PAUSE` | `300` | Seconds to sleep between monitoring loop iterations |
| `PUSHGATEWAY_URL` | _(unset)_ | Prometheus PushGateway URL. If unset, metrics are silently disabled |
| `XDBG_DB_ROOT` | _(unset)_ | Override the default data directory for xdbg state |

---

## Prometheus Metrics

Metrics are **opt-in**: they activate only when `PUSHGATEWAY_URL` is set in the
environment. Without it, all metric calls are silent no-ops.

### Emitted metrics

| Metric | Type | Labels | Description |
| --- | --- | --- | --- |
| `xdbg_operation_latency_seconds` | Gauge | `operation_type` | Latency of the most recent operation |
| `xdbg_group_add_member_count` | Gauge | `operation_type` | Number of members added to a group |
| `xdbg_messages_sent_total` | Counter | `operation_type` | Cumulative count of messages sent |

Metrics are pushed to the PushGateway after each timed operation under job names
`xdbg_debug` (generate commands) and `xdbg_test` (test scenarios).

### CSV metric output

In addition to Prometheus, every timed operation prints a CSV line to stdout:

```text
kind,name,value,timestamp_ms,label1=v1;label2=v2
```

This can be filtered with standard Unix tools or piped into a log aggregation
pipeline.

---

## Future Work

See [The Tracking Issue](https://github.com/xmtp/libxmtp/issues/1310) for
in-progress features & future work.
