---
title: Stream group updates with XMTP
---

Stream group metadata changes (name, description, members) in real-time using the `group_updated` native content type.

## Structure

Type ID: `xmtp.org/group_updated:1.0`. The payload is a `GroupUpdated` protobuf. It has no fallback text. The codec sets `shouldPush` to `false`, but the locally stored transcript message has `shouldPush: true` on all four platforms. The message is not published, so it never reaches a push server.

| Field                      | Meaning                               |
| -------------------------- | ------------------------------------- |
| `initiatedByInboxId`       | Inbox that made the commit            |
| `addedInboxes`             | Inboxes added                         |
| `removedInboxes`           | Inboxes removed by another member     |
| `leftInboxes`              | Inboxes removed after a leave request |
| `metadataFieldChanges`     | Mutable metadata changes              |
| `addedAdminInboxes`        | Inboxes made admin                    |
| `removedAdminInboxes`      | Inboxes no longer admin               |
| `addedSuperAdminInboxes`   | Inboxes made super admin              |
| `removedSuperAdminInboxes` | Inboxes no longer super admin         |

One message can contain several changes. `metadataFieldChanges` can identify the group name, description, group image URL, disappearing-message settings, minimum protocol version, app data, or commit-log signer.

`Inbox`

| Field     | Type   |
| --------- | ------ |
| `inboxId` | string |

`MetadataFieldChange`

| Field       | Type   | Present                            |
| ----------- | ------ | ---------------------------------- |
| `fieldName` | string | Always                             |
| `oldValue`  | string | Absent when the field had no value |
| `newValue`  | string | Absent when the field was cleared  |

`fieldName` is one of:

| Value                                | Field                           |
| ------------------------------------ | ------------------------------- |
| `group_name`                         | Group name                      |
| `description`                        | Group description               |
| `group_image_url_square`             | Group image URL                 |
| `message_disappear_from_ns`          | Disappearing messages, start    |
| `message_disappear_in_ns`            | Disappearing messages, duration |
| `minimum_supported_protocol_version` | Minimum protocol version        |
| `app_data`                           | Application data                |
| `_commit_log_signer`                 | Commit-log signer               |

## When this message appears

A `group_updated` message is not sent. Each client writes it into its own database when it processes a commit. The message you see is produced locally, by you, from the commit you just applied.

Your own client writes one when it reads its commit back. Other installations create their own copies. A newly added member joins through a welcome and receives a synthetic update that names only its own inbox. A key update with no membership or metadata change produces no group-update message.
