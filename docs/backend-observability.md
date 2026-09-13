# Backend observability

The local stack contains `db`, `replica`, `backend`, `anvil`, `toxiproxy`,
`tempo`, `prometheus`, and `grafana`. All local service data is temporary.

```sh
just backend up
just backend observe-check
just backend logs backend tempo prometheus
just backend down
```

The check creates two clients, one group, and ten messages with xdbg. It uses a
new temporary client database. It checks metrics, one trace with both services,
and Grafana. Each assertion has a 60-second deadline. Failures name the assertion
and exit non-zero. The amd64 backend image CI job runs the same check.

## Configuration

Set these keys in the backend TOML file. Unknown keys fail startup.

| Key | Default | Use |
| --- | --- | --- |
| `telemetry.metrics_listen` | `"0.0.0.0:9464"` | Prometheus listener. An empty string disables only the listener. |
| `telemetry.otlp_endpoint` | absent | OTLP gRPC HTTP(S) URL. If absent, use `OTEL_EXPORTER_OTLP_ENDPOINT`. With neither set, trace export is off. |
| `telemetry.otlp_logs` | `false` | Export correlated logs when an endpoint is set. |
| `telemetry.service_name` | `"xmtp-backend"` | Exported service name. Must not be empty. |
| `telemetry.sample_ratio` | `1.0` | Root trace sample ratio, from zero through one. Must be finite. Parent sampling is preserved. |
| `telemetry.resource_attributes` | `{}` | Extra string attributes. `service.name` and `service.version` are reserved. |
| `server.log_format` | `"text"` | Stdout format: `text` or `json`. |

The explicit endpoint takes precedence over the environment fallback. An invalid
resolved endpoint fails startup. An unreachable collector does not stop serving.
The stack sets `OTEL_EXPORTER_OTLP_ENDPOINT=http://tempo:4317` for the backend.
Tempo accepts traces; use a log collector if you enable OTLP logs.

`resource_attributes` are exported verbatim. Secrets never belong there.
The pipeline adds `service.version` from the build. Trace propagation uses W3C
`traceparent` and `tracestate`. Do not add request data to labels or span fields.

`server.log_level` defaults to `info`; `--log-level` overrides it.
`server.request_logger` defaults to `true`. These stdout controls do not suppress
operation metrics or trace export. Shutdown bounds export flush to five seconds
after request drain.

## Metric catalogue

`apps/backend/src/telemetry.rs` defines `CATALOGUE`. The table preserves its names,
types, and help text. The catalogue test checks both this table and spec 002.
“Automatic” means transport or shared logging collection. “Explicit” means a
backend recording call. All families below are described by the backend.

RPC means `grpc_type`, `grpc_service`, and `grpc_method`. Route labels use a fixed
route table. Unknown routes use `unknown`. Prometheus also adds `job` and
`instance`. Histogram buckets add `le`; histogram families expose `_bucket`,
`_sum`, and `_count`. No label contains a user, topic, or payload.

Backend metrics are in-process counters, gauges, and histograms. They do **not**
depend on trace sampling. Tempo-derived `traces_spanmetrics_*` client counts and
bucket populations **do** scale with `sample_ratio`. Their quantiles and error
ratios are estimates from the sampled population, not values multiplied by that
ratio. Collector loss can reduce that population further.

