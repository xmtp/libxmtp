# XMTP Debug

### Debug your app on local & dev XMTP environments

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
| `test` | Run e2e latency test scenarios |
| `inspect` | Inspect an inbox's groups, messages, or identity state |
| `query` | Query backend APIs (identity updates, key packages, commit logs) |
| `info` | Show information about local generated state |
| `export` | Export generated identities/groups to JSON |

### Examples

---

#### Generate

##### Generate 1000 random identities

```
cargo xdbg generate --entity identity --amount 1000
```

##### Generate 100 random groups, inviting 50 random identities to each

```
cargo xdbg generate --entity group --amount 100 --invite 50
```

##### Generate 20 messages

```
cargo xdbg generate --entity message --amount 20
```

##### Generate 20 messages in a loop every 500 milliseconds

```
cargo xdbg generate --entity message --amount 20 --interval 500 --loop
```

##### Generate 20 messages in a loop every 500 milliseconds, raising maximum size of each message

```
cargo xdbg generate --entity message --amount 20 --interval 500 --loop --max-message-size 1000
```

#### Inspect

##### Inspect an InboxId

```
cargo xdbg inspect 1d8ec149b5670b1df0bbea0b9f2f0ba513eef805a02eafb37df3587fc23d89fe groups
```

#### Info

##### Show information about local generated state

```
cargo xdbg info
```

#### Export Identities to JSON

```
cargo xdbg export --entity identity | jq > identities.json
```

#### Query

##### Get information about identity updates for an inbox id

```
cargo xdbg query identity 01a43cdd27b196472687262ed5783006eabc7c26db9e09630bc5004b8fc689dc
```

##### Get information about key packages for multiple inboxes

```
cargo xdbg query fetch-key-packages d43e83f66ad7dbbe87add243806999d608bb0b6f7b88ba5efcaabdb532728309 01a43cdd27b196472687262ed5783006eabc7c26db9e09630bc5004b8fc689dc
```

##### Get information about the query log for multiple groups (optionally skipping unspecified commits)

```
cargo xdbg --backend dev query batch-query-commit-log e261da64fd225fc90034631945259cdf 0bc5493237d3399dddd3735a049ea237 --skip-unspecified
```

#### Test

##### Measure message delivery latency (sender → receiver)

```
cargo xdbg -d -b dev test message-visibility --iterations 5
```

##### Measure group sync latency after 20 messages

```
cargo xdbg -d -b dev test group-sync --iterations 3 --message-count 20
```

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
docker run --rm xdbg:local xdbg -d -b dev generate --entity identity --amount 5
```

### Running as a monitoring daemon

The default entrypoint (`docker/entrypoint.sh`) loops indefinitely: it
generates identities, groups, and messages, then sleeps before repeating. This
is designed for ECS/Fargate deployment as a continuous health probe.

```bash
docker run -d \
  -e WORKSPACE=testnet-dev \
  -e XDBG_LOOP_PAUSE=300 \
  -e PUSHGATEWAY_URL=http://pushgateway:9091 \
  ghcr.io/xmtp/xdbg
```

| Variable | Default | Description |
| --- | --- | --- |
| `WORKSPACE` | _(empty → local)_ | Target environment: `testnet`, `testnet-dev`, `testnet-staging` |
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

```
kind,name,value,timestamp_ms,label1=v1;label2=v2
```

This can be filtered with standard Unix tools or piped into a log aggregation
pipeline.

---

## Future Work

See [The Tracking Issue](https://github.com/xmtp/libxmtp/issues/1310) for
in-progress features & future work.
