import type * as Node from "../../../../../target/sdk-generated/typescript-napi/public-values.gen";
import type * as Browser from "../../../../../target/sdk-generated/typescript-wasm/public-values.gen";

type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2
    ? true
    : false;
type Assert<T extends true> = T;
export type SameIdentity = Assert<
  Equal<Node.PublicIdentity, Browser.PublicIdentity>
>;
export type IdentityKind = Assert<
  Equal<Browser.PublicIdentity["kind"], "ethereum" | "passkey">
>;
export type SameCredential = Assert<
  Equal<
    Node.BackendOptions["credentials"],
    Browser.BackendOptions["credentials"]
  >
>;
export type SameContent = Assert<
  Equal<Node.MessageContent, Browser.MessageContent>
>;
export type ByteView = Assert<
  Equal<Browser.EncodedContent["content"], Uint8Array>
>;
export type WideInteger = Assert<
  Equal<Browser.LogRecord["droppedRecords"], bigint>
>;
export type SignerResult = Assert<
  Equal<Awaited<ReturnType<Browser.Signer["identity"]>>, Browser.PublicIdentity>
>;

export function browserDefaults(conversations: Browser.Conversations) {
  return conversations.createGroup([]);
}
export function nodeDefaults(conversations: Node.Conversations) {
  return conversations.createDm("inbox");
}
