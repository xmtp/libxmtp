---
title: Agent middleware
---

Use middleware for routing, metrics, rate limits, and other message handling. Middleware runs before content and `message` events.

## Standard middleware

Middleware can be registered with `agent.use` either one at a time or as an array. They are executed in the order they were added.

Middleware functions receive a `ctx` (context) object and a `next` function. Normally, middleware calls `next()` to hand off control to the next one in the chain. However, middleware can also alter the flow in the following ways:

| Action         | Result                                                  |
| -------------- | ------------------------------------------------------- |
| `await next()` | Continue the main chain                                 |
| `return`       | Accept this message and stop the chain without an event |
| `throw error`  | Start the error middleware chain                        |

```ts source="agents-middleware-1.ts" region="example1"

```

Register error middleware with `agent.errors.use()`.

Error middleware can be registered with `agent.errors.use` either one at a time or as an array. They are executed in the order they were added.

Error middleware receives the `error`, `ctx`, and a `next` function. Just like regular middleware, the flow in error middleware depends on how to use `next`:

| Action              | Result                                                           |
| ------------------- | ---------------------------------------------------------------- |
| `await next()`      | Mark the error handled and continue the main chain               |
| `await next(error)` | Send an error to the next error handler                          |
| `return`            | Stop the current reader without acknowledging the failed message |
| `throw error`       | Send a new error through the error chain                         |

The error context always has a `client`. Its `message` and `conversation` can be absent. Check them before use.

For a message error, `next()` accepts recovery and lets processing continue. Return without `next()` to reject acceptance and stop the reader. The failed message can be delivered again when you open a new default reader. Keep external actions safe if a message is delivered more than once.

A terminal stream error closes both readers before error middleware runs. `next()` handles that error but does not open replacement readers. Call `agent.start()` explicitly when your recovery policy permits it.
