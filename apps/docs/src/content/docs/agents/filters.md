---
title: Agent filters
---

Import built-in filters as `filter` or `f` from `@xmtp/agent-sdk`.

| Filter                                          | Checks                                                                  |
| ----------------------------------------------- | ----------------------------------------------------------------------- |
| `fromSelf(message, client)`                     | Sender is the current client                                            |
| `hasContent(message)`                           | Content is known, with a decoded value for custom content               |
| `isDM(conversation)`                            | Conversation is a direct message                                        |
| `isGroup(conversation)`                         | Conversation is a group                                                 |
| `isGroupAdminAsync(conversation, message)`      | Sender is a group admin                                                 |
| `isGroupSuperAdminAsync(conversation, message)` | Sender is a group super admin                                           |
| `usesCodec(message, Codec)`                     | Codec authority, type name, and major version match; content is decoded |

```ts source="agents-filters-1.ts" region="example1"

```

Await `isGroupAdminAsync` and `isGroupSuperAdminAsync` before an access check.
The examples use the async name to make this requirement clear.

`MessageContext` also supplies type guards for Markdown, text, replies, reactions, read receipts, remote attachments, transaction references, and wallet send calls.

`usesCodec` does not compare the minor version. `hasContent` accepts built-in content, including read receipts. It rejects unknown content and custom content that has no decoded `value`. After a context guard, read its decoded value from `ctx.content`.
