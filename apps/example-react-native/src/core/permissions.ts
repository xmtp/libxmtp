// Group permission presets, policy labels and member authority.

import type {
  GroupPolicyType,
  MetadataFieldKind,
  PermissionPolicy,
  PermissionPolicySet,
  PermissionUpdateKind,
} from "@xmtp/react-native-sdk";

import type { ConversationKind } from "./titles.ts";

/** The app's policy values. SDK `doesNotExist` and `other` both read as custom. */
export type GroupPolicy = "allow" | "deny" | "admin" | "superAdmin" | "custom";

export const POLICY_LABELS: Readonly<Record<GroupPolicy, string>> = {
  allow: "All members",
  deny: "Denied",
  admin: "Admins",
  superAdmin: "Super admins",
  custom: "Custom",
};

/** One `group.updatePermission(kind, policy, field)` write. */
export interface PermissionWrite {
  readonly kind: PermissionUpdateKind;
  readonly policy: GroupPolicy;
  readonly field?: MetadataFieldKind;
}

const DETAIL_FIELDS: readonly MetadataFieldKind[] = [
  "name",
  "description",
  "imageUrl",
  "appData",
];

/**
 * The nine preset writes, in order: add member, remove member, add admin,
 * remove admin, then name, description, image, app data and disappearing.
 * "Admins only" restricts adding people and details to admins.
 */
export function permissionPresetWrites(
  adminOnly: boolean,
): readonly PermissionWrite[] {
  const member: GroupPolicy = adminOnly ? "admin" : "allow";
  return [
    { kind: "addMember", policy: member },
    { kind: "removeMember", policy: "admin" },
    { kind: "addAdmin", policy: "superAdmin" },
    { kind: "removeAdmin", policy: "superAdmin" },
    ...DETAIL_FIELDS.map((field): PermissionWrite => ({
      kind: "updateMetadata",
      policy: member,
      field,
    })),
    { kind: "updateMetadata", policy: "admin", field: "disappearing" },
  ];
}

/** The SDK policy for an app policy (custom writes `other`). */
export function sdkPermissionPolicy(policy: GroupPolicy): PermissionPolicy {
  return policy === "custom" ? "other" : policy;
}

/** The app policy for an SDK policy. */
function groupPolicy(policy: PermissionPolicy): GroupPolicy {
  return policy === "doesNotExist" || policy === "other" ? "custom" : policy;
}

/** The preset radio that a group's policy type selects. */
export function presetLabel(type: GroupPolicyType): string {
  switch (type) {
    case "allMembers":
      return "All members";
    case "adminOnly":
      return "Admins only";
    case "custom":
      return "Custom";
  }
}

/** The nine current policies of a group, in preset order. */
export function policiesFromSet(
  set: PermissionPolicySet,
): readonly PermissionWrite[] {
  return [
    { kind: "addMember", policy: groupPolicy(set.addMember) },
    { kind: "removeMember", policy: groupPolicy(set.removeMember) },
    { kind: "addAdmin", policy: groupPolicy(set.addAdmin) },
    { kind: "removeAdmin", policy: groupPolicy(set.removeAdmin) },
    {
      kind: "updateMetadata",
      policy: groupPolicy(set.updateName),
      field: "name",
    },
    {
      kind: "updateMetadata",
      policy: groupPolicy(set.updateDescription),
      field: "description",
    },
    {
      kind: "updateMetadata",
      policy: groupPolicy(set.updateImage),
      field: "imageUrl",
    },
    {
      kind: "updateMetadata",
      policy: groupPolicy(set.updateAppData),
      field: "appData",
    },
    {
      kind: "updateMetadata",
      policy: groupPolicy(set.updateDisappearing),
      field: "disappearing",
    },
  ];
}

const FIELD_LABELS: Readonly<Record<MetadataFieldKind, string>> = {
  name: "Name",
  description: "Description",
  imageUrl: "Image",
  appData: "App data",
  disappearing: "Disappearing messages",
};

export const KIND_LABELS: Readonly<Record<PermissionUpdateKind, string>> = {
  addMember: "Add members",
  removeMember: "Remove members",
  addAdmin: "Add admins",
  removeAdmin: "Remove admins",
  updateMetadata: "Metadata",
};

function writeLabel(write: PermissionWrite): string {
  return write.field === undefined
    ? KIND_LABELS[write.kind]
    : FIELD_LABELS[write.field];
}