| Metric | Type | Help | Labels | Source | Owner | Failure identified | Bound to |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `xmtp_operation_duration_seconds` | histogram | Operation span duration by operation and status. | operation, status | automatic | xmtp_logging | Slow operation | Span creation to close; status follows ERROR events |
| `xmtp_telemetry_export_failures_total` | counter | Failed telemetry export batches. | none | automatic | xmtp_logging | OTLP export failure | Failed export batches; not lost span count |
| `grpc_server_started_total` | counter | gRPC requests started. | RPC | automatic | backend transport | Traffic loss | Request admission; excludes health and preflight |
| `grpc_server_handled_total` | counter | gRPC requests completed. | RPC, grpc_code | automatic | backend transport | RPC failures | Response body completion, failure, or cancellation |
| `grpc_server_handling_seconds` | histogram | gRPC response body lifetime. | RPC, grpc_code | automatic | backend transport | Slow response | Response body lifetime; streams can be long |
| `grpc_server_in_flight` | gauge | gRPC requests in flight. | RPC | automatic | backend transport | Too many active requests | Admitted requests with unfinished response bodies |
| `grpc_server_request_bytes_total` | counter | Consumed gRPC request body bytes. | RPC | automatic | backend transport | Large requests | Consumed body bytes, including framing; excludes headers |
| `grpc_server_response_bytes_total` | counter | Emitted gRPC response body bytes. | RPC | automatic | backend transport | Large responses | Emitted body bytes; not client receipt |
| `xmtp_db_released_open_transactions_total` | counter | Open transactions rolled back on pool release. | none | explicit | backend pool | Transaction left open | Rollback on pool release |
| `xmtp_db_errors_total` | counter | Database errors mapped to RPC statuses. | kind | explicit | backend error mapping | Database failure | timeout, invariant, connection, other; RPC mapping only |
| `xmtp_publish_envelopes_total` | counter | Publish input positions by response origin. | outcome | explicit | backend publish | Rejected or repeated input | Input positions by stored/duplicate/rejected response origin |
| `xmtp_publish_rejections_total` | counter | Rejected publishes by validation reason. | reason | explicit | backend validation | Invalid publish | Bounded publish validation reason codes |
| `xmtp_scw_verifications_total` | counter | Smart contract wallet verification results. | result | explicit | backend SCW verifier | Chain verification failure | valid/invalid/error verifier results; includes cache behavior |
| `xmtp_stream_sessions` | gauge | Registered stream sessions. | kind | explicit | backend registry | Session growth | Registered bidi/static sessions |
| `xmtp_stream_topics_registered` | gauge | Registered stream topic interests. | none | explicit | backend registry | Interest growth | Registered topic interests across sessions |
| `xmtp_stream_frames_sent_total` | counter | Stream frames admitted to the outbound queue. | frame | explicit | backend stream | Missing outbound activity | started/applied/messages/keepalive/ping/pong/update queue admission |
| `xmtp_stream_frames_received_total` | counter | Stream frames received. | frame | explicit | backend stream | Missing inbound activity | Received stream frames by bounded frame kind |
| `xmtp_stream_envelopes_sent_total` | counter | Stream envelopes admitted by delivery phase. | phase | explicit | backend stream | Missing delivery activity | catch_up/live queue admission; not receipt |
| `xmtp_stream_updates_total` | counter | Stream interest update results. | outcome | explicit | backend stream | Invalid or rate-limited update | applied/invalid/rate_limited interest updates |
| `xmtp_stream_ended_total` | counter | Stream sessions ended by reason. | reason | explicit | backend stream | Stream failure | One termination per registered session; bounded StreamEnd reason |
| `xmtp_stream_outbound_wait_seconds` | histogram | Time waiting for outbound capacity. | none | explicit | backend stream | Outbound capacity pressure | Waits only when capacity is unavailable |
| `xmtp_stream_fetch_wait_seconds` | histogram | Time waiting for a stream fetch permit. | none | explicit | backend stream | Fetch queue delay | Permit acquisition lifetime, including cancellation |
| `xmtp_tailer_polls_total` | counter | Tailer poll results. | result | explicit | backend tailer | Tailer query failure | ok/error poll results |
| `xmtp_tailer_rows_total` | counter | Tailer rows read by source. | source | explicit | backend tailer | Tailer progress loss | forward/gap rows read; not unique delivered rows |
| `xmtp_tailer_gap_ranges` | gauge | Unresolved tailer gap ranges. | none | explicit | backend tailer | Unresolved allocation gaps | Range count; not missing ID count |
| `xmtp_tailer_restarts_total` | counter | Tailer recovery generations started. | none | explicit | backend tailer | Repeated recovery | Recovery generations, including initial startup |
| `xmtp_tailer_ready` | gauge | Whether stream recovery is ready. | none | explicit | backend tailer | Recovery incomplete | Current recovery readiness |
| `xmtp_boundary_advances_total` | counter | Allocation boundary advance results. | result | explicit | backend boundary worker | Allocation boundary starvation | ok/lock_timeout/error advance results |
| `xmtp_backend_ready` | gauge | Whether the backend reports Serving. | none | explicit | backend health | Backend not serving | Same state change as gRPC Serving |
| `xmtp_backend_info` | gauge | Backend build version. | version | explicit | backend startup | Wrong build deployed | Build version with value one |

