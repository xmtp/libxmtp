---
title: Debug
---

## Forked group debugging tool

A conversation has the asynchronous `debugInfo()` method. You can use this to see:

- The MLS epoch of a group chat conversation for a member
- The local commit log for expert analysis
- Whether a group chat is forked

Use the conversation debug information when a group stops processing messages. Some forks can recover through a remove-and-add commit and a replacement Welcome. Recovery cannot repair every fork. Collect the debug data before you decide whether to create a new group.

All four SDKs return these fields: `epoch`, `cursor`, `maybeForked`, `isCommitLogForked`, `localCommitLog`, `remoteCommitLog`, and `forkDetails`. An absent `isCommitLogForked` value means the result is unknown. `maybeForked` is a separate diagnostic.

For a group, read `state().common.commitLogForkStatus`. For a DM, read `state().commitLogForkStatus`. Both state calls are asynchronous.

## Protocol upgrade pause

Read `state().common.pausedForVersion` for a group or `state().pausedForVersion` for a DM when an SDK reports that a protocol upgrade blocks processing. Upgrade the SDK before you resume writes.

## File logging

File logging is available on Node, Kotlin, and Swift. Call `initLogging` first, then `enterDebugWriter` with a directory, rotation schedule, maximum file count, log level, and process type. Call `exitDebugWriter` to stop file output. Browser can send records to a log sink. Do not log encryption keys, message content, or full installation IDs.

### Multi-process logging

On Android, identify the main app process and notification-extension process with `LogProcessType.MAIN` and `LogProcessType.EXTENSION`. Separate process labels prevent both processes from writing the same log file.

## Enable debug logging

Set `LoggingOptions.level` in `initLogging` before you reproduce the problem. Set `LoggingOptions.structured` when a log collector needs JSON records. Use `setLogSink` for an app log sink.

## Backend statistics

Get the diagnostics object with `client.diagnostics` on Browser and Node, or `client.diagnostics()` on Kotlin and Swift. Its methods are asynchronous on all four SDKs.

| Task                      | Method                  |
| ------------------------- | ----------------------- |
| Format all API statistics | `aggregateStatistics()` |
| Read API counters         | `apiStatistics()`       |
| Read identity counters    | `identityStatistics()`  |
| Clear counters            | `clearStatistics()`     |

Clear counters before one action to measure only that action.

## Client version information

| Value           | Browser, Node           | Kotlin, Swift             |
| --------------- | ----------------------- | ------------------------- |
| libxmtp version | `client.libxmtpVersion` | `client.libxmtpVersion()` |
| App version     | `client.appVersion`     | `client.appVersion()`     |

The app version is absent when it was not set in backend options.

Include these versions, the backend URL, the inbox ID, installation ID, and current cursor when you collect a report. Redact private data before sharing it.