/**
 * One "Current policies" value: the policy label when every write agrees,
 * else "Label: Policy; …", or "—" when there is none.
 */
export function policyValue(writes: readonly PermissionWrite[]): string {
  const first = writes.at(0);
  if (first === undefined) return "—";
  if (writes.every((write) => write.policy === first.policy)) {
    return POLICY_LABELS[first.policy];
  }
  return writes
    .map((write) => `${writeLabel(write)}: ${POLICY_LABELS[write.policy]}`)
    .join("; ");
}

export interface PolicyRow {
  readonly title: string;
  readonly value: string;
}

/** The titles of the "Current policies" rows. */
export const POLICY_ROW_TITLES = {
  addMembers: "Add members",
  removeMembers: "Remove members",
  admins: "Add or remove admins",
  details: "Name, description, image",
  appData: "App data",
  disappearing: "Disappearing messages",
} as const;

/** The six "Current policies" rows of the Permissions screen. */
export function policyRows(
  writes: readonly PermissionWrite[],
): readonly PolicyRow[] {
  const row = (
    title: string,
    include: (write: PermissionWrite) => boolean,
  ): PolicyRow => ({ title, value: policyValue(writes.filter(include)) });
  return [
    row(POLICY_ROW_TITLES.addMembers, (write) => write.kind === "addMember"),
    row(
      POLICY_ROW_TITLES.removeMembers,
      (write) => write.kind === "removeMember",
    ),
    row(
      POLICY_ROW_TITLES.admins,
      (write) => write.kind === "addAdmin" || write.kind === "removeAdmin",
    ),
    row(
      POLICY_ROW_TITLES.details,
      (write) =>
        write.field === "name" ||
        write.field === "description" ||
        write.field === "imageUrl",
    ),
    row(POLICY_ROW_TITLES.appData, (write) => write.field === "appData"),
    row(
      POLICY_ROW_TITLES.disappearing,
      (write) => write.field === "disappearing",
    ),
  ];
}

export type GroupRole = "member" | "admin" | "superAdmin";

export const ROLE_LABELS: Readonly<Record<GroupRole, string>> = {
  member: "Member",
  admin: "Admin",
  superAdmin: "Super admin",
};

export interface GroupMember {
  readonly id: string;
  readonly role: GroupRole;
}

/** The conversation facts that member authority reads. */
export interface Authority {
  readonly kind: ConversationKind;
  readonly members: readonly GroupMember[];
  readonly policies: readonly PermissionWrite[];
  readonly pendingLeave?: boolean;
}

function canManage(
  details: Authority,
  target: GroupMember,
  own: string,
  kind: PermissionUpdateKind,
): boolean {
  if (
    details.kind !== "group" ||
    target.id === own ||
    target.role === "superAdmin"
  ) {
    return false;
  }
  if (!details.members.some((member) => member.id === target.id)) return false;
  const actor = details.members.find((member) => member.id === own);
  const policy = details.policies.find(
    (write) => write.kind === kind && write.field === undefined,
  )?.policy;
  if (actor === undefined || policy === undefined) return false;
  switch (policy) {
    case "allow":
      return true;
    case "admin":
      return actor.role === "admin" || actor.role === "superAdmin";
    case "superAdmin":
      return actor.role === "superAdmin";
    case "deny":
    case "custom":
      return false;
  }
}

/** Make admin (or Remove admin for an admin) is allowed for `own`. */
export function canChangeRole(
  details: Authority,
  target: GroupMember,
  own: string,
): boolean {
  return canManage(
    details,
    target,
    own,
    target.role === "admin" ? "removeAdmin" : "addAdmin",
  );
}

/** Remove from group is allowed for `own`. */
export function canRemoveMember(
  details: Authority,
  target: GroupMember,
  own: string,
): boolean {
  return canManage(details, target, own, "removeMember");
}

/** Only a super admin of a group changes the permission preset. */
export function canChangePermissionPreset(
  details: Authority,
  own: string,
): boolean {
  return (
    details.kind === "group" &&
    details.members.some(
      (member) => member.id === own && member.role === "superAdmin",
    )
  );
}

/** A member who is not a super admin can leave a group with other members. */
export function canLeave(details: Authority, own: string): boolean {
  return (
    details.kind === "group" &&
    details.members.length > 1 &&
    !details.members.some(
      (member) => member.id === own && member.role === "superAdmin",
    ) &&
    details.pendingLeave !== true
  );
}
