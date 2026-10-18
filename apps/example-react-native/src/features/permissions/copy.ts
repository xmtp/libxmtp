/** Permissions strings (`Features/Permissions/Permissions.xcstrings`, Common). */
export const permissionsCopy = {
  title: "Permissions",
  header: "Who can change group settings",
  allMembers: "All members",
  allMembersDetail: "Members can update group details and add people.",
  adminsOnly: "Admins only",
  adminsOnlyDetail: "Only admins can update group details and add people.",
  custom: "Custom",
  customDetail: "Shown when the policies match no preset.",
  presetFooter:
    "A preset makes several policy changes. If one change fails, this screen shows the policies that the group has now.",
  currentPolicies: "Current policies",
  policiesFooter: "Only super admins can change permissions.",
} as const;
