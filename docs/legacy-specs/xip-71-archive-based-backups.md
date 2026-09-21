---
xip: 71
title: Archive-based backups
description: Enables users to back up their conversations, messages, and consent to an archive to be imported into another installation, if needed.
author: Naomi Plasterer (@nplasterer)
discussions-to: https://community.xmtp.org/t/xip-71-archive-based-backups/1117
status: Draft
type: Standards
category: XRC
created: 2025-07-30
---

## Abstract

This proposal introduces an **archive-based backup** mechanism for XMTP clients. The feature enables apps to create an encrypted SQLite-based archive of an installation’s conversations, messages, and consent, which can be stored in an app-designated location. The archive can then be downloaded and imported into a new installation.

In its **alpha phase**, the feature includes three core methods:

- `createArchive`
- `archiveMetadata`
- `importArchive`

When an archive-based backup is imported into an installation, **all imported conversations start off as inactive, with history visible and in read-only mode**. This is as intended by Messaging Layer Security (MLS), which ensures that there is no way to offer immediate access to active conversations. To protect the security of conversations, MLS requires that all new installations go through the process of being re-added as a member of a conversation.

As such, all conversations will be inactive until the installation is added to the imported conversations by active members of the conversations. To learn more, see  [4. Handle post-import conversation statuses](#4-handle-post-import-conversation-statuses).

## Motivation

Developers and users need reliable ways to back up and restore XMTP conversations, messages, and consent.

This XIP addresses the following problems:

- Users need an easy and durable way to back up their XMTP conversations and messages and import them onto a new device.
- Users want explicit control over which and when data is backed up.

Archive-based backups can be used as a more **deterministic and user-controlled** alternative to [history sync](https://docs.xmtp.org/inboxes/history-sync).  To take this approach, set the `historySyncUrl` client option to an empty string.

## Specification

The following methods are available in alpha:

- `createArchive(path, encryptionKey, options?)`
- `archiveMetadata(path, encryptionKey)`
- `importArchive(path, encryptionKey)`

### 1. Create the archive

To enable a user to create an archive:

1. Create the archive file that will be used as the SQLite database backup.
2. Specify the archive file path (e.g., iCloud, Google Cloud, or your server).
3. Generate a 32-byte array encryption key to protect the archive contents. This ensures that  other apps and devices cannot access the contents without the key. Securely store the key in a location that is easily accessible to the user.
4. Call `createArchive(path, encryptionKey, options?)` with the archive file path and the encryption key. Optionally, you can pass in the following:
   - Archive start and end time. If left blank, the archive will include all time.
   - Archive contents, which can be Consent or Messages. If left blank, the archive will include Consent and Messages.

    ```tsx
    createArchive(path: string, encryptionKey: string | Uint8Array, options?: {
      startTime?: Date,
      endTime?: Date,
      elements?: ("Consent" | "Messages")[]
    })
    ```

    This writes the selected content into the empty file and encrypts it using the provided key.

If the user tries to close the app before `createArchive` is complete, you can do a check to see if the file on the server is empty. If empty, display a warning to the user letting them know that exiting the app will cancel archive creation.

**Future improvements** to this feature include:

- Ability to see the progress of `createArchive` while it’s happening
- Ability to see the archive file size before proceeding
- Ability to archive only specific groups
- Ability to archive only specific content types. For example, only archive text messages.
- Ability to archive only messages without an expiration. For example, don't archive [disappearing messages](https://docs.xmtp.org/inboxes/send-messages#set-disappearing-message-settings-on-conversation-create).

### 2. Check archive metadata

To enable a user to view information about their archive(s) before importing it to an installation:

```tsx
archiveMetadata(path: string, encryptionKey: string)
```

This will return information that enables the user to better understand the archive(s) they want to import:

- Archive creation date
- Archived elements (e.g., Consent, Messages)
- Start and end time of archived data

You can get the archive file size from the file system.

### 3. Import the archive

To enable a user to import a selected archive to an installation:

```tsx
importArchive(path: string, encryptionKey: string)
```

This downloads and integrates the archive data into the installation’s local database. The archive import is **additive**, not destructive: existing messages are preserved, and duplicate messages are ignored.

A **future improvement** for this feature includes the ability to see the progress of the download and import.

In the meantime, if the user tries to close the app before `importArchive` is complete, display a warning to the user letting them know that exiting the app will cancel the archive import.

### 4. Handle post-import conversation statuses

After importing the archive to an installation, **all imported conversations will be inactive, with history visible and in read-only mode**, as intended by MLS as described in [Abstract](#abstract).

You should gray out UI functionality that involves network requests for inactive conversations.

Attempting to send or sync on inactive conversations will throw a `Group is inactive` error.

To check conversation status before initiating a network action:

```tsx
conversation.isActive()
```

This will check to see if the installation is actively in the conversation yet.

To reactivate a DM or group conversation:

- A participant, or a preexisting installation belonging to the user who ran the import, can add the new installation by sending a message to the conversation.
- For DM conversations, you may choose to programmatically create a duplicate DM for every inactive DM to trigger [stitching](https://docs.xmtp.org/inboxes/push-notifs/understand-push-notifs#dm-stitching-considerations-for-push-notifications). This will activate the DM conversations.

Inactive conversations in which participants frequently send messages may seem to activate immediately.

## Rationale

This archive-based backup system balances privacy and portability:

- Users regain access to their conversations and messages in a readable state.
- Conversations are inactive, retain MLS guarantees, and only become active after an explicit MLS add.
- The archive model gives integrators and users granular control.

## Backward compatibility

This feature is new and introduces no breaking changes.

- Archives are compatible with existing installations.
- Importing an archive into an installation that already has a local database is **additive**, not destructive and no messages will be duplicated if they share the same message ID.

## Test cases

- Archive and import to a **new installation**
  - Confirm that import works as described here.
- Archive and import to a **preexisting installation**
  - Confirm that import is additive and not destructive
  - Confirm that duplicate message IDs are ignored
- DM stitching:
  - Confirm that duplicating an imported DM triggered stitching and activated the DM conversation.
- Attempt to sync or send on an inactive conversation
  - Confirm that `Group is inactive` error is thrown

## Reference implementation

Reference implementation will be included in the alpha release of archive-based backups in XMTP protocol release 1.4.

## Security considerations

- If a [disappearing message](https://docs.xmtp.org/inboxes/send-messages#set-disappearing-message-settings-on-conversation-create) is present in the local database at the time of archive creation, it will be included in the archive. Upon import, a worker will remove these disappearing messages from the app UI based on their expiration timestamps. However, the disappearing messages will still remain in the underlying archive file. This can be addressed with this **future improvement**:
  - Ability to archive only messages without an expiration. For example, don't archive [disappearing messages](https://docs.xmtp.org/inboxes/send-messages#set-disappearing-message-settings-on-conversation-create).

### Threat model

This archive-based backup system assumes a malicious actor might attempt:

- To import an archive they did not create.
- To gain access to private conversations by circumventing MLS.

The following measures protect against this:

- Archives are encrypted with a 32-byte array key. Without the encryption key, the contents cannot be decrypted.
- Importing an archive does NOT restore network access. Conversations remain **inactive** unless the installation is added per MLS requirements.
- Even with full archive access, users can only read previously seen messages. They cannot impersonate participants or send messages in groups without membership.

This architecture maintains forward secrecy and privacy guarantees.

### Caveats

- Integrators must securely store archive files.
- Encryption keys must be protected.

## Copyright

Copyright and related rights waived via [CC0](https://creativecommons.org/publicdomain/zero/1.0/).
