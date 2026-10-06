---
title: Actions and intents
---

Actions let a sender present choices. An intent carries the recipient's selected action.

## Actions

Type ID: `coinbase.com/actions:1.0`. The payload is JSON-encoded `Actions`. Its fallback contains the description, a numbered action list, and an instruction to reply with a number. `shouldPush` defaults to `true` on Browser, Node, Kotlin, and Swift.

The encoder requires 1 to 10 actions and unique action IDs. Each action has an `id`, a label, an optional image URL, and an optional `style`. The style is `primary`, `secondary`, or `danger`. Both the message and each action can have an expiry. The codec preserves expiry values. Your app must check expiry before it accepts a selection.

| Actions field | Meaning                              |
| ------------- | ------------------------------------ |
| `id`          | Action set ID                        |
| `description` | Text shown before the choices        |
| `actions`     | 1 to 10 action entries when encoding |
| `expiresAt`   | Optional expiry for the full set     |

| Action field | Meaning                                            |
| ------------ | -------------------------------------------------- |
| `id`         | Unique action ID                                   |
| `label`      | Display label                                      |
| `imageUrl`   | Optional image URL                                 |
| `style`      | Optional `primary`, `secondary`, or `danger` style |
| `expiresAt`  | Optional expiry for this action                    |

## Intents

Type ID: `coinbase.com/intent:1.0`. The payload is JSON-encoded `Intent` with an ID, action ID, and optional metadata. The encoder limits serialized metadata to 10 KiB. Its fallback names the selected action. `shouldPush` defaults to `true` on Browser, Node, Kotlin, and Swift.

| Intent field | Meaning                                        |
| ------------ | ---------------------------------------------- |
| `id`         | Intent ID                                      |
| `actionId`   | Selected action ID                             |
| `metadata`   | Optional JSON metadata; encode limit is 10 KiB |

The SDK `Intent` record exposes the wire `metadata` object as an optional JSON string named `metadataJson`. Action expiry fields use SDK timestamps; the codec writes RFC 3339 UTC strings with millisecond precision.

The Agent SDK receives selections with the `intent` event. The actions fallback numbers the choices so an app without actions rendering can show them.

```ts source="content-types-actions-and-intents-1.ts" region="example1"

```
