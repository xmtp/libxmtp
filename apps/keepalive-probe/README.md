# keepalive-probe

Throwaway diagnostic for [`herald-lite#70`](https://github.com/xmtplabs/herald-lite/issues/70) — "UNAVAILABLE status GRPC errors."

herald instances regularly log `hyper::Error(Http2, KeepAliveTimedOut)` → gRPC `UNAVAILABLE` against the dev backend, then retry. We ruled out the NLB, server, CPU, and memory as causes. This binary isolates the **transport keepalive** in the simplest possible form: open *N* idle HTTP/2 connections to the gRPC endpoint, hold them doing nothing, and measure how long each survives — and whether tweaking the keepalive config changes that.

## What it does

For each of `--count` connections, one task: TCP → TLS (rustls, ALPN `h2`) → `hyper` HTTP/2 handshake with the configured keepalive, then it **holds the `SendRequest` handle and sends nothing**, awaiting the connection driver. The driver future resolves only when the connection actually dies, so we get an exact lifetime and reason, pinned to that one connection (no tonic-style reconnect masking). Setup failures are recorded as outcomes, never panics — one bad connection can't kill the run.

## Why these defaults

The defaults mirror libxmtp's real channel config in
[`crates/xmtp_api_grpc/src/grpc_client/native.rs`](../../crates/xmtp_api_grpc/src/grpc_client/native.rs)
(`apply_channel_options`), so a bare run reproduces herald's transport behavior:

| flag | default | libxmtp source |
|------|---------|----------------|
| `--ka-interval` | `16s` | `http2_keep_alive_interval` |
| `--ka-timeout` | `10s` | `keep_alive_timeout` ← the #70 knob |
| `--ka-while-idle` | `true` | `keep_alive_while_idle` |
| `--connect-timeout` | `10s` | `connect_timeout` |
| `--endpoint` | `https://grpc.dev.xmtp.network:443` | node-sdk `ApiUrls.dev` |

## Usage

```sh
# single connection, default (libxmtp) config — smoke test
cargo run -p keepalive-probe -- --count 1 --duration 120s

# the experiment: 1000 idle connections for 30 min
cargo run -p keepalive-probe -- --count 1000 --duration 1800s --stagger 5ms

# sweep the #70 knob: does a longer keepalive timeout stop the deaths?
cargo run -p keepalive-probe -- --count 200 --duration 1800s --ka-timeout 30s

# control: no idle pings at all
cargo run -p keepalive-probe -- --count 200 --duration 1800s --ka-while-idle false
```

Stops at `--duration` or on Ctrl-C; either way it prints a summary. `RUST_LOG=debug` for more detail.

## Subscribe mode (real V3 streams) — `--subscribe-group`

Instead of an idle connection, hold a real `MlsApi/SubscribeGroupMessages` stream per connection and log every payload + every disconnect. This is the herald-shaped test (herald holds subscribe streams), and it discriminates a *one-directional black hole* (return path drops → stream disconnects, client-side, no server error) from an idle-transport issue. Subscribe is unauthenticated — the V3 backend doesn't gate it.

**Local first** (point at a local node-go, make a group with `xdbg`):
```sh
# 1. run node-go locally (V3 gRPC on http://localhost:5556)
# 2. make a group + grab its id (hex)
cargo run -p xmtp_debug -- --backend local generate group   # then `inspect` to get the group id
# 3. hold a few streams to it and watch for disconnects
cargo run -p keepalive-probe -- \
  --endpoint http://localhost:5556 --subscribe-group <hex-group-id> --count 4 --duration 1h
# 4. send a message to the group with xdbg → probe logs "payload received"
# 5. kill / pause the local node → probe logs "DISCONNECTED: ..." with the reason + lifetime
```

Against dev: `--endpoint https://grpc.dev.xmtp.network:443 --subscribe-group <id>`. `http://` = plaintext h2 (local), `https://` = TLS. The keepalive flags apply to the stream's channel just like idle mode.

A `DISCONNECTED` line is the disconnect notice; `payloads received` in the summary confirms the streams were live.

## Output

Per-connection lines as they die, then a summary: established / died / closed / survived / setup-errors, the lifetime distribution (min/p50/p95/max) of the connections that **died**, and a death-reason histogram (keepalive timeout / GOAWAY / reset / …).

## Reading the result

- **Idle connections die on default config** → keepalive-on-an-idle-path is the mechanism; confirm `--ka-timeout 30s` (or `--ka-while-idle false`) makes them survive, and that's the fix to push into libxmtp.
- **Idle connections survive but herald's don't** → the death is stream/activity-specific, not pure transport; escalate to holding a real `Subscribe` stream.

## Scope

Throwaway. No XMTP protos, no auth (the gRPC layer has none), no real RPCs, no reconnect logic, no metrics export. Not in `default-members`, so it never builds in normal CI; `cargo run -p keepalive-probe` only.
