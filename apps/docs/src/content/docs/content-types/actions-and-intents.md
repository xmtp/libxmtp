---
title: Actions and intents
---

Actions let a sender present choices. An intent carries the recipient's selected action.

## Actions

Type ID: `coinbase.com/actions:1.0`. The payload is JSON-encoded `Actions`. Its fallback contains the description, a numbered action list, and an instruction to reply with a number. `shouldPush` defaults to `true` on Browser and Node. Kotlin and Swift do not include this codec.

An actions message contains 1 to 10 actions. Each action has a unique `id`, a label, an optional image URL, and a `style`. The style is `primary`, `secondary`, or `danger`. Both the message and each action can have an expiry. Clients enforce expiry.

| Actions field | Meaning                          |
| ------------- | -------------------------------- |
| `id`          | Action set ID                    |
| `description` | Text shown before the choices    |
| `actions`     | 1 to 10 action entries           |
| `expiresAt`   | Optional expiry for the full set |

| Action field | Meaning                                            |
| ------------ | -------------------------------------------------- |
| `id`         | Unique action ID                                   |
| `label`      | Display label                                      |
| `imageUrl`   | Optional image URL                                 |
| `style`      | Optional `primary`, `secondary`, or `danger` style |
| `expiresAt`  | Optional expiry for this action                    |

## Intents

Type ID: `coinbase.com/intent:1.0`. The payload is JSON-encoded `Intent` with an ID, action ID, and optional metadata. Metadata is limited to 10 KiB. Its fallback names the selected action. `shouldPush` defaults to `true` on Browser and Node. Kotlin and Swift do not include this codec.

| Intent field | Meaning                                |
| ------------ | -------------------------------------- |
| `id`         | Intent ID                              |
| `actionId`   | Selected action ID                     |
| `metadata`   | Optional JSON metadata, at most 10 KiB |

The Agent SDK receives selections with the `intent` event. Its text fallback numbers the available actions so a client without actions support can still show the choices.

```ts source="content-types-actions-and-intents-1.ts" region="example1"

```