Error-only families can be absent until their first event.

## Span names

These are the backend operation names from its source. Shared validation can
also produce spans from shared crates. Two database helpers use `db.history`.

- `db.acquire`, `db.advance`, `db.apply_projection`, `db.boundary`, `db.clock_ns`, `db.commit_publish`, `db.connect`, `db.dedicated_read`, `db.find_duplicates`, `db.forward`, `db.gaps`, `db.heads`, `db.history`, `db.inbox_ids`, `db.newest_envelopes`, `db.newest_metadata`, `db.payloads`, `db.query`, `db.release`, `db.snapshot`, `db.snapshot_connection`.
- `publish.locks`, `publish.parse_publish`, `publish.validate_publish`.
- `tailer.bootstrap`, `tailer.poll`.
- `stream.fetch`, `stream.update`.
- `scw.verify`.
- `rpc.get_inbox_ids`, `rpc.publish`, `rpc.query`, `rpc.query_newest`, `rpc.verify_smart_contract_wallet_signatures`.

Transport spans have the tracing name `grpc_request`. Their exported name is
`<service>/<method>` from the fixed route table. This includes Query, QueryNewest,
Publish, Subscribe, SubscribeStatic, GetInboxIds,
VerifySmartContractWalletSignatures, and health Check, Watch, and List.
Unknown routes export `unknown/unknown`. Transport spans cover response lifetime.

## Failure modes

Use several signals for a diagnosis. A zero value does not prove that the whole
system works. Readiness does not prove that a client received a message.

| Failure | Signal | Limit |
| --- | --- | --- |
| Backend unavailable | `up{job="backend"}`, `xmtp_backend_ready` | Scrape reachability and serving state are different checks. |
| RPC failure or slow request | `grpc_server_handled_total`, `grpc_server_handling_seconds` | Long streams measure session lifetime. |
| Database connection, timeout, or invariant failure | `xmtp_db_errors_total{kind="connection"}`, `{kind="timeout"}`, `{kind="invariant"}` | Counts errors mapped to RPC statuses only. |
| Transaction left open | `xmtp_db_released_open_transactions_total` | Counts release-time rollback only. |
| Slow publish lock or commit | `xmtp_operation_duration_seconds{operation="publish.locks"}`, `{operation="db.commit_publish"}` | Elapsed span time includes waits. Use histogram suffixes in queries. |
| Invalid or repeated publish | `xmtp_publish_rejections_total`, `xmtp_publish_envelopes_total` | Rejected response does not prove database rollback. |
| Chain RPC or signature failure | `xmtp_scw_verifications_total`, duration for `scw.verify` | Verifier calls can use cache; they are not a count of network calls. |
| WAL-retention exhaustion | Infrastructure monitoring and PostgreSQL logs | No retained-WAL or replication-slot metric in the backend. |
| Tailer recovery or query failure | `xmtp_tailer_ready`, `xmtp_tailer_restarts_total`, `xmtp_tailer_polls_total` | Initial startup also counts as a generation. |
| Tailer gaps or boundary starvation | `xmtp_tailer_gap_ranges`, `xmtp_boundary_advances_total` | Gap count measures ranges, not lost messages. |
| Stream failure | `xmtp_stream_ended_total` | Inspect the reason; client cancellation can be normal. |
| Stream capacity pressure | `xmtp_stream_outbound_wait_seconds`, `xmtp_stream_fetch_wait_seconds` | Measures capacity wait time, including fetch cancellation. |
| Missing stream traffic | `xmtp_stream_frames_sent_total`, `xmtp_stream_frames_received_total`, `xmtp_stream_envelopes_sent_total` | Outbound counts prove admission only. |
| Telemetry export failure | `xmtp_telemetry_export_failures_total` | Export failures count batches, not all dropped spans. |

