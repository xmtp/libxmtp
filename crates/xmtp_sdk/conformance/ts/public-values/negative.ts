import type * as Public from "../../../../../target/sdk-generated/typescript-wasm/public-values.gen";

declare const identity: Public.PublicIdentity;
identity.identifier = "changed";
identity.kind = 0;
declare const content: Public.MessageContent;
content.tag;
content.inner;
const backend: Public.BackendOptions = {
  url: "https://example.test",
  credential: { value: "token", expiresAtSeconds: 1n },
};
const bytes: Public.EncodedContent["content"] = new ArrayBuffer(2);
void backend;
void bytes;
