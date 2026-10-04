---
title: Agent filters
---

Import built-in filters as `filter` or `f` from `@xmtp/agent-sdk`.

| Filter                                          | Checks                                                   |
| ----------------------------------------------- | -------------------------------------------------------- |
| `fromSelf(message, client)`                     | Sender is the current client                             |
| `hasContent(message)`                           | Decoded content is present                               |
| `isDM(conversation)`                            | Conversation is a direct message                         |
| `isGroup(conversation)`                         | Conversation is a group                                  |
| `isGroupAdminAsync(conversation, message)`      | Sender is a group admin                                  |
| `isGroupSuperAdminAsync(conversation, message)` | Sender is a group super admin                            |
| `usesCodec(message, Codec)`                     | Message uses a custom codec; narrows its TypeScript type |

```ts source="agents-filters-1.ts" region="example1"

```

Await `isGroupAdminAsync` and `isGroupSuperAdminAsync` before an access check.
The examples use the async name to make this requirement clear.

`MessageContext` also supplies type guards for Markdown, text, replies, reactions, read receipts, remote attachments, transaction references, and wallet send calls.
