---
title: Envelope types
---

Envelope types are the top-level protocol payloads that clients publish to the backend. SDKs handle them automatically. App developers usually work with [content types](/content-types/overview/) instead.

The backend derives the wire topic from each payload. A publish request does not supply a topic. Query and subscription responses use unsigned backend envelopes over a trusted transport.

| Envelope         | Purpose                                                    | Wire topic kind |
| ---------------- | ---------------------------------------------------------- | --------------- |
| Group message    | MLS application or commit message                          | `0x00`          |
| Welcome          | Bootstrap a new group member                               | `0x01`          |
| Identity update  | Link, rotate, or revoke inbox identities and installations | `0x02`          |
| Key package      | Publish an installation's MLS credentials                  | `0x03`          |
| Commit-log entry | Detect group commit forks                                  | `0x04`          |

## Group messages

Application messages carry encrypted content such as text, attachments, reactions, and receipts. Commit messages change the group's cryptographic state, membership, metadata, or permissions.

Commits can add or remove members, update metadata, update permissions, rotate keys, or repair missing membership. A client processes commits in backend topic order.

## Welcome messages

A Welcome bootstraps a new member with group context, encrypted group secrets, the ratchet tree, and confirmation data. It depends on the new installation's key package.

## Key packages

A key package contains the public encryption key, signature key, capabilities, lifetime, and identity credential for an installation. The newest key package for an installation is the package with the highest sequence ID on its topic.

## Identity updates

Identity updates link installations and account identifiers to an inbox. They also carry rotation and revocation changes.

## Commit-log entries

A commit-log entry records a group commit for fork detection. Its topic is separate from the group-message topic, even though both use the group ID as their identifier.
