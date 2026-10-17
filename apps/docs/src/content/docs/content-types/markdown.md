---
title: Markdown
---

Markdown messages carry rich text on Browser, Node, Kotlin, and Swift.

Type ID: `xmtp.org/markdown:1.0`. The payload is UTF-8 bytes with an `encoding` parameter. It has no fallback. `shouldPush` defaults to `true`. All four SDKs include `MarkdownCodec`.

The codec transports Markdown text. Your renderer determines which syntax it supports. Treat received Markdown as untrusted input. Sanitize rendered HTML.

In the Agent SDK, use `ctx.conversation.sendMarkdown()` to send a Markdown message or `ctx.sendMarkdownReply()` to reply. The agent emits the `markdown` event.
