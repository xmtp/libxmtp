import type { Member } from "@xmtp/browser-sdk";

export const getMemberAddress = (member: Member) => {
  return member.identities[0].identifier;
};