### Collection limits

- There is no replica replay-delay metric. A gauge built on
  `pg_last_xact_replay_timestamp()` reports zero when the received and replayed
  WAL positions are equal. A replica that stopped receiving WAL can then appear
  healthy. This timestamp gives the age of the last replayed transaction. It
  does not give the age of the oldest pending transaction. Use a replication
  health check outside the backend.
- The backend exposes no sampled gauges. Pool, queue, replica-progress, and
  process/runtime observation belong to infrastructure monitoring outside the
  backend. Periodic observations can miss events shorter than the interval.
  Gauges that support paging alerts must not be approximations.
- `xmtp_stream_frames_sent_total` and `xmtp_stream_envelopes_sent_total` count
  queue admission. `grpc_server_response_bytes_total` counts emitted bytes.
  None proves receipt. Cut redundant traffic panels before adding more counters.
- `xmtp_operation_duration_seconds` uses ERROR events for status. Cancellation
  without an ERROR event is `ok`. Treat duration as reliable elapsed time, but
  do not use its status label as an exact operation success rate.
- `xmtp_scw_verifications_total` and `scw.verify` duration include verifier cache
  behavior. The “Chain RPC p99” panel is not pure network latency.
- `traces_spanmetrics_*` client metrics depend on sampling and successful export.
  They are unsuitable for exact traffic or error counts.

## Dashboard

Open <http://127.0.0.1:3000> and select **XMTP Backend**. Local anonymous users have
Admin access. The dashboard source is
`dev/docker/grafana/dashboards/backend.json`. The table lists every data panel;
row headings only group panels. Queries are copied from that source.

