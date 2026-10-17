import { standardContentType, type ContentTypeId } from "@xmtp/node-sdk";
export const contentTypeMap: Record<string, ContentTypeId> = {
  actions: standardContentType("actions"),
  attachment: standardContentType("attachment"),
  "group-updated": standardContentType("groupUpdated"),
  intent: standardContentType("intent"),
  "leave-request": standardContentType("leaveRequest"),
  markdown: standardContentType("markdown"),
  "multi-remote-attachment": standardContentType("multiRemoteAttachment"),
  reaction: standardContentType("reaction"),
  "read-receipt": standardContentType("readReceipt"),
  "remote-attachment": standardContentType("remoteAttachment"),
  reply: standardContentType("reply"),
  text: standardContentType("text"),
  "transaction-reference": standardContentType("transactionReference"),
  "wallet-send-calls": standardContentType("walletSendCalls"),
  "group-membership-change": {
    authorityId: "xmtp.org",
    typeId: "group_membership_change",
    versionMajor: 1,
    versionMinor: 0,
  },
};
export const contentTypeOptions = Object.keys(contentTypeMap) as [
  string,
  ...string[],
];
