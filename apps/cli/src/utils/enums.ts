import type {
  ConsentState,
  ConversationKind,
  PublicIdentityKind,
} from "@xmtp/node-sdk";
export const identifierKindMap: Record<string, PublicIdentityKind> = {
  ethereum: "ethereum",
  passkey: "passkey",
};
export const consentStateMap: Record<string, ConsentState> = {
  allowed: "allowed",
  denied: "denied",
  unknown: "unknown",
};
export const conversationTypeMap: Record<string, ConversationKind> = {
  dm: "dm",
  group: "group",
};