| Panel | Query |
| --- | --- |
| Version (A) | `xmtp_backend_info` |
| Backend ready (A) | `xmtp_backend_ready` |
| Tailer ready (A) | `xmtp_tailer_ready` |
| Backend scrape (A) | `up{job="backend"}` |
| Requests/s by method (A) | `sum by (grpc_method) (rate(grpc_server_started_total[1m]))` |
| In flight (A) | `sum by (grpc_type) (grpc_server_in_flight)` |
| Bytes in/out (A) | `sum(rate(grpc_server_request_bytes_total[1m]))` |
| Bytes in/out (B) | `sum(rate(grpc_server_response_bytes_total[1m]))` |
| Error ratio (A) | `sum(rate(grpc_server_handled_total{grpc_code!~"OK\|InvalidArgument\|NotFound\|Aborted"}[5m])) / sum(rate(grpc_server_started_total[5m]))` |
| Errors by code and method (A) | `sum by (grpc_method, grpc_code) (rate(grpc_server_handled_total{grpc_code!="OK"}[5m]))` |
| Publish rejections (A) | `sum by (reason) (rate(xmtp_publish_rejections_total[5m]))` |
| Telemetry failures (A) | `rate(xmtp_telemetry_export_failures_total[5m])` |
| Unary p50/p99 by method (A) | `histogram_quantile(0.5, sum by (le, grpc_method) (rate(grpc_server_handling_seconds_bucket{grpc_code="OK",grpc_type="unary"}[5m])))` |
| Unary p50/p99 by method (B) | `histogram_quantile(0.99, sum by (le, grpc_method) (rate(grpc_server_handling_seconds_bucket{grpc_code="OK",grpc_type="unary"}[5m])))` |
| Stage p99 (A) | `histogram_quantile(0.99, sum by (le, operation) (rate(xmtp_operation_duration_seconds_bucket{operation=~"publish\\..*\|db\\..*"}[5m])))` |
| Lock wait p99 (A) | `histogram_quantile(0.99, sum by (le) (rate(xmtp_operation_duration_seconds_bucket{operation="publish.locks"}[5m])))` |
| Sessions (A) | `sum by (kind) (xmtp_stream_sessions)` |
| Topics registered (A) | `xmtp_stream_topics_registered` |
| Envelopes/s (A) | `sum by (phase) (rate(xmtp_stream_envelopes_sent_total[1m]))` |
| Frames/s (A) | `sum by (frame) (rate(xmtp_stream_frames_sent_total[1m]))` |
| Frames/s (B) | `sum by (frame) (rate(xmtp_stream_frames_received_total[1m]))` |
| Ended by reason (A) | `sum by (reason) (rate(xmtp_stream_ended_total[5m]))` |
| Outbound wait p99 (A) | `histogram_quantile(0.99, sum by (le) (rate(xmtp_stream_outbound_wait_seconds_bucket[5m])))` |
| Poll p99 (A) | `histogram_quantile(0.99, sum by (le) (rate(xmtp_operation_duration_seconds_bucket{operation="tailer.poll"}[5m])))` |
| Rows/s by source (A) | `sum by (source) (rate(xmtp_tailer_rows_total[1m]))` |
| Gap ranges (A) | `xmtp_tailer_gap_ranges` |
| Restarts (A) | `increase(xmtp_tailer_restarts_total[1h])` |
| Boundary results (A) | `sum by (result) (rate(xmtp_boundary_advances_total[5m]))` |
| Envelopes by outcome (A) | `sum by (outcome) (rate(xmtp_publish_envelopes_total[1m]))` |
| SCW by result (A) | `sum by (result) (rate(xmtp_scw_verifications_total[5m]))` |
| Chain RPC p99 (A) | `histogram_quantile(0.99, sum by (le) (rate(xmtp_operation_duration_seconds_bucket{operation="scw.verify"}[5m])))` |
| Client operation p99 (A) | `histogram_quantile(0.99, sum by (le, span_name) (rate(traces_spanmetrics_latency_bucket{service="libxmtp"}[5m])))` |
| Client error ratio (A) | `sum(rate(traces_spanmetrics_calls_total{service="libxmtp",status_code="STATUS_CODE_ERROR"}[5m])) / sum(rate(traces_spanmetrics_calls_total{service="libxmtp"}[5m]))` |
| Service graph (A) | `Tempo serviceMap query` |

## Alerts

Prometheus loads `dev/docker/prometheus/alerts.yml`. These thresholds are starting
points. Tune them for traffic, capacity, and the cost of a false alarm.
“No hold” means the rule has no `for` duration. Missing series do not evaluate as
zero, so an unused error counter can leave a ratio absent. The dashboard error
ratio includes more status codes than the HighErrorRate alert.

