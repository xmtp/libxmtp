---
prefix: PROC
status: draft
---
# Message processing

A client receives envelopes, processes them in topic order, and delivers stored messages to an app. Receipt, processing, and app acknowledgement are separate progress measures. A connection failure or a crash must not turn one into another.

The backend supplies ordered topic prefixes under API-201, API-240, and API-254. The client preserves received work until it applies or rejects it. GMOD, JOIN, and IDENT own payload validation; this spec owns progress and the effect of a validation result on pending work.

## Scope

In scope: receipt validation; durable receipt and processing positions; rejection and retry; operation targets and completion; capacity; stream recovery; and stored-message delivery with app acknowledgement.

Out of scope: backend wire formats and errors (API); commit and proposal validation (GMOD); Welcome validation (JOIN); identity validation (IDENT); publishing (SEND); fork recovery (FORK); consent states (CONS); and topic format (TOPIC).

| Related | Relation |
| --- | --- |
| API | Owns ordered reads and authoritative envelope metadata. This spec owns client validation, receipt, and recovery. |
| JOIN | Owns Welcome validation, retention deadlines, and the join anchor. This spec owns pending work and completion. |
| GMOD | Owns commit and proposal validation. This spec owns whether a failure advances processing. |
| SEND | Owns outgoing attempts. This spec owns ordered receipt of their echoes. |
| AUTH | Owns credential failures and lockout. This spec owns recovery for other connection failures. |
| EVENT | EVENT-001 reports stored message changes to an app; this spec owns durable receipt and stream delivery. |

## Terms

| Term | Meaning |
| --- | --- |
| Received position | `F(topic)`: the highest sequence id through which the retained topic prefix after the starting anchor is durably pending or already handled. Sequence ids can have gaps. |
| Processed position | `P(topic)`: the highest sequence id through which every group or identity envelope after the starting anchor is applied or terminally rejected. Welcome topics have no `P`; their progress is `F` and the unresolved Welcome ids. |
| Pending envelope | An admitted envelope not yet applied or terminally rejected. |
| Head | The pending envelope with the lowest sequence id on a group or identity topic. |
| Admission | Acceptance of an ordered prefix as durable received work. |
| Target | `H(topic)`: the fixed sequence id through which one operation waits for processing. Absent means target capture has not succeeded; zero means a sampled topic was empty. |
| Terminal rejection | A recorded refusal that completes an envelope without applying it. |
| Held | Pending work that cannot advance processing, including invalid identity history and failures with no safe rejection rule. |
| Scope generation | A distinct revision of one operation's selected topics and discovery obligations. |
| Delivery number | An immutable, increasing local number assigned when a message first becomes deliverable. |
| Delivery position | `D(group)`: the last delivery number acknowledged or excluded by the default consumer's filter; zero before either occurs. |
| Delivery cursor | A database identity and a delivery number, meaning resume strictly after that item. |
| Default consumer | The one message reader per client database that advances `D`. |
| Replay reader | A reader started from an app-supplied delivery cursor, independent of `D`. |
| Acknowledgement | The app's callback returns normally, or the app requests the next iterator item. |
| Recovery episode | The interval from a stream's initial connection or detected failure until sustained network recovery or termination. |

## 1. Durable receipt

`Subscribe`, `SubscribeStatic`, and `Query` supply ordered prefixes. Overlap on reconnect is normal. A batch can start at or below `F`, but not above it: the latter leaves an unfetched part of the log. Numeric gaps do not imply missing envelopes because API-287 makes sequence ids unique across topics.

