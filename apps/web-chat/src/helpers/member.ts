import type { Member } from "@xmtp/browser-sdk";

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

/** The handle fields that identity components take from a member. */
export const memberHandles = (
  member: Pick<Member, "handle" | "otherHandles">,
) => ({
  handle: member.handle ?? null,
  otherHandles: member.otherHandles,
});

export type MemberProfile = Member & {
  address: string;
  avatar: string | null;
  description: string | null;
  displayName: string | null;
};

export const toMemberProfile = (member: Member): MemberProfile => {
  const address = getMemberAddress(member);
  return {
    ...member,
    address,
    avatar: null,
    description: null,
    displayName: address || member.inboxId,
  };
};
