---
title: Markdown
---

Markdown messages support rich text on Browser and Node.

Type ID: `xmtp.org/markdown:1.0`. The payload is UTF-8 bytes with an `encoding` parameter. It has no fallback. `shouldPush` defaults to `true`. Kotlin and Swift do not include this codec.

Supported syntax includes headings, emphasis, links, ordered and unordered lists, block quotes, code, tables, and horizontal rules. Treat received Markdown as untrusted input. Sanitize rendered HTML.

The Agent SDK provides `sendMarkdown()` and emits the `markdown` event.
