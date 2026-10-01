import assert from "node:assert/strict";

import * as B from "../../../../target/sdk-conformance/typescript-napi/xmtp_sdk.ts";
import { dispose, drained, rawOptions } from "./callback-lifetime-support.mts";

// This control has no cancellation or delayed callback. It separates a live
// app wrapper from a credential source retained after native object free.
export async function checkNodeCredentialOwnership(backendURL: string) {
  for (const withGroup of [false, true]) {
    const signer = await B.generateLocalSigner();
    const options = rawOptions(backendURL);
    options.backend = B.BackendSource.Options.new({
      options: {
        url: backendURL,
        credentials: {
          credential() {
            return Promise.resolve({
              value: "Bearer ownership-control",
              expiresAtSeconds: 9_000_000_000_000_000n,
            });
          },
        },
      },
    });
    const client = await B.Client.create(signer, options);
    let group: B.GroupLike | undefined;
    try {
      if (withGroup) {
        const conversations = client.conversations();
        try {
          group = await conversations.createGroup([], undefined);
        } finally {
          dispose(conversations);
        }
      }
    } finally {
      await client.end();
      if (group) dispose(group);
      dispose(client);
      dispose(signer);
    }
    // The app still holds the JavaScript wrappers here. Every wrapper has
    // already called its generated native free function and cannot be reused.
    assert.throws(() => client.inboxId());
    if (group) assert.throws(() => group.id());
    global.gc?.();
    console.log(
      JSON.stringify({
        withGroup,
        handles: B.sdkConformanceCallbackHandleCounts(),
      }),
    );
    await drained();
    console.log(
      JSON.stringify({
        withGroup,
        released: B.sdkConformanceCallbackHandleCounts(),
      }),
    );
  }
}
