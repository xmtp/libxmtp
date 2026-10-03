# XMTP Browser SDK

Version 8 uses one generated package. The package owns its worker and OPFS
storage. Apps use the public root. Pure codecs use the `/pure` entry.

```ts
import { Client, XmtpError, type Signer } from "@xmtp/browser-sdk";
import { initPureWasm, TextCodec } from "@xmtp/browser-sdk/pure";

await initPureWasm();
const encoded = new TextCodec().encode("hello"); // Synchronous after init.

const client = await Client.create(signer, {
  backend: { url: "https://your-backend.example", appVersion: "my-app/1" },
  storage: { location: "default", label: "production" },
});
const group = await client.conversations.createGroup([]);
await group.send(encoded, { shouldPush: true });
await client.end();
```

A signer implements async `identity`, `kind`, and `sign` methods. Identity uses
`{ identifier, kind: "ethereum" }`. An EOA kind uses `{ kind: "eoa" }`.
The sign method receives a request with `text`. It returns
`{ kind: "ecdsa", value: signatureBytes }`.

Keep the worker and WASM assets beside the package entry. Do not copy one WASM
file from a different build. Vite must preserve the package asset URLs:

```ts
export default defineConfig({
  optimizeDeps: { exclude: ["@xmtp/browser-sdk", "@xmtp/browser-sdk/pure"] },
});
```

OPFS has one owner per origin. A second tab that opens persistent storage gets
`XmtpError.StorageBusy`. Show this error to the user. Ask the user to close the
other tab, then retry. Await `client.end()` before another client takes storage.
Use `Storage.admin()` to inspect or restore storage before a client exists.
Await `admin.end()` when the operation ends.

Messages have a `content.kind` discriminator. For example, text is
`{ kind: "text", value: "hello" }`. A message returns its public client through
`message.client()`. Timestamps use `Timestamp` and nanoseconds use `.ns`.
Byte values use `Uint8Array`. Public enums use string values.

Use `client.attachments` for uploads and downloads. The SDK owns transfer state
and recovery. A completed download returns an OPFS path.

Run repository commands from the repository root:

```sh
dev/nix-shell 'just install-js'
dev/nix-shell 'just js build-browser-sdk'
dev/nix-shell 'just js test-browser-sdk-ci'
```
