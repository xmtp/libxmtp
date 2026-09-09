---
title: Agent middleware
---

Extend your agent with custom business logic using middleware. Compose cross-cutting behavior like routing, telemetry, rate limiting, analytics, and feature flags, or plug in your own.

## Standard middleware

Middleware can be registered with `agent.use` either one at a time or as an array. They are executed in the order they were added.

Middleware functions receive a `ctx` (context) object and a `next` function. Normally, middleware calls `next()` to hand off control to the next one in the chain. However, middleware can also alter the flow in the following ways:

| Action         | Result                                   |
| -------------- | ---------------------------------------- |
| `await next()` | Continue the main chain                  |
| `return`       | Stop the chain and do not emit the event |
| `throw error`  | Start the error middleware chain         |

```ts source="agents-middleware-1.ts" region="example1"

```

Register error middleware with `agent.errors.use()`.

Error middleware can be registered with `agent.errors.use` either one at a time or as an array. They are executed in the order they were added.

Error middleware receives the `error`, `ctx`, and a `next` function. Just like regular middleware, the flow in error middleware depends on how to use `next`:

| Action              | Result                                             |
| ------------------- | -------------------------------------------------- |
| `await next()`      | Mark the error handled and continue the main chain |
| `await next(error)` | Send an error to the next error handler            |
| `return`            | Stop error handling and the main chain             |
| `throw error`       | Send a new error through the error chain           |
