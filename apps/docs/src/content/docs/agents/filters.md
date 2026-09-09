---
title: Agent filters
---

Import built-in filters as `filter` or `f` from `@xmtp/agent-sdk`.

| Filter                                     | Checks                                                   |
| ------------------------------------------ | -------------------------------------------------------- |
| `fromSelf(message, client)`                | Sender is the current client                             |
| `hasContent(message)`                      | Decoded content is present                               |
| `isDM(conversation)`                       | Conversation is a direct message                         |
| `isGroup(conversation)`                    | Conversation is a group                                  |
| `isGroupAdmin(conversation, message)`      | Sender is a group admin                                  |
| `isGroupSuperAdmin(conversation, message)` | Sender is a group super admin                            |
| `usesCodec(message, Codec)`                | Message uses a custom codec; narrows its TypeScript type |

```ts source="agents-filters-1.ts" region="example1"

```

`MessageContext` also supplies type guards for Markdown, text, replies, reactions, read receipts, remote attachments, transaction references, and wallet send calls.
