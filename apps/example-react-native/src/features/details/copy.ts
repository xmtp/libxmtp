/**
 * Details, Add member and Edit group strings (`Features/Details/
 * Details.xcstrings`, `DetailsFeatureView.swift`, `AddMemberSheet.swift`).
 */
export const detailsCopy = {
  title: "Details",
  edit: "Edit",
  directMessage: "Direct message",
  addMember: "Add member",
  disappearing: "Disappearing",
  localName: "Local name",
  nameOnThisDevice: "Name on this device",
  nameFooter:
    "Use a local name to find this conversation. Other people do not see this name.",
  notificationsOff: "Notifications are not configured in this build.",
  members: "Members",
  pending: "Pending",
  membersFooter:
    "Membership changes stay pending until the group confirms them.",
  sharedMedia: "Shared media and files",
  mediaAndFiles: "Media and files",
  groupSettings: "Group settings",
  permissions: "Permissions",
  disappearingMessages: "Disappearing messages",
  developerTools: "Developer tools",
  groupFields: "Group fields",
  myFields: "My fields",
  developerFooter: "Raw metadata fields for this conversation.",
  block: "Block conversation",
  leave: "Leave group",
  membershipPending: "Membership change pending",
  localRemovalFootnote:
    "Block removes this conversation from Chats. It does not delete its messages. Leave requests your removal from the group. You leave when the group confirms the change.",
  saveAliasFailed: "Cannot save local name. Try again.",
  off: "Off",
  retry: "Retry",
  // Edit group
  editGroup: "Edit group",
  groupName: "Group name",
  description: "Description",
  cancel: "Cancel",
  save: "Save",
  // Add member
  add: "Add",
  recipientPlaceholder: "Handle, Ethereum address, or inbox ID",
  found: "Found",
  addMemberFooter:
    "The app finds the inbox for a handle or an address before it adds the member. A handle can start with @.",
} as const;

/** `"Group · N members"`, `"@handle · Direct message"` or `"Direct message"`. */
export function detailsSubtitle(
  kind: "group" | "dm",
  memberCount: number,
  peerHandle: string | undefined,
): string {
  if (kind === "group") return `Group · ${memberCount} members`;
  return peerHandle === undefined
    ? detailsCopy.directMessage
    : `@${peerHandle} · ${detailsCopy.directMessage}`;
}

/** `"Members · N"`. */
export function membersHeader(count: number): string {
  return `${detailsCopy.members} · ${count}`;
}

/** The Disappearing messages row value: "Off" or "<seconds> s". */
export function retentionValue(retentionNs: bigint): string {
  return retentionNs === 0n
    ? detailsCopy.off
    : `${(retentionNs / 1_000_000_000n).toString()} s`;
}
