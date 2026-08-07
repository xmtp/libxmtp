# Key-Package Liveness Check — Design

Date: 2026-08-07
Status: implemented
Depends on: [xmtp/proto#342](https://github.com/xmtp/proto/pull/342), the `KpLiveness`
`Task` variant. It merged as `f53e21af`. `crates/xmtp_proto/proto_version` pins that rev.

## 1. Problem

Key-package rotation is driven **purely by a local deadline column**
(`identity.next_key_package_rotation_ns`). `rotate_if_needed` reads
`is_identity_needs_rotation()`; if the column says "not due", it returns
`Ok(false)` and does nothing. Nothing in the client ever verifies that a usable
key package is actually published on the network.

When that column is wrong, the resulting failure is silent, total, and
self-sustaining:

1. The client stops rotating. Its published key package passes `not_after` and
   becomes unusable. Lifetime is ~84 days and the rotation interval is 30 days,
   so a healthy client keeps ~54 days of slack. A client reaches this state only
   if it misses several rotations.
2. `fetch_key_packages` for that installation returns nothing.
3. Anyone adding the installation to a group records it in
   `failed_installations` instead of emitting an `Add` proposal. The
   installation is listed as a group member **with no MLS leaf**.
4. With no leaf, no welcome is ever delivered, so the existing welcome-driven
   rotation nudge (`queue_key_rotation`, fired from the welcome path) never
   fires either.
5. The affected client observes **no local error at all**. It syncs, sends
   messages, and creates groups against its existing groups, where it still has
   valid leaves. Nothing is logged.

A real dev-network installation reached this state. It registered in early May.
Its key package expired in late July. Weeks later the client still looked healthy
to itself. Seven client builds ran in a 31-hour window with rotation two months
overdue. The INFO line `"Start rotating keys and uploading the new key package"`
never appeared. Its absence was the only external symptom. A second client on the
same build and network rotated correctly, so the rotation code itself works.

The systemic gap is the missing feedback edge: **local rotation state is never
reconciled against network reality.**

## 2. Goals and non-goals

### Goals

- Detect, from the affected client itself, that it has no usable key package on
  the network, and repair it by queueing a rotation.
- Run unconditionally on a throttle, so the check does not depend on any signal
  that the broken state suppresses.
- Rescue already-broken installations on first client build after upgrade.
- Make rotation scheduling visible in logs. The "not due" path is silent today.
  That silence is why the incident took hours to diagnose.

### Non-goals

- Changing rotation cadence, key-package lifetime, or the rotation deadline
  semantics. `KEY_PACKAGE_ROTATION_INTERVAL_NS` is untouched.
- Fixing whatever corrupted the deadline column. That root cause is still
  unknown. This change is the safety net. It makes the failure self-healing and
  visible, whatever the cause.
- Server-side detection or backfill.

## 3. Architecture

A new recurring singleton TaskRunner task, `KpLiveness`, seeded alongside the
existing `KpRotation` / `KpDeletion` singletons.

```text
KpLiveness dispatch
  │
  ├─ throttle: now < checked_at + KEY_PACKAGE_LIVENESS_INTERVAL_NS
  │     └─> debug log, RescheduleAt(checked_at + interval)          [no network]
  │
  └─ probe: fetch_key_packages([my_installation_id])
        ├─ Ok, verifies, > MIN_REMAINING_LIFETIME left
        │     └─> INFO "healthy", record checked_at,
        │          RescheduleAt(now + interval)
        │
        ├─ absent | unverifiable | expiring soon
        │     └─> WARN with reason, queue_key_rotation()  (column + pull-in
        │          + wake, one transaction), record checked_at,
        │          RescheduleAt(now + interval)
        │
        └─ transient API failure (offline, 5xx, timeout)
              └─> WARN "inconclusive", do NOT record checked_at,
                   RescheduleAt(now + KEY_PACKAGE_LIVENESS_RETRY_INTERVAL_NS)
```

`queue_key_rotation` is the repair path the welcome nudge already uses. In one
transaction it lowers `next_key_package_rotation_ns` (5s debounce) and enqueues a
`PullInDeadline` against the `KpRotation` singleton. Then it wakes the worker.
Liveness does not rotate inline. It reuses the one repair path.

### Components

| Component | Location | Role |
| --- | --- | --- |
| `KpLiveness` task variant | `xmtp/proto`, `proto/mls/database/task.proto` field 8 | Durable, independently scheduled singleton |
| `kp_liveness_proto` / `kp_liveness_hash` | `crates/xmtp_mls/src/worker/key_package_maintenance.rs` | Stable payload + `data_hash` for pull-ins |
| `LivenessOutcome` | same | Probe classification (`Healthy` / `Absent` / `Unverifiable` / `ExpiringSoon` / `Inconclusive`) |
| `probe_key_package_liveness` | same | The network probe; no writes |
| `run_liveness_check` | same | Throttle + probe + repair + next deadline |
| `nudge_liveness` | same | Seed-then-pull-in, for event-driven triggers |
| `KpLiveness` dispatch arm | `crates/xmtp_mls/src/worker/tasks.rs` | Wires the handler into the TaskRunner |
| `key_package_liveness_checked_at_ns` | `identity` table, `crates/xmtp_db` | Durable throttle state |

### Triggers

**1. Scheduled (primary).** `seed_and_reconcile_kp_tasks` insert-or-ignores a
`KpLiveness` seed at client build, like `KpRotation` and `KpDeletion`.
`create_or_ignore_task` keeps an existing row's deadline, so the throttle
survives restarts. A fresh seed is due at `now`, so the first build after upgrade
checks at once. That is what rescues installations that are already broken.

This is the trigger that would have caught the incident, and it is the only one
that is load-bearing.

**2. Rotation handoff.** The `KpRotation` arm calls `nudge_liveness` on every
dispatch, rotated or not. The not-rotated branch is the code path that went
silent in the incident. It must now hand off proof that the decision was correct.

This trigger is not needed for normal cadence, which the daily task supplies. Its
purpose is narrow: it is a second way to un-strand a `KpLiveness` row whose
deadline drifted into the future, while rotation still dispatches. Startup
reconciliation is the primary recovery. A not-rotated-only nudge would skip the
healthy client. It costs two insert-or-ignore writes per dispatch (~monthly).

### Rejected trigger: own installation in `failed_installations`

A third trigger was built and then removed: on validating an incoming commit,
notice our own installation id in the commit's carried-forward
`failed_installations` and nudge the check.

It was removed for two independent reasons, either of which is sufficient.

- **It is almost unreachable.** An installation in `failed_installations` has no
  MLS leaf in that group, so it cannot decrypt the group's commits. The client
  that needs the signal most is the one that cannot see it.
- **It wrote durable state inside a security boundary, from unvalidated input.**
  The hook ran before `expected_diff_matches_commit` and the credential checks.
  A rejected commit could still write task rows and wake the worker. A hostile
  member could drive DB writes with commits that never validate. Coalescing
  bounds the row count at one instant, not the write rate over time.

The safe version defers the side effect until validation completes, as an event
outside the validator. That is real work for a trigger that carries no load. The
scheduled check already provides the recovery, so the hook does not ship.

Worth recording for anyone tempted to re-add it: if it comes back, it must be a
nudge to *verify*, never a direct rotation. The list is remote-supplied, so
acting on it directly would let a hostile member force unbounded key-package
churn.

### Rejected trigger: `MissingIdentityUpdate`

`AssociationError::MissingIdentityUpdate` was evaluated as an event trigger and
**not** wired. It is raised in three places, all in
`crates/xmtp_mls/src/identity_updates.rs`, and every one of them means "the
local identity-update log is behind the network for *some* inbox" — including
other participants' inboxes, not ours. It says nothing about key packages, the
remedy is an identity-update sync rather than a rotation, and a liveness probe
fired from it would find our key package healthy and do nothing. Wiring it would
add network probes with no diagnostic or repair value.

## 4. Data flow and state

### Throttle state

`identity.key_package_liveness_checked_at_ns` (`BIGINT NULL`, new column). NULL
means "never checked" and is therefore due.

**The stamp is never trusted blindly.** A future value can come from forward
clock skew at write time, or from column corruption. It would suppress the
watchdog until real time caught up, possibly for months. That is the same failure
this check exists to catch: a wrong local timestamp disables key package
maintenance. So the throttle holds only for a stamp in the past and inside one
interval. Anything else logs a warning and counts as due. A watchdog whose
off-switch responds to a bad timestamp is not a watchdog. `next_liveness_deadline`
is the one place that decides this. A boundary test pins every case: `None`,
just-checked, exactly one interval old, future, `i64::MAX`, and ancient.

**Clamping the stamp is not enough.** The same clock jump also writes
`skewed_now + interval` onto the task row. The dispatcher skips a task whose
deadline is in the future, so the handler never runs and its clamp never
executes. Startup reconciliation carries the recovery. It enqueues a liveness
pull-in **unconditionally**, computed by the same `next_liveness_deadline`, which
returns "due now" for a bad stamp. Pull-ins only lower deadlines, so a healthy
row is unchanged and a stranded row is rescued. When liveness runs again it also
repairs the rotation deadline, through `queue_key_rotation`'s lower-only write.
Recovering the watchdog recovers rotation with it.

**Recovery from a forward clock jump needs a client restart. That is a real
limitation, not a solved problem.** The `KpRotation` arm nudges liveness on every
dispatch, which un-strands the liveness row while rotation still dispatches. But
a jump large enough to strand both recurring rows leaves no in-process trigger.
Every deadline in the tasks table is an absolute wall-clock instant written under
the skew. The dispatcher only skips future rows, and nothing reconciles on a
timer. No work inside this feature fixes that. The exposure belongs to the
scheduler, not to key packages, and every recurring task kind has it.

The general fix is for the TaskRunner to distrust a deadline further out than the
task's own maximum interval, and treat it as due. That belongs with the task
leasing follow-up in §5a. Mobile and desktop clients restart often. A long-lived
server-side client needs the restart or the scheduler fix.

Why a column, and not the task row's `next_attempt_at_ns`:

- A `PullInDeadline` nudge overwrites the target's `next_attempt_at_ns`. The row
  cannot hold both the next run and the last run. The nudge destroys the
  information the throttle needs.
- If the throttle lived in the row, every nudge would force a network probe.

With the column the handler is the authority. A nudge only asks the task to
consider running. If the interval has not elapsed, the handler declines
cheaply. Nudges are then free and idempotent.

The column is only written on a *conclusive* probe. An inconclusive probe
(offline client) leaves it alone and retries on the shorter retry interval, so
an offline client does not silently consume its 24h budget.

A successful key-package **upload** also stamps it. That happens at registration,
where the `StoredIdentity` row is built next to the initial rotation deadline,
and on every later rotation.

Be precise about the assumption. A server that acknowledges a write does not
prove the write is readable. This is an accepted assumption, not stronger
evidence than a read. The alternative is worse. Without it, every new client
probes at once and races its own registration. That produces a false "no usable
key package" warning and a false rotation on almost every new client, which
poisons the signal this change exists to make trustworthy. The cost is a bounded
24h delay to notice an acknowledged but unindexed write, against an ~84-day
lifetime.

The coupling runs one way. Rotation success feeds liveness state, never the
reverse. Liveness scheduling never depends on rotation state. A stale stamp
causes at most one extra probe.

### Nudge idempotence

`nudge_liveness` enqueues a `PullInDeadline` with a fixed `not_later_than_ns` of
`0`, not `now`. Every call then builds the same payload, so
`create_or_ignore_task` coalesces on `data_hash`. Only one pull-in can be pending
at a time. The `0` means "at the next opportunity". The handler's throttle, not
this deadline, decides if work happens.

The rotation nudge does the same thing. It keeps its payload stable by reading
the deadline column inside the transaction.

The exact scope: coalescing holds only while a pull-in row is pending. After the
worker consumes and deletes it, the next nudge creates another. Nudging is
idempotent in the task table, but the table does not rate-limit it. Only the DB
throttle bounds network probes. This is why the one remaining nudge site fires
at most monthly, and why the commit-driven nudge was removed.

### Constants

| Constant | Value |
| --- | --- |
| `KEY_PACKAGE_LIVENESS_INTERVAL_NS` | 1 day |
| `KEY_PACKAGE_LIVENESS_RETRY_INTERVAL_NS` | 1 hour |
| `KEY_PACKAGE_LIVENESS_MIN_REMAINING_LIFETIME_NS` | 7 days |

All three live in `common/mls.rs`, with **no prod/test split**. That is a
deliberate correction. An earlier revision shortened the interval to 3s for
tests, which made any client built more than 3s after registration probe the
network on startup. That broke `create_client_does_not_hit_network` in
`bindings/mobile`, a test that asserts a client build performs no network I/O.
The short interval was the bug, not the assertion.

Tests that need the check to run write the throttle column directly with
`set_key_package_liveness_checked_at_ns`. That is faster than sleeping and it
keeps the interval realistic, so no unrelated test starts probing.

The 7-day margin holds for tests too: a healthy client rotates every 30 days
against an ~84-day lifetime, so a live key package always keeps far more than 7
days, and test key packages use the same openmls lifetime as production.

## 5. Error handling

**"Absent" has two wire shapes. A test round found the second one.**

`fetch_key_packages` is a positional batch API. A short response surfaces as
`ApiError::MismatchedKeyPackages`, which is unambiguous for a single-id request.
The existing `is_missing_key_package` helper in `crates/xmtp_mls/src/groups/mod.rs`
handles that shape, and this design first assumed it was the only one.

The backend returns a present-but-empty payload for an installation with no
published key package. `xdbg query fetch-key-packages` showed exactly that for
the installation that prompted this work. Both shapes mean `Absent`.

Two further classification rules exist because the batch API is **positional** —
the response carries no installation ids of its own:

- Only `MismatchedKeyPackages { key_packages: 0, installation_keys: 1 }` proves
  absence. Any other count mismatch is a server or protocol defect. Treating it
  as absence would make a backend incident rotate every client's key package on
  every interval while repairing nothing.
- The probe compares the verified key package's own installation id, its leaf
  signature key, against the id it asked for. A backend, cache, or ordering
  defect can return a valid key package that belongs to someone else. It
  verifies, it has a healthy lifetime, and we would record ourselves as
  reachable while staying unaddable. That rebuilds the silent loop this check
  must break. A mismatch is `Inconclusive`, not unhealthy, for the same reason as
  an overfull response: it says nothing about our key package, and a new upload
  cannot repair a mapping defect. Rotation would make one backend incident churn
  the whole fleet every interval and fix nothing. The probe logs it at ERROR.

So the probe calls `context.api().fetch_key_packages` directly, not
`MlsStore::get_key_packages_for_installation_ids`. `MlsStore` deserializes
eagerly. It turns an empty payload into a generic `KeyPackageVerificationError`,
which looks the same as a corrupt key package. Both still trigger a rotation, so
behavior stays correct either way. But the log would say "failed verification"
about an installation that published nothing. That is the misleading signal this
change exists to remove.

Classification:

| Probe result | Outcome | Action |
| --- | --- | --- |
| `Err(MismatchedKeyPackages { key_packages: 0, installation_keys: 1 })` | `Absent` | rotate |
| `Err(MismatchedKeyPackages { .. })` — any other counts | `Inconclusive` | retry sooner |
| `Ok(map)`, id missing from map | `Absent` | rotate |
| `Ok(map)`, **empty payload** | `Absent` | rotate |
| `Ok(map)`, bytes fail `VerifiedKeyPackageV2::from_bytes` | `Unverifiable` | rotate |
| `Ok(map)`, verifies, but for a **different installation** | `Inconclusive` | retry sooner |
| `Ok(map)`, verifies, `life_time()` is `None` | `Unverifiable` | rotate |
| `Ok(map)`, verifies, `not_after` within margin | `ExpiringSoon` | rotate |
| `Ok(map)`, verifies, healthy | `Healthy` | record only |
| any other `Err` | `Inconclusive` | retry sooner, no state change |

An expired key package is `Unverifiable`, not `Healthy`.
`VerifiedKeyPackageV2::from_bytes` validates with `LeafNodeLifetimePolicy::Verify`,
so it fails before `life_time()` is read. The `ExpiringSoon` margin covers the
window before expiry, which is the only window where repair is still cheap.

The handler never returns `Err` for a probe failure. An `Err` drives the
TaskRunner's backoff (2s initial, 60s cap), which would make an offline client
re-probe every minute forever. The inconclusive path returns a normal
`RescheduleAt` instead. Real storage errors still propagate as `Err` through
`KeyPackageMaintenanceError`, which forwards `needs_db_reconnect` for the
supervisor's reconnect contract.

`rotate_and_upload_key_package`'s existing contract is preserved unchanged: on
upload failure it returns `Err` **without** resetting the deadline, so a failed
rotation stays due.

## 5a. Known limitations (deliberately not fixed here)

**Single-TaskRunner scope.** "At most one probe per interval" holds for one
TaskRunner. `get_next_task` reads the earliest due row without claiming or
leasing it, so two TaskRunners sharing a DB could select the same `KpLiveness`
row, both read the same stale stamp, and both probe — and, if both saw an
unhealthy result, both queue a rotation. The `task_receiver` mutex serializes
runners within a process, not across processes.

The worst case is not benign. State it precisely instead of calling it
"duplicate work". Two concurrent rotations generate key packages A then B, with
history ids A < B. `rotate_and_upload_key_package` marks every history row below
the one it uploaded for deletion. If B's upload completes first, it marks A for
deletion. If A's upload lands last, the network publishes A. After the grace
period the local private material for A is gone. The client then fetches A. It
verifies, it has the right installation id, and it has a healthy lifetime, so the
watchdog reports `Healthy`. Meanwhile welcomes encrypted to A cannot be
processed. The watchdog cannot see this unreachability.

Three reasons this is documented rather than fixed here:

- **It exists already and is unchanged.** The race lives in `KpRotation`,
  `rotate_and_upload_key_package`, and the mark-below-id cleanup. Scheduled
  rotation and the welcome nudge have had the same exposure since they landed.
  This change adds no concurrency to that path.
- **It does not increase rotation frequency for healthy clients.** A liveness
  check on a healthy installation performs one read and queues nothing. Extra
  rotations only arise when a repair is warranted: an installation that is
  already unreachable, or one inside the `ExpiringSoon` margin (still reachable,
  but overdue). Note this is *not* a claim that the race outcome is milder than
  what it repairs — the race produces unreachability the watchdog cannot detect,
  which is in that respect worse. The claim is only that this change does not
  make the race more likely to be reached.
- **The fix belongs elsewhere.** It needs atomic task claiming with a
  crash-recoverable lease across the whole TaskRunner, which serves seven other
  task kinds. Landing that inside a key-package bugfix makes both changes harder
  to review and to revert.

Follow-up worth filing separately: either task claiming/leases in the
TaskRunner, or a DB-scoped rotation lease so `rotate_and_upload_key_package`
cannot interleave with itself. Until then, the operating assumption — already
implicit in the existing design — is one TaskRunner per database.

**Failure-domain independence is partial.** The liveness task is independent of
rotation's deadline and retry budget, which is what this change needed. It is not
independent of the TaskRunner. Tasks run serially, so a slow task delays the
probe. Disabling the TaskRunner disables the watchdog and the thing it watches.
A fully independent watchdog would be its own worker with its own timeout budget.
That is a larger change, and the observed failure does not need it.

## 6. Observability

The "not needed" path was silent, which made the incident expensive to diagnose.
Added, at INFO unless noted:

- `rotate_if_needed`, not due: logs the deadline and the time remaining. Low
  volume, because the `KpRotation` task reschedules ~30 days out.
- `rotate_if_needed`, due: logs that it proceeds, before
  `rotate_and_upload_key_package` writes its existing "Start rotating keys…" line.
- Liveness healthy: logs the remaining key-package lifetime. A log sample then
  answers "does this client have a live key package?".
- Liveness unhealthy: WARN with the reason, and that it queued a rotation.
- Liveness inconclusive: WARN with the error.
- Liveness throttled: DEBUG with the next check time. This path is frequent, so
  it is not INFO.

## 7. Design alternatives considered

### A. Dedicated `KpLiveness` proto variant (chosen)

A new variant in the `Task` oneof in `xmtp/proto`, plus an in-repo `xmtp_db`
migration for the throttle column.

- **+** Independent schedule. Liveness needs a ~24h cadence; `KpRotation`'s
  deadline follows `next_key_package_rotation_ns` (~30 days).
- **+** Independent retry/backoff. A liveness probe is a network call that fails
  routinely on offline clients. A shared task row would mean a failing probe
  pushes the *rotation* task into exponential backoff, and a failing rotation
  suppresses liveness.
- **+** Independently nudgeable. `PullInDeadline` targets a task by `data_hash`.
  With one shared row there is no way to express "verify now" as distinct from
  "rotate now".
- **+** Structural independence. The watchdog shares no row, deadline, or retry
  budget with the thing it watches. The bug is a wrong rotation deadline, so
  this matters.
- **−** Cross-repo dependency: the proto PR had to land before
  `crates/xmtp_proto/src/gen` could be regenerated. It has
  ([xmtp/proto#342](https://github.com/xmtp/proto/pull/342), merged as
  `f53e21af`), so this cost is now paid.

### B. Fold into the existing `KpRotation` arm (rejected)

Run the probe inside the `KpRotation` dispatch arm when rotation is not due,
reschedule the shared row to `min(rotation_deadline, liveness_deadline)`.

- **+** No proto change; ships without a cross-repo dependency.
- **−** The shared row must wake daily instead of monthly, so liveness drives the
  rotation task's deadline. Two unrelated schedules share one column.
- **−** A shared retry budget, in both directions. This is the strongest
  objection. An offline client's failing probe would push its rotation into
  backoff. Avoiding that means swallowing all probe errors, which weakens the
  diagnostics that are half the point of this change.
- **−** Nudges become ambiguous. A pull-in can only say "run the KP task", not
  which half of it.
- **−** It still needs the same `xmtp_db` migration for throttle state. It avoids
  the proto work, not the schema work.

### Decision

**A.** The original brief preferred B to avoid a cross-repo dependency. That
constraint was lifted, because proto PRs land quickly. Without that cost, B has
no remaining advantage, and its coupling problems are the class of problem this
change exists to fix. A watchdog that shares a schedule and a retry budget with
its subject fails with its subject.

Sequencing: xmtp/proto#342 merged first. Then `crates/xmtp_proto/proto_version`
moved to the merged `main` rev (`f53e21af`), and the generated Rust came from the
normal `dev/gen_protos.sh` path. Nothing was hand-edited. The regenerated diff
covers only the new `KpLiveness` message and its oneof arm.

## 8. Testing

Native only (`#[cfg(all(test, not(target_arch = "wasm32")))]`), in
`crates/xmtp_mls/src/worker/key_package_maintenance.rs`. They follow
`.claude/skills/writing-rust-tests/SKILL.md`: `#[xmtp_common::test(unwrap_try = true)]`
and the `tester!` macro with `worker_config: no_runner_cfg()`. Tasks then
dispatch through `TaskWorker::run_and_reschedule_task` instead of racing a live
worker.

| Test | Asserts |
| --- | --- |
| `liveness_healthy_kp_records_and_does_not_queue_rotation` | A freshly registered client's probe is `Healthy`; `checked_at` is recorded; the rotation deadline is untouched and no `KpRotation` pull-in is enqueued |
| `liveness_absent_kp_queues_and_nudges_rotation` | With the network probe seeing no key package, rotation becomes due and a `KpRotation` pull-in is enqueued |
| `liveness_throttle_skips_repeat_check` | A second dispatch inside the interval performs no probe and reschedules to `checked_at + interval` |
| `liveness_nudge_is_idempotent` | Repeated `nudge_liveness` calls collapse to exactly one pending pull-in row |
| `liveness_future_stamp_does_not_disable_check` | A stamp a year in the future does not suppress the probe; the conclusive check overwrites it |
| `startup_reconcile_rescues_liveness_row_stranded_by_clock_skew` | A row dated a year out is confirmed *not* dispatchable, then reconciliation pulls it back to due — the scheduler-level half of the clock-skew defence |
| `liveness_deadline_never_trusts_an_out_of_range_stamp` | Every `next_liveness_deadline` boundary: none, just-checked, exactly one interval, future, `i64::MAX`, ancient |
| `classify_covers_every_response_shape` | Every wire shape classified directly: no entry, empty payload, corrupt bytes, a *valid key package belonging to another installation*, healthy, and inside the expiry margin |
| `classify_fetch_error_only_treats_empty_response_as_absent` | Only "asked for 1, got 0" is absence; an overfull response is `Inconclusive` |
| `probe_reports_absent_for_unregistered_installation` | A live-backend probe of a never-registered installation classifies as `Absent` — this is the assertion that caught the empty-payload shape |
| `kp_tasks_seeded_when_workers_run_absent_when_passive` (extended) | The `KpLiveness` singleton is seeded when the TaskRunner is enabled and absent when it is not |
| `rotation_task_not_due_reschedules_without_rotating` (extended) | The `KpRotation` arm hands off to liveness when rotation is not due |

Coverage notes:

- A `PROBE_OVERRIDE` test hook drives the unhealthy repair wiring, because the
  test backend cannot forget a published key package. The `RESCHEDULE_OVERRIDE`
  hook in `worker/tasks.rs` works the same way.
- Such a hook hides changes in the classification logic, so classification is
  not tested through it. `classify_key_package` and `classify_fetch_error` are
  pure functions, and `now_ns` is a parameter rather than a hidden clock. Tests
  assert them directly over every response shape, with no network and no
  override. `probe_reports_absent_for_unregistered_installation` also pins the
  real backend. That test found the empty-payload shape.
- The wrong-installation test classifies a valid key package against a different
  requested id. That is the failure a positional API can produce.

Registration seeds `key_package_liveness_checked_at_ns`, so a test that needs a
real check first moves that stamp back past one interval.

## 9. Migration and rollout

- The `xmtp_db` migration
  `2026-08-07-000000-0000_add_key_package_liveness_to_identity` adds one nullable
  column. There is no backfill. NULL means "never checked", so every existing
  client is due on its first build after upgrade.
- The proto field is additive. An older client decodes `KpLiveness` as an unknown
  oneof variant, and the existing `None` arm deletes the row. A downgrade cannot
  wedge the TaskRunner.
- Liveness is coupled to `WorkerKind::TaskRunner`, like the other KP maintenance
  tasks. Disabling the TaskRunner disables it too, which matches existing
  behavior.
- Cost per client: one `fetch_key_packages` round trip per day.