A target names work to fetch. It does not establish receipt. Push metadata, a publish receipt, a registration target, and a newest-envelope result can all supply targets without supplying the prefix below them. The backend hash is authoritative under API-211; a client re-encoding is not a check of that hash.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| PROC-001 | Validate every received prefix | When the client admits envelopes from an ordered read, it MUST validate the topic, `cursor.sequence_id` under API-286 and `server_ns` between 0 and 9223372036854775807 inclusive, the 32-byte SHA-256 hash shape of API-211, an exclusive start at or below `F`, and strictly increasing per-topic sequence ids across batches from that registration or query. If any check fails, it MUST refuse the batch without changing received work or `F`; otherwise it MUST preserve the backend hash unchanged without comparing it to a local recomputation. | A skipped prefix loses messages; a changed hash breaks envelope identity. |
| PROC-002 | Receipt survives interruption | When the client advances `F`, it MUST durably retain every unhandled envelope through that position without duplicate admission effects, and MUST leave both received work and `F` unchanged if admission fails. Except for a valid join under PROC-010, it MUST NOT advance `F` from a target or past an envelope it has not durably received, and MUST NOT move `F` backwards. | A position without the received work makes a crash lose messages. |
| PROC-036 | Protect retained envelopes | While the client retains pending envelopes, it MUST apply the client database's encryption and access protection to them and MUST NOT log raw payloads, private keys, database keys, or full installation identifiers. | Receipt must not expose data that message storage protects. |

## 2. Ordered processing

Group and identity processing completes a prefix. A held head blocks later envelopes on that topic. Independent topics and independent Welcomes can still progress. Processing a received envelope can produce messages, update group state, or reject input without producing either.

`P` never exceeds `F`. A closed stream can leave `F` above `P`: the client has stored ciphertext that it has not processed. CONF-075 closes streams with network interest when the connection is blocked; it does not erase that work or require the two positions to be equal. JOIN section 7 compares its anchor with `P`, not with `F`.

A valid Welcome starts or resumes the group at its join anchor under JOIN-046. Envelopes below that anchor do not belong to the installed membership; envelopes above it remain work for that membership.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| PROC-005 | Process a topic prefix | When the client processes a group or identity topic, it MUST apply or terminally reject the head before a later envelope, and MUST advance `P` only through that handled prefix, without moving it backwards or above `F`. After a commit removes this installation, it MUST stop processing later group envelopes, retain them, and resume only after a valid rejoin under JOIN section 7. | Out-of-order state changes fork the group. |
| PROC-008 | Effects survive interruption together | After an interrupted processing attempt, the client MUST expose either all of the envelope's effects with its completed processing position or none of them, and MUST apply each envelope's effects at most once across retries and concurrent clients sharing the database. | Partial or repeated effects lose state or duplicate messages. |
| PROC-010 | Preserve the join boundary | When the client installs group state at anchor `A` under JOIN-046, it MUST establish `F` as the greater of its previous value and `A`, discard pending group envelopes at or below `A`, and retain those above `A`, with the installed state. | Discarding work above the anchor loses messages sent while the Welcome was in flight. |

## 3. Rejection and held work

A terminal group rejection needs a complete preceding prefix and the state needed to validate it. An error's retry flag alone proves neither condition. Missing local state is not proof that the input is invalid. Identity processing has a different rule: an invalid update blocks the identity prefix and every parent that requires that state. It is not skipped to produce a later association state. IDENT-070 and IDENT-071 own the exact verified state a parent can use.

