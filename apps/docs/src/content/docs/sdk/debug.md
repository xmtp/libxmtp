---
title: Debug
---

## Forked group debugging tool

A conversation has `getDebugInformation`. You can use this to see:

- The MLS epoch of a group chat conversation for a member
- The local commit log for expert analysis
- Whether a group chat is forked

Use the conversation debug information when a group stops processing messages. A forked group is not recoverable. Start a new group after you collect the data needed to diagnose the fork.

| Field          | Browser, Node                       | Kotlin, Swift         |
| -------------- | ----------------------------------- | --------------------- |
| MLS epoch      | `epoch`                             | `epoch`               |
| Backend cursor | `cursor`                            | Not available         |
| Fork status    | `isCommitLogForked`                 | `commitLogForkStatus` |
| Commit logs    | `localCommitLog`, `remoteCommitLog` | Same names            |
| Details        | `forkDetails`                       | `forkDetails`         |

Use `conversation.debugInfo()` on Browser and Node. Use `getDebugInformation()` on Kotlin and Swift.

## Protocol upgrade pause

Check the conversation pause state when an SDK reports that a protocol upgrade blocks processing. Upgrade the SDK before you resume writes.

## File logging

File logging is available on mobile. Set the log directory, level, and maximum file size in client options. Do not log encryption keys, message content, or full installation IDs.

### Multi-process logging

On Android, identify the main app process and notification-extension process with `ProcessType.MAIN` and `ProcessType.NOTIFICATION_EXTENSION`. Separate process labels prevent both processes from writing the same log file.

## Enable debug logging

Set the JavaScript logging level or mobile log level before you reproduce the problem. Use structured logging on Browser and Node when a log collector needs JSON records.

## Backend statistics

Statistics belong to one client instance and reset when it closes.

| Task                      | Browser, Node                  | Kotlin, Swift               |
| ------------------------- | ------------------------------ | --------------------------- |
| Format all API statistics | `apiAggregateStatistics()`     | `aggregateStatistics`       |
| Read API statistic        | `apiStatistics.<name>`         | `apiStatistics.<name>`      |
| Read identity statistic   | `apiIdentityStatistics.<name>` | `identityStatistics.<name>` |
| Clear counters            | `clearAllStatistics()`         | `clearAllStatistics()`      |

Clear counters before one action to measure only that action. Clearing also releases counter memory.

## Client version information

| Value           | Browser, Node           | Kotlin, Swift           |
| --------------- | ----------------------- | ----------------------- |
| libxmtp version | `client.libxmtpVersion` | `client.libXMTPVersion` |
| App version     | `client.appVersion`     | Not available           |

Include these versions, the backend URL, the inbox ID, installation ID, and current cursor when you collect a report. Redact private data before sharing it.
