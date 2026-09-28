---
prefix: LOG
status: draft
---
# Client logging

Client diagnostic logs go to native logging layers or to an optional app sink. The sink is process-wide on native platforms and Node.js, and worker-wide in a browser. A slow or failed sink must not stop client work. The client keeps a fixed number of records that wait for handoff and reports overflow to the app.

## Scope

In scope: client diagnostic log delivery, sink lifetime, queue capacity, drop reporting, failure isolation, and client secret redaction.

Out of scope: audit logging, backend telemetry, and message bodies. AUTH-027 owns redaction in backend logs and does not replace LOG-010.

## Terms

| Term | Meaning |
| --- | --- |
| Sink generation | One installed app sink. Clearing or replacing it ends that generation. |
| Dispatch | The point where the client selects one retained record for its next delivery attempt and captures that generation's pending drop count. The selected record still awaits handoff. |
| Handoff | The point at which the host-side generation check admits a callback for a record. |
| Pending drop count | The number of overflow records not yet cleared by a successful report in that sink generation. |

## 1. Log delivery

The client sends diagnostic records to its logging layers by default. When an app installs a sink, the client retains records for asynchronous handoff in admission order. Dispatch selects one record for its next delivery attempt and captures its pending drop count. The record still waits for host handoff. One callback can run at a time across sink generations. A record waiting for handoff uses one queue slot; a callback that has started does not. A replacement may discard queued records and the pending drop count. There is no final flush when a sink ends.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| LOG-001 | Native default logging | When no app sink is installed, the client MUST send logs to its native logging layers. | Apps must get diagnostics without a callback. |
| LOG-002 | Asynchronous sink order | When a sink is installed, the client MUST hand retained records to it asynchronously in admission order, with at most one active app log callback across sink generations; LOG-011 defines which records replacement discards. | Concurrent callbacks can reorder events and retain a thread per replaced sink. |
| LOG-003 | Queue capacity | While app log records await handoff, the client MUST retain at most 4096 queued records across sink generations, queues, and transport, in addition to the one active callback. | A slow app must not cause unbounded retained logs. |
| LOG-004 | Nonblocking log emission | When recording a log or completing a client operation, the client MUST NOT wait for an app log callback to complete. | A slow sink must not block messaging. |
| LOG-005 | Overflow drops new records | When a sink generation already has 4096 queued records, the client MUST discard each new record for that sink and increment its pending drop count by one. | The app must be able to detect lost diagnostics. |
| LOG-007 | Sink replacement boundary | When clearing or replacing a sink, the client MUST stop old-generation handoffs before the operation returns, without waiting for a callback already handed off. | Waiting can deadlock a sink that replaces itself. |
| LOG-008 | Reentrant sink calls | When a sink callback calls an SDK operation, the client MUST let that operation complete without waiting for the calling callback. | A callback must not wait on itself or a held client lock. |
| LOG-009 | Sink failure isolation | When an app sink throws or rejects a record, the client MUST keep messaging and its configured native logging layers operational. | Diagnostic code must not stop the client. |
| LOG-010 | Client secret redaction | When emitting a log to an app sink or native logging layer, the client MUST omit authentication credential values, private signing keys, and database encryption keys. | An app log destination must not receive secrets used by the client. |
| LOG-011 | Replacement discards queued logs | When clearing or replacing a sink, the client MUST discard that generation's queued records and pending drop count without a final reporting callback. | Sink replacement cannot promise delivery after it stops handoffs. |
| LOG-012 | Old callback result isolation | When a callback from a cleared or replaced generation completes, the client MUST ignore its result for the installed generation's drop count and queue. | A late completion must not erase or add another sink's diagnostics. |
| LOG-013 | Report dispatched drops | While a sink generation remains installed, the client MUST include the pending drop count captured at dispatch in each call's `droppedRecords` field, clear only that reported count on success, and keep it pending on failure. Drops after dispatch MUST remain pending for a later delivery attempt. LOG-011 and LOG-012 govern a cleared or replaced generation. | A failed callback must not erase overflow evidence, and a delayed handoff must not lose later drops. |
