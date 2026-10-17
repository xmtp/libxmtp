import type { Member } from "@xmtp/browser-sdk";

export const getMemberAddress = (member: Member) => {
  return member.identities.at(0)?.identifier ?? "";
};
