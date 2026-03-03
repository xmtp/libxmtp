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

XMTP Debug is a comprehensive testing tool for the XMTP network. It may be used
to inspect

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

#### Test scenarios

`xdbg test` runs end-to-end latency measurements using throwaway identities. Each
iteration creates fresh users and groups so results are network-only.

```
# Measure message visibility latency (send → stream receive)
cargo xdbg -d -b staging test message-visibility

# Measure group sync latency over 5 iterations, 20 messages each
cargo xdbg -d -b staging test group-sync --iterations 5 --message-count 20
```

---

## Monitor mode

`xdbg` doubles as the ECS-deployed `ghcr.io/xmtp/d14n-client-monitor` daemon.
Activate monitor mode by setting `PUSHGATEWAY_URL`; without it every metric call is
a silent no-op — safe for local developer usage.

### Environment variables

| Variable | Consumed by | Default | Description |
|---|---|---|---|
| `PUSHGATEWAY_URL` | xdbg binary | _(none)_ | Activates Prometheus push; absent = silent no-op |
| `XDBG_LOOP_PAUSE` | xdbg binary + runner scripts | `300` | Seconds to sleep between generate iterations |
| `XDBG_DB_ROOT` | xdbg binary | XDG data dir | Override the DB root directory (used by `concurrent-runner.sh` for daemon isolation) |
| `WORKSPACE` | `newrunner.sh`, `concurrent-runner.sh` | _(none)_ | Maps to backend: `testnet`→`production`, `testnet-dev`→`dev`, `testnet-staging`→`staging`, anything else→`local` |
| `WEB_HEALTHCHECK_ENDPOINTS` | `web_healthcheck.sh` | _(none)_ | Comma-separated URLs to probe; non-200 responses are logged as errors |

### Local observability stack

```bash
cd xmtp_debug/metrics
docker-compose up -d       # starts PushGateway :9091, Prometheus :9090, Grafana :3000
cd ../..

PUSHGATEWAY_URL=http://localhost:9091 \
  cargo xdbg -d -b staging generate --entity identity --amount 1

# Verify metrics
curl -s http://localhost:9091/metrics | grep xdbg_

cd xmtp_debug/metrics && docker-compose down
```

Grafana default credentials: `admin` / `admin`.

### Metric output

Two parallel channels are active when `PUSHGATEWAY_URL` is set:

**Prometheus PushGateway** — gauges/counters pushed after each operation:

| Metric | Type | Description |
|---|---|---|
| `xdbg_operation_latency_seconds` | GaugeVec | Latency in **seconds**, labelled by `operation_type` |
| `xdbg_group_add_member_count` | GaugeVec | Member count per `add_members` call |
| `xdbg_messages_sent_total` | CounterVec | Cumulative sends, labelled by `operation_type` |

**CSV stdout** — one line per event, format `kind,name,value,timestamp_ms,label=v;…`:

```bash
# Show all latency events
xdbg ... | grep '^latency_seconds'

# Count throughput events
xdbg ... | awk -F, '$1=="throughput_events"{count++} END{print count}'
```

### Docker

```bash
# Build from repo root (BuildKit required for layer caching)
docker buildx build \
  -f xmtp_debug/docker/Dockerfile \
  -t ghcr.io/xmtp/d14n-client-monitor:local \
  .

# Run with ECS-equivalent environment
docker run --rm \
  -e PUSHGATEWAY_URL=http://your-pushgateway:9091 \
  -e WORKSPACE=testnet-staging \
  -e XDBG_LOOP_PAUSE=300 \
  -e WEB_HEALTHCHECK_ENDPOINTS=https://your-service/health \
  ghcr.io/xmtp/d14n-client-monitor:local
```

The container entrypoint is `newrunner.sh`. It loops continuously:
reset state → generate identities → create groups → send messages → run health checks → sleep.

CI publishes `ghcr.io/xmtp/d14n-client-monitor:{short-sha}` and `:latest` on every merge
to `main` that touches `xmtp_debug/`, `xmtp_api/`, or related crates.

---

## Future Work

See [The Tracking Issue](https://github.com/xmtp/libxmtp/issues/1310) for
in-progress features & future work.