| Rule | Expression and threshold | Hold | Severity |
| --- | --- | --- | --- |
| BackendDown | `up{job="backend"} == 0` | 1m | page |
| BackendNotReady | `xmtp_backend_ready == 0` | 2m | page |
| TailerNotReady | `xmtp_tailer_ready == 0` | 1m | page |
| HighErrorRate | `sum(rate(grpc_server_handled_total{grpc_code=~"Unavailable\|Internal\|DeadlineExceeded\|Unknown"}[5m])) / sum(rate(grpc_server_started_total[5m])) > 0.02` | 5m | page |
| DatabaseUnavailable | `rate(xmtp_db_errors_total{kind="connection"}[5m]) > 0` | 2m | page |
| StorageInvariant | `increase(xmtp_db_errors_total{kind="invariant"}[10m]) > 0` | No hold | page |
| PublishLatencyHigh | `histogram_quantile(0.99, sum by (le) (rate(grpc_server_handling_seconds_bucket{grpc_method="Publish",grpc_code="OK"}[5m]))) > 1` | 10m | warn |
| QueryLatencyHigh | `histogram_quantile(0.99, sum by (le) (rate(grpc_server_handling_seconds_bucket{grpc_method=~"Query\|QueryNewest",grpc_code="OK"}[5m]))) > 0.5` | 10m | warn |
| LockWaitHigh | `histogram_quantile(0.99, sum by (le) (rate(xmtp_operation_duration_seconds_bucket{operation="publish.locks"}[5m]))) > 0.5` | 10m | warn |
| LeakedTransactions | `increase(xmtp_db_released_open_transactions_total[10m]) > 0` | No hold | warn |
| TailerRestarting | `increase(xmtp_tailer_restarts_total[10m]) > 2` | No hold | page |
| GapRangesGrowing | `xmtp_tailer_gap_ranges > 5000` | 5m | warn |
| BarrierStarved | `sum by (instance) (rate(xmtp_boundary_advances_total{result="lock_timeout"}[10m])) > sum by (instance) (rate(xmtp_boundary_advances_total{result="ok"}[10m]))` | No hold | warn |
| StreamsFailing | `sum(rate(xmtp_stream_ended_total{reason=~"backpressure\|capacity\|tailer\|database"}[5m])) > 0.1` | No hold | warn |
| ChainRpcFailing | `sum(rate(xmtp_scw_verifications_total{result="error"}[5m])) / sum(rate(xmtp_scw_verifications_total[5m])) > 0.1` | 5m | warn |
| ExportFailing | `rate(xmtp_telemetry_export_failures_total[5m]) > 0` | 10m | warn |

## Trace a client call

Start the local stack. Keep the backend service name `xmtp-backend` and use
sample ratio `1.0` for this check. The client must reach both port 5050 and the
OTLP gRPC receiver on port 4317. A physical mobile device needs a reachable host
address and receiver binding; its loopback address is not the development host.

### Node SDK

Use the first client created in the process to configure logging. Supply your
normal signer to `Client.create`:

```ts
import { Client, flushTelemetry } from "@xmtp/node-sdk";

const client = await Client.create(signer, {
  backendUrl: "http://127.0.0.1:5050",
  otelEndpoint: "http://127.0.0.1:4317",
  otelServiceName: "libxmtp",
  otelSampleRatio: 1.0,
});
const group = await client.conversations.createGroup([peerInboxId]);
await group.send("trace check");
flushTelemetry();
```

Use an existing registered peer inbox. Keep the process alive until the operation
and flush finish. `flushTelemetry` is process-global.

### Mobile binding

Use the generated mobile binding functions. `enable_otlp_telemetry` initializes
the shared logger on first use. If Sentry owns the telemetry slot, call
`disable_sentry_telemetry` first. Do not install a second subscriber.

1. Await `enable_otlp_telemetry` with `FfiOtlpConfig`: set `endpoint` to the
   reachable OTLP gRPC URL, `service_name` to `libxmtp`, `sample_ratio` to `1.0`,
   and `resource_attributes` to an empty map. Swift and Kotlin use generated
   camel-case names such as `enableOtlpTelemetry` and `sampleRatio`.
2. Create a client with the reachable backend URL. Create a group with another
   registered client. Send a message and await completion.
3. Call `flush_telemetry` (`flushTelemetry` in generated bindings) before app
   background or process exit. Call `disable_otlp_telemetry` when export must stop.

### Find the shared trace

1. In Grafana, open **Explore** and select **Tempo**.
2. Search the recent time range with TraceQL
   `{ resource.service.name = "xmtp-backend" }`.
3. Open a trace for the operation. Expand its resource attributes and spans.
   The same trace must contain `service.name=libxmtp` and
   `service.name=xmtp-backend`, with the backend request below the client call.
4. Inspect the backend `Publish` request and `db.commit_publish` operation.
   If only one service appears, check the endpoint, parent sampling, W3C header
   forwarding, and exporter failures. Wait for Tempo to flush, then search again.

`just backend observe-check` verifies both services in one fetched trace. Finding
two separate traces with one service each is not proof of propagation.
