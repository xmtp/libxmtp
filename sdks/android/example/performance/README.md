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
the production timestamp loader and production cache. It records maximum page,
transcript, and cache row counts. It also records the actual history read
size, including the sentinel, with a limit of 501. The observation calls the
unchanged production read function with its real selection. The heap result
keeps the last 500 projected UI rows alive and records before/after totals. The red control removes the production cache
eviction statement. It must fail with `Transcript cache trimming was removed`
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
# Linux x86_64 with KVM and the current backend:
dev/nix-shell 'just android example-performance'
```

The Android recipe stages matched bindings, loads the worktree backend URL,
sets API 34 and the fixed CPU/RAM flags, forwards the backend port, and runs the
owned emulator scope. The Linux CI job retains all existing SDK platform and
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
