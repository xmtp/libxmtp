# @xmtp/cli

## Unreleased

- `conversation messages` returns at most the 100 newest matching messages unless you pass `--limit` (1 to 1000); a `--limit` below 1 or above 1000 fails.
- `conversations list` returns at most 100 conversations unless you pass `--limit` (at most 1000). New `--before-id` resumes after a conversation, and each row now has `createdAtNs` to pass as `--created-before`.

## 0.2.0

- Added default app version
- Updated default network to `production`

## 0.1.0

Rebuilt CLI using oclif.

## 0.0.6

### Patch Changes

- Updated dependencies [a87b56b]
  - @xmtp/agent-sdk@2.0.2

## 0.0.5

### Patch Changes

- Updated dependencies [e100655]
  - @xmtp/agent-sdk@2.0.1

## 0.0.4

### Patch Changes

- Updated dependencies [f3017b3]
  - @xmtp/agent-sdk@1.1.16

## 0.0.3

### Patch Changes

- 70952ad: Added repository details to publish CLI

## 0.0.2

### Patch Changes

- Updated dependencies [cddbb41]
  - @xmtp/agent-sdk@1.1.15
