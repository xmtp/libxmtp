import type { GroupMember } from "@xmtp/browser-sdk";
import { getMemberAddress } from "@/helpers/xmtp";

export type Profile = {
  address: string;
  avatar: string | null;
  description: string | null;
  displayName: string | null;
};

const EMPTY_PROFILES = new Map<string, Profile[]>();

export const useAllProfiles = () => EMPTY_PROFILES;

export const combineProfiles = (
  address: string,
  _profiles: Profile[],
): Profile => ({
  address,
  avatar: null,
  description: null,
  displayName: address,
});

export type MemberProfile = GroupMember & {
  address: string;
  avatar: string | null;
  description: string | null;
  displayName: string | null;
};

export const toMemberProfile = (member: GroupMember): MemberProfile => {
  const address = getMemberAddress(member);
  return {
    ...member,
    address,
    avatar: null,
    description: null,
    displayName: address || member.inboxId,
  };
};
