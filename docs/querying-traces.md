# Query local traces

Use Tempo's HTTP API to find and read traces. Use Grafana Explore to view the
same trace. Start with traces when you need the SDK call path, epochs, timing,
or a client/backend boundary. Use persisted state to confirm a fork; a warning
or failed span alone does not prove one.

## Find the correct stack

```sh
dev/nix-shell 'just backend status'
```

Ports differ by worktree. Tempo's HTTP API uses `XMTP_TEMPO_PORT`. SDK export
uses `XMTP_OTLP_GRPC_PORT`, not the HTTP API port. Grafana uses
`XMTP_GRAFANA_PORT`. The recipes load these values through `dev/docker/load-env`.
Check `/ready` on Tempo and `/api/health` on Grafana when a request fails.

Grafana also proxies the same Tempo API. Its local data source UID is `tempo`.
To use Grafana with either Python example below, replace `base` with:

```python
base = ("http://127.0.0.1:" + os.environ["XMTP_GRAFANA_PORT"]
        + "/api/datasources/proxy/uid/tempo")
```

Both routes were checked against the local stack. The local Grafana permits
anonymous access; a remote Grafana can require an authenticated request.

The SDK must install `xmtp_logging` with a telemetry endpoint. A JSON log
subscriber does not export traces. Chaos children export service `xmtp-chaos`,
sample ratio `1.0`, and resource attributes `xmtp.chaos.run` and
`xmtp.chaos.instance`. Other SDK hosts normally use `libxmtp`.

## Search, then fetch one trace

Run this block from the repository root. It returns at most five trace IDs.
Both `start` and `end` are required when you specify a time range; they use
Unix **seconds**. Returned span timestamps use Unix **nanoseconds**.

```sh
dev/nix-shell 'dev/worktree-env && . dev/docker/load-env && python3 - <<"PY"
import json, os, time, urllib.parse, urllib.request

base = "http://127.0.0.1:" + os.environ["XMTP_TEMPO_PORT"]
query = "{ resource.service.name = \"xmtp-chaos\" }"
params = {"q": query, "start": int(time.time()) - 900,
          "end": int(time.time()) + 1, "limit": 5}
url = base + "/api/search?" + urllib.parse.urlencode(params)
with urllib.request.urlopen(url, timeout=15) as response:
    data = response.read(4 * 1024 * 1024 + 1)
    assert len(data) <= 4 * 1024 * 1024, "response cap exceeded"
for trace in json.loads(data).get("traces", []):
    print(trace["traceID"], trace.get("rootTraceName"),
          trace.get("serviceStats"))
PY'
```

Useful replacements for `query`:

```traceql
{ resource.xmtp.chaos.run = "RUN_DIRECTORY_NAME" }
{ resource.xmtp.chaos.run = "RUN_DIRECTORY_NAME" && resource.xmtp.chaos.instance = "2" }
{ resource.xmtp.chaos.run = "RUN_DIRECTORY_NAME" && name = "diagnostic.epoch_mismatch" }
{ resource.xmtp.chaos.run = "RUN_DIRECTORY_NAME" && name = "chaos.stream" }
{ resource.xmtp.chaos.run = "RUN_DIRECTORY_NAME" && resource.xmtp.chaos.instance = "4" && status = error }
{ resource.service.name = "xmtp-chaos" } && { resource.service.name = "xmtp-backend" }
```

Resource attribute names are not span attribute names. Chaos instance values
are strings. The final query finds traces that contain both services. Finding
two separate traces, each with one service, does not prove context propagation.

Fetch a selected ID with `GET /api/traces/TRACE_ID`. This example keeps output
to selected fields and at most 60 spans. Replace `TRACE_ID` first.

```sh
dev/nix-shell 'dev/worktree-env && . dev/docker/load-env && python3 - <<"PY"
import json, os, urllib.request

trace_id = "TRACE_ID"
base = "http://127.0.0.1:" + os.environ["XMTP_TEMPO_PORT"]
with urllib.request.urlopen(base + "/api/traces/" + trace_id, timeout=15) as response:
    data = response.read(4 * 1024 * 1024 + 1)
    assert len(data) <= 4 * 1024 * 1024, "response cap exceeded"
trace = json.loads(data)
def attributes(items):
    return {a["key"]: next(iter(a["value"].values())) for a in items}
fields = {"operation", "group_id", "sequence_id", "message_epoch",
          "current_epoch", "is_commit", "error_code", "rpc.method",
          "generation", "eof", "error_count", "retryable", "lease_waits"}
remaining = 60
for batch in trace.get("batches", trace.get("resourceSpans", [])):
    if remaining == 0:
        break
    resource = attributes(batch.get("resource", {}).get("attributes", []))
    print({k: v for k, v in resource.items()
           if k in {"service.name", "xmtp.chaos.run", "xmtp.chaos.instance"}})
    for scope in batch.get("scopeSpans", batch.get("instrumentationLibrarySpans", [])):
        for span in scope.get("spans", [])[:remaining]:
            values = attributes(span.get("attributes", []))
            print(span["name"], span.get("status", {}).get("code"),
                  {k: v for k, v in values.items() if k in fields})
            remaining -= 1
print("output capped" if remaining == 0 else "selected spans complete")
PY'
```

Tempo returns `batches`; some OTLP JSON uses `resourceSpans`. Scope fields can
also use either name shown above. Attribute values are typed objects. Integers
can arrive as strings, so convert epoch values before comparing them.

## Diagnose without overstating the evidence

- Search a short time range and one run first. If the result reaches `limit`,
  narrow the time range or instance. A capped search is not a complete count.
- Empty search results can reflect export/index delay. Retry within a fixed
  deadline. Check the endpoint, sampling, and shutdown flush before concluding
  that an operation did not run. A forced kill can lose buffered spans.
- Inspect parent/child spans and both service resources to check propagation.
  In Grafana, select the Tempo data source in Explore and open the trace ID.
- For a selected failed span, inspect its `status.message` and `events` after
  checking that those fields contain no private data. A database error can
  explain why an owned stream ended; distinguish EOF from a live stream stall.
- For `diagnostic.epoch_mismatch`, compare `message_epoch` with `current_epoch`.
  A stale concurrent proposal or commit can produce `WrongEpoch` and set
  `maybe_forked`. In the initial traced baseline, all 484 such spans were one
  epoch stale; matching persisted histories showed no divergence.
- Confirm state with `just chaos inspect --forks [--group GROUP_ID]` after the
  run stops. Compare authenticators, membership, metadata, commit history, and
  delivery. Preserve an unset commit-log flag as unknown.
- Report the run, instance, trace ID, sequence, observed cause, and search caps.
  Print selected fields. Do not dump payloads, keys, or whole trace responses.

See [backend observability](backend-observability.md) for exporter setup and
`just backend observe-check`, which checks SDK/backend spans in one trace.