The group disposition table below applies only after those prerequisites hold. Commit validation is defined by [RFC 9420 §12.4.2](https://www.rfc-editor.org/rfc/rfc9420.html#section-12.4.2) and GMOD; proposal validation is defined by [RFC 9420 §12.1](https://www.rfc-editor.org/rfc/rfc9420.html#section-12.1) and GMOD-001 through GMOD-003. The application-message window does not authorize an old commit. A commit requires the current epoch; SEND-014 covers rebuilding an own commit that lost its epoch.

| Group input or failure | Disposition |
| --- | --- |
| Supported input fails decoding, topic/group matching, sender authentication, or payload validation | Terminal rejection |
| Authentication fails, generation is out of bound, wire format is wrong, or a message secret was consumed or discarded for forward secrecy | Terminal rejection; local key, ratchet, library, and storage failures remain held |
| Application message from another installation at the current epoch or one of the three retained preceding epochs | Apply if validation succeeds; reject if its epoch secrets were discarded under the retention policy |
| Own application-message echo at the current epoch or one or two epochs earlier | Apply if validation succeeds; an echo three or more epochs earlier is stale |
| Application message older than its applicable window, or before the installed join epoch | Terminal rejection |
| Supported message names a future epoch after the complete preceding prefix | Terminal rejection, not a stale-attempt retry |
| Commit or proposal has a terminal validation result under the rules above, including a commit from a past epoch or a proposal reference absent from the complete prefix | Terminal rejection; missing local state for an already handled proposal remains held |
| Own envelope has no matching attempt under SEND-012, or its effects were already applied | Terminal rejection without applying effects again |
| Storage failure, interruption, absent local key, unsupported MLS or required group version, a held proposal kind under GMOD-001, unresolved validation, or any error not established as a terminal case above | Held |

| Identity input or failure | Disposition |
| --- | --- |
| Update validates against the complete preceding identity history | Apply under IDENT-004 and IDENT-071 |
| Invalid identity history, missing local state, unresolved verification, unsupported input, or any other validation failure | Hold the identity prefix and dependent work; do not substitute a later or partial association state |

Welcome rejection and retention belong to JOIN-047, JOIN-048, JOIN-077, and JOIN-079. A later Welcome can complete while an earlier one stays unresolved. A running client needs later attempts to reach the retention decision without a restart.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| PROC-011 | Reject on complete evidence | When a group envelope reaches the head with its preceding prefix handled and its required validation state available, the client MUST apply the group disposition table and durably record the topic, sequence id, and typed reason of a terminal rejection with the advance of `P`. It MUST base rejection on the ordered prefix, installed state, and verified identity state, using an own attempt only to match and validate its echo under SEND-012, and MUST NOT use wall-clock timing or app delivery progress as rejection evidence. | Local failures must not become permanent message loss. |
| PROC-012 | Preserve unresolved work | When group or identity work has a held disposition in the tables above, the client MUST keep it pending without advancing `P` across it or applying its successors, and MUST block parents that require invalid or unresolved identity history. While a Welcome remains held under JOIN, it MUST retain the original deadline across retries and restarts and, while running, eventually make the attempt at or after that deadline without requiring a new network event. | Invalid identity state must not authorize a member; an unreadable Welcome must not retain keys forever. |
| PROC-013 | A hold stays isolated | While a topic head or a Welcome is held, the client MUST continue to attempt independent ready topics and Welcomes and MUST preserve completed dependency results when another dependency fails. | One unreadable conversation must not stop all conversations. |

## 4. Targets and completion

An explicit sync samples the serving backend's heads. A send or externally supplied fetch target names a particular envelope instead. Only a backend-sampled head describes what that backend could see at capture; an external target can be ahead of it. Neither target moves with later traffic. API-244 omits empty topics from `QueryNewest`; that omission supplies target zero, not an uncaptured target.

Welcome-aware catch-up and sync of Welcomes with groups include discovery. They enroll groups installed by Welcomes through the fixed Welcome target, subject to the operation's consent selection. A generic topic operation or a send with fixed targets has no such discovery scope. Unrelated local groups and Welcomes beyond the target do not extend a run.

Completion on group and identity topics uses `P`. Completion on Welcome topics uses `F` and every unresolved Welcome through `H`; the highest successful Welcome does not replace that set. An inactive group completes its obligation at the removal position without processing messages for a membership it no longer holds.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| PROC-015 | Fix operation targets and scope | When an operation establishes its scope, the client MUST fix each target from a fresh `QueryNewest` result, a new registration acknowledgement for that operation, a publish receipt, or an externally supplied fetch target, using zero for a successfully queried empty topic and leaving failed captures absent. For Welcome-aware catch-up or sync of Welcomes with groups, it MUST enroll discovered groups through the fixed Welcome target that pass that operation's selection, with one fixed target each; later traffic and unrelated groups MUST NOT extend the run. For an all-groups stream, it MUST include stored groups at startup and discover newly stored groups without relying on a new network event. | Moving targets prevent completion under continuous traffic. |
| PROC-016 | Complete only processed work | The client MUST report an operation complete only after all required targets and discovery are established and, for every group or identity topic, `P >= H` or a removal at or below `H` made the group inactive, and for every Welcome topic, `F >= H` with no unresolved Welcome at or below `H`. | Receipt alone leaves the app with unprocessed ciphertext. |
| PROC-018 | Report incomplete work without loss | When an operation ends with unfinished work, the SDK MUST return a typed incomplete or blocked result naming each unfinished topic, its optional target, `F`, `P` for group or identity topics or unresolved Welcome ids through the target, and its typed cause, and MUST preserve committed progress and pending work. On scope replacement or cancellation it MUST identify cancelled obligations under their old scope generation, release only that operation's interests, and MUST NOT report them complete. Before returning blocked, it MUST let independent obligations complete, block, or reach the deadline. | An app needs to distinguish missing targets, held input, and cancelled work. |

## 5. Capacity

Pending work consumes storage and memory. Separate capacity for groups, Welcomes, and identity updates prevents a group backlog from blocking its own dependencies. These budgets limit pending work; they do not bound the whole database. Local delivery reads have a separate budget and are not envelope admission.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| PROC-020 | Capacity never loses work | When capacity prevents receipt, the client MUST preserve pending work and `F`, return a typed capacity error if no legal admission fits, and resume ready topics without starvation as capacity is released. It MUST reserve separate pending capacity for group, Welcome, and identity work so one kind cannot consume another kind's capacity, and MUST NOT discard pending input or advance `F` to make room. | Dropping a pending commit loses the state needed for later messages. |

## 6. Streams and recovery

The app selects network interests, including denied conversations. Consent filters local delivery; it does not authorize subscriptions. A topic stays registered while any operation needs it. Static replacement and reconnect both resume from durable receipt, and obsolete registration results cannot update a replacement scope.

The recovery rules below also cover silence and Query fallback. Silence is three advertised keepalive intervals without an inbound frame while the client is able to read, using 30 seconds for an absent or zero interval. Client backpressure is not wire silence. For a send or supplied target, a healthy receiver has a receipt wait of 1 second from the operation's start of receipt waiting; partial receipt does not restart it. An explicit sync starts Query immediately. Once `F >= H`, only processing remains.

AUTH-025 owns credential lockout and terminal credential failures. CONF-075 owns blocked connections caused by configuration. Terminal credential failures and blocked connections close a stream with network interest. A credential cool-down alone does not close it; the recovery episode limits still apply. An explicit remote cancellation ends the affected stream. A timeout or cancellation caused by the local transport uses normal recovery. Other transport errors retain pending work and use reconnect backoff. API-284 still prohibits an unchanged invalid request, so recovery cannot repeat that request unchanged.

Each app stream has its own recovery budget. Initial connection starts an episode. A later episode starts when that stream detects a failure. Ten failed recovery cycles or ten minutes without sustained recovery exhaust the budget. A failed fallback Query spends one cycle only for streams that select its topic. Sustained recovery means that the stream's selected registrations have remained active for 30 seconds under the wire-silence rules. A selected topic that is paused or blocked does not prove recovery. Application messages are not required. Opening a socket alone does not reset the budget. Time spent in credential cool-down counts toward the episode deadline. Waiting alone adds no failed recovery cycle.

Exhaustion ends that app stream and preserves stored receipt, processing, and delivery positions. Internal receipt, device-sync, and barrier operations keep their own lifecycle and can continue to use a shared receiver. A new stream on the same client starts with a fresh budget even when the network remains unavailable. Old timers and cleanup cannot end the new stream. Conversation notifications have no durable app delivery cursor; message readers resume under section 7.

The connection states describe transport activity: `connecting` is the first open, `connected` has a receipt source, `reconnecting` is recovery after a retryable failure, `failed` is recovery after a non-retryable source response with delayed retries, and `closed` has no future automatic attempts. `failed` does not mean pending processing failed. Registration states are `pending`, `active`, and `removed`; processing states are `pending`, `complete`, `blocked`, and `cancelled`.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| PROC-021 | Recover without losing progress | When a stream fails, becomes silent under the silence rule above, or needs replacement, the client MUST recover under the recovery rules in this section from current durable `F` and current interests, preserve pending work, discard obsolete registration results, and wait for a static replacement to be ready before cancelling replaced registrations. While `F < H`, it MUST use Query after `F` immediately for explicit sync or an uncovered target, and after the receipt wait above for a covered target, skipping that wait if it would reach the operation's deadline. It MUST NOT refetch a stored prefix or turn connection failure into processing completion. | Stream failure must not strand work that Query can supply. |
| PROC-023 | Expose scope and progress | An SDK MUST expose a current catch-up snapshot and change notifications with the scope generation, connection state under the definitions above, each selected topic's registration and processing states, optional fixed target, durable progress, unresolved Welcome ids, and typed cause. It MUST expose pending discovery until its targets are enrolled and cancellations under the old generation, and MUST NOT report the current scope caught up while any registration, discovery, or processing obligation is unfinished. | An app otherwise cannot tell a missing target from an empty topic or a retry from completion. |
| PROC-038 | Bound network recovery | While an app stream is active, the client MUST apply the recovery episode limits in this section and reset its budget only after sustained recovery. When the budget is exhausted, the SDK MUST end that stream with a typed exhaustion cause through its error callback, when supplied, and reject its pending iterator read, without treating a connection open alone as recovery. | A failed network must produce an actionable error instead of an unlimited wait. |
| PROC-039 | Fresh streams recover independently | When an app opens a stream after another stream ended, the client MUST give the new stream a fresh recovery budget and preserve its saved receipt, processing, and delivery positions. Ending one stream MUST NOT exhaust another stream's budget, create a client-wide or database-wide exhaustion latch, or let its delayed work close a replacement stream. | An app must be able to recover on the same client after an extended outage. |
| PROC-044 | Current connection state | When an app subscribes to a message or conversation stream's connection-state changes, the SDK MUST deliver the state at subscription and, after each change, its latest state under PROC-023, never older than one already delivered. | An older state can leave the app showing the wrong connection status. |

## 7. Local delivery

Message delivery reads retained local messages. The default consumer remembers app acknowledgement in `D`; a replay reader uses its supplied cursor and leaves `D` alone. Receipt, sync, and network reconnection do not acknowledge app work. Messages stored by another process or an import remain visible without a new network event.

An app callback or iterator can sit behind a binding queue. A successful enqueue is not an acknowledgement: the app must reach the acknowledgement boundary. A crash after app handling and before durable acknowledgement can repeat the item.

Scope and filter have different effects. A group outside scope keeps its backlog. A filter excludes a candidate inside scope and consumes it for default delivery. Conversation callbacks are live notifications; message replay does not replay conversation discovery or later message edits and deletions.

A deletion is an application message (CTYPE-014) that names a target message id. It changes what the app is shown for the target, so the client checks it before it applies it: CTYPE-018 owns which targets are eligible, and the sender check below owns who may delete. The sender is the authenticated MLS sender of the deletion, never a field of its payload. A deletion the client rejects is kept as a message like any other and has no effect on the target.

A database operation uses its normal retry and busy-wait policy. If a local read, acknowledgement write, or ownership operation then returns a storage error, that stream ends with the typed cause. There is no additional stream-level storage retry. The app decides when to open another stream. If the app completes an item but its acknowledgement write fails, a new stream can deliver the item again.

An explicit end under PROC-042 gives the reason `closed`. It is not an error close.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| PROC-024 | Stable delivery identity | When a message first becomes deliverable, the client MUST assign it one delivery number greater than all previously assigned numbers in that database, with the change that makes it deliverable, and MUST NOT change or reuse it, including after deletion or duplicate insertion. Every repeat delivery MUST carry the same message id and delivery cursor. | A changed identity makes app deduplication and resume unreliable. |
| PROC-025 | Deliver only eligible messages | A message stream MUST deliver only retained application or membership-change messages with `Published` status and a delivery number whose disappearing-message deadline has not passed, and MUST NOT deliver `Unpublished` or `Failed` messages. | An optimistic message is not yet accepted by the group. |
| PROC-026 | Resume durable app delivery | The default consumer MUST deliver eligible messages in delivery-number order above each selected group's saved `D`, including messages stored by sync, push-driven fetch, import, or another process, without requiring a network connection or a new network event. Receipt, processing, stream close, and reconnect MUST NOT advance or reset `D`. | Network activity must not consume the app's unread messages. |
| PROC-028 | Acknowledge at the app boundary | With one item handed to the app and not yet acknowledged, the SDK MUST persist that item's `D` only when the app callback returns normally or the app requests the next iterator item, including across binding queues, and MUST NOT hand over another item until that write succeeds. A callback error, enqueue, iterator cancellation, or failed acknowledgement write MUST leave the item eligible on restart. | Acknowledgement before app handling loses a message on a crash. |
| PROC-031 | Exclusive recoverable consumer ownership | Before each default-consumer handoff or change to `D`, the client MUST verify that the consumer still has exclusive ownership across clients sharing the database, and MUST reject a competing consumer with a typed ownership error. It MUST maintain that ownership while active or stop handoffs and writes, release ownership on clean close, and permit takeover after an owner stops without closing, without permitting the old owner to change the new owner's `D`. | Two owners can skip each other's messages. |
| PROC-032 | Scope excludes and filters consume | When an app changes selected groups or filters, including selection of denied groups, the SDK MUST apply that choice without making consent or membership a subscription authorization check, stop new handoffs for removed groups immediately, and reselect queued items from changed scopes before handoff even if a group was added back. For default delivery it MUST leave out-of-scope `D` unchanged and advance `D` past an in-scope filter exclusion without a callback, without replaying exclusions after a filter change; network interest MUST remain while another operation needs it, and local delivery MUST NOT wait for registration acknowledgement. | Removing a group must preserve its unread backlog without allowing stale callbacks. |
| PROC-033 | Cursors name the database | An SDK MUST attach a delivery cursor to every item from either reader, supply a beginning cursor with delivery number zero, and reject a supplied cursor for a different database with a typed cursor error. Exposing a cursor MUST NOT acknowledge default delivery. | A foreign cursor resumes at an unrelated message. |
| PROC-034 | Replay is independent | When an app opens a stream from a supplied cursor, the client MUST deliver every eligible message in scope strictly after that cursor in delivery-number order and then continue with new messages, without reading, changing, or acquiring ownership of `D`. | A replay must not consume the default consumer's backlog. |
| PROC-035 | History and stream meet | An SDK MUST supply selected eligible history and a delivery cursor from one database snapshot, such that every message made deliverable after that snapshot has a greater delivery number. | Separate snapshots can leave messages between history and stream. |
| PROC-037 | Apply only an authorized deletion | When the client processes a `xmtp.org/deleteMessage` message, it MUST change the target only when the target is a stored message of the same group that passes CTYPE-018, and the deletion's authenticated MLS sender inbox is the target's sender inbox or is in the group's `SUPER_ADMIN_LIST` when the deletion is processed. Otherwise it MUST leave the target and its delivery unchanged. | A member could erase another member's messages, and two installations that apply different rules show different histories. |
| PROC-040 | End streams on storage failure | When a local read, acknowledgement write, or ownership operation returns a storage error, the SDK MUST end that stream with the typed cause, report the error once through the error callback when supplied, and reject iterator consumption with the cause. It MUST NOT perform another handoff or add a stream-level storage retry, and MUST preserve PROC-028 and PROC-031 when the app opens another stream. | The app must control retries without losing unacknowledged work. |
| PROC-041 | One close notification | When a message or conversation stream ends for any reason, the SDK MUST deliver exactly one close notification with its close reason. | An app needs one final signal to release its stream state. |
| PROC-042 | Explicit end is closed | When an app ends a message or conversation reader, the SDK MUST complete the end without an error and give its stream the close reason `closed`. | An intentional stop must not appear to be a failure. |
| PROC-043 | Retryable error close | When a message or conversation stream ends with an error, the SDK MUST mark it retryable for PROC-038, PROC-040, and lag, and not retryable for AUTH-025, CONF-075, PROC-031, and PROC-033. | An app needs to decide whether to reopen without retrying a terminal error. |
| PROC-045 | Codec failure keeps stream open | When a custom content codec fails while a message stream decodes an item, the SDK MUST expose that item as custom content with its error and continue the stream under CTYPE-008 and CTYPE-009. | One app codec failure must not hide later messages. |

## Known limitations

Delivery is at-least-once. A crash or loss of consumer ownership after app handling but before durable acknowledgement can repeat an item. The app can deduplicate on its message id.

A backend-sampled target includes replica lag. Completion through it does not claim receipt of every primary commit before the call. An externally supplied target has no such sampled-head guarantee.

Push-envelope entry points accept a backend `ServerEnvelope` and use its metadata as an ordered-fetch target. These entry points do not accept the PUSH JSON body containing `topic` and `sequence_id`; automatic Welcome discovery for an unknown group named by that body is not part of their interface.

A held group or identity prefix has no retention deadline. Unsupported input can require a client upgrade; invalid identity history can require repair. Retrying a local failure does not make invalid history valid.
