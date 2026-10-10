# Messenger performance proof

This runner measures local SDK queries and the app row projection. It does not
measure client startup, network publication, object transfer, or frame timing.
Do not use its host tests as evidence of device performance.

Use a Linux x86_64 host with KVM. The fixed device is an API 34 x86_64 emulator
with four virtual CPUs and 4096 MiB RAM. The runner checks its API, ABI, CPU
count, hardware, and guest memory. An ARM emulator does not satisfy this gate.
The Android CI host is `blacksmith-16vcpu-ubuntu-2404`. The result records its
CPU, memory, kernel, emulator version, launch flags, SDK version, and source
commit. Keep this host fixed when you compare results.

## Workload

The device test creates two public `SDKClient` instances. The sender creates
1000 groups and sends text through the current public codec. The receiver
retains exactly 100000 Published application messages. No app SQL, database
fixture insertion, SDK test export, or scripted message data creates this
workload.

The first group has 50000 messages. The next nine groups have 1000 messages
each. Another 410 groups have 42 messages each. The last 580 groups have 41
messages each. Each text has a distinct group/row prefix and 256 ASCII padding
bytes. The receiver permits all groups. The test reopens its seeded app profile through
the normal AppSession restore path. The production generation guards stay active. Names, previews, and unread counts use
the production conversation list projection, including its four-read limit.
The unread selection includes the sender's incoming texts.

The sender queues at most 256 optimistic texts before `publishMessages()`.
Before the next send, fixture preparation calls receiver group sync and requires
the exact Published Application count after each batch. It also checks the
49,900-row flush, each newest separate send, and each group tail. This keeps
the unsynced text count at most 256. SDK receipt cursors can include metadata;
they are not text counts. Each checkpoint records counts and send, publication,
sync and count durations in app-private `seed-progress.jsonl` and stdout. A
failed SDK barrier records its typed target, receipt and processing progress.
The host runner saves this file even when instrumentation fails. No successful
receipt cursor is invented: public sync returns no cursor. This pacing is
fixture preparation; its complete Linux seed result remains pending.

The test records the initial ViewModel state and separate start, completion,
or failure markers for restored-owner and first-list readiness. Both waits keep
their 120-second deadline. `readiness-progress.jsonl` contains screen names,
row counts, state flags and exception classes. It excludes error messages,
credentials, keys, payloads and profile identifiers. Failed runs retain this
file beside the seed progress. A readiness timeout is not a measured budget result.

`measurement-progress.jsonl` records each warmup and measured iteration. It
separates the actual SDK page call from row mapping and retains the original
overall durations. Row counts, opaque-boundary use and sample flags are recorded;
message bodies, IDs and keys are excluded. Records are written after the timers.
The original five warmups, thirty ordinary samples and every budget still apply.
Segment timings diagnose the original gate; they cannot replace its overall
durations or exclude SDK work. Failed runs retain this file for attribution.

Fixture preparation uses public Welcome and group sync calls. It also drains
fixture texts through the public sequential Flow before normal AppSession
restore. The test observes the complete production app callback after reopen.
It waits for a quiet callback interval outside timing, accepts at most one tail
replay, and records that count. The count must stay unchanged through all
measurements. A seed backlog or later replay fails the gate. Text selection reads the public
`SDKMessageContent.Standard.value` wrapper through the same selector as the app
row mapper. A generated-message JVM regression rejects the old direct-content
check. This is type-selection proof; the full device drain is still unrun.
The measured
reads do not call sync. The newest 100 texts in the large group use separate
publication calls. A backend batch can assign equal sent timestamps, so these
calls establish two ordinary 50-row pages for the main timing gate.

The test checks the backend and each group's raw selected count and the full conversation
count before it measures. The workload stays in the test app between the
green, broken, and restored runs. Each Gradle invocation sets
`android.injected.androidTest.leaveApksInstalledAfterRun=true`. This preserves
the app, result, database and Keystore records across the three passes. Keys use the app's encrypted secret store.
The owned emulator scope removes the device files when it ends.

## Measurements and failure control

Discard five warmup runs. Measure 30 runs outside the timestamp tie retry
case. Record tie retries separately. The runner uses nearest-rank p95, which
is the 29th sorted sample for 30 runs.

| Measure | Limit |
| --- | --- |
| First 50 transcript rows, query and row mapping | p95 at most 300 ms |
| Next older 50 rows, query and row mapping | p95 at most 250 ms |
| First 50 names, previews, and counts after client ready | p95 at most 1000 ms |
| Ten visited transcripts, managed heap delta after GC | At most 64 MiB |
| Published rows retained in one transcript | At most 500 |
| Published rows retained across cached transcripts | At most 1500 in three transcripts |

The heap run reads at least 1000 messages in each of ten transcripts through
the production SDK page loader and `SDKTranscriptCache`. Each query uses the
public SDK page method with its raw `MessageHistoryPosition` continuation.
Converted rows plus skipped candidates must not exceed the 50-row SDK request.
The production four-read budget and Published selection remain active. The
performance hook retains the complete `HistoryWindow` through the production
cache append method, including raw positions. It does not rebuild boundaries
from converted messages. It records maximum page, transcript, and cache row
counts. The observation calls the unchanged production page read with its real
selection. The heap result
keeps the last 500 projected UI rows alive and records before/after totals. The red control removes the production cache
eviction statement in `SDKHistoryPages.kt`. It rejects the pending overlay's
legacy cache as a control target. It must fail with `Transcript cache trimming was removed`
and retain more than 1500 cached rows. The host runner restores the exact
source in a `finally` block. It then rebuilds and repeats the device gate.
Seeding happens once. A partial workload cannot pass the gate.

The result files contain all samples. `seedMs` records fixture preparation
separately. Until an actual fixed-device run succeeds, these budgets are
unverified. Slow seeding, missing hardware, or a failed gate must be reported;
they do not permit a smaller measured workload.

## Commands

Run from the repository root. The host gate needs no device or backend:

```sh
dev/nix-shell 'just android example-performance-check'
# Linux x86_64 with KVM and local worktree services:
dev/nix-shell 'just android example-performance'
```

The Android recipe stages matched bindings and starts an owned disposable
backend with a unique database, listener and process session. The runner admits
only its private active lease and generated loopback URL. It has no arbitrary
backend URL argument or remote opt-in. Instrumentation rejects missing admission
and noncanonical loopback targets before SDK creation. The fixture owns backend,
database and child teardown. The recipe sets API 34 and the fixed CPU/RAM flags,
forwards only that backend's port, and runs the owned emulator scope.
The Linux CI job retains all existing SDK platform and
app integration gates. Its 210-minute step and 240-minute job limits are
provisional. Set the final timeout from observed seed progress and `seedMs`.
Do not reduce the dataset to fit a timeout.

Each run streams seed progress from device logcat and saves that log. The host
runner keeps a separate copy of each run's connected XML and device logs.
The workload identity must match across green, broken, and restored runs.

Result files are `environment.json`, `green.json`, `red-cache.json`, and
`restored.json`, with a Gradle log for each run. Preserve the connected test
XML and emulator startup diagnostics with them. An instrumentation failure
cannot be replaced by a passing JSON report.
