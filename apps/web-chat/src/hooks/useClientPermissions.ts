import { type PermissionLevel, type PermissionPolicy } from "@xmtp/browser-sdk";
import { useMemo } from "react";

import { useClient } from "@/contexts/XMTPContext";
import { useConversation } from "@/hooks/useConversation";

const hasPermission = (
  permissionLevel: PermissionLevel,
  policy?: PermissionPolicy,
) => {
  if (
    policy === undefined ||
    policy === "deny" ||
    policy === "other" ||
    policy === "doesNotExist"
  ) {
    return false;
  }
  if (policy === "allow") {
    return true;
  }

  switch (permissionLevel) {
    case "superAdmin":
      // super admin can do anything
      return true;
    case "admin":
      return policy === "admin";
    default:
      return false;
  }
};

export type ClientPermissions = {
  canAddMembers: boolean;
  canRemoveMembers: boolean;
  canPromoteMembers: boolean;
  canDemoteMembers: boolean;
  canChangeGroupName: boolean;
  canChangeGroupDescription: boolean;
  canChangeGroupImage: boolean;
  canChangeMessageDisappearingPolicy: boolean;
  canChangePermissionsPolicy: boolean;
};

export const useClientPermissions = (
  conversationId: string,
): ClientPermissions => {
  const { permissions, members } = useConversation(conversationId);
  const client = useClient();

  const clientPermissionLevel: PermissionLevel = useMemo(() => {
    if (client.inboxId) {
      const member = members.get(client.inboxId);
      return member?.permissionLevel ?? "member";
    }
    return "member";
  }, [members, client.inboxId]);

  return useMemo(() => {
    return {
      canAddMembers: hasPermission(
        clientPermissionLevel,
        permissions?.policySet.addMember,
      ),
      canRemoveMembers: hasPermission(
        clientPermissionLevel,
        permissions?.policySet.removeMember,
      ),
      canPromoteMembers: hasPermission(
        clientPermissionLevel,
        permissions?.policySet.addAdmin,
      ),
      canDemoteMembers: hasPermission(
        clientPermissionLevel,
        permissions?.policySet.removeAdmin,
      ),
      canChangeGroupName: hasPermission(
        clientPermissionLevel,
        permissions?.policySet.updateName,
      ),
      canChangeGroupDescription: hasPermission(
        clientPermissionLevel,
        permissions?.policySet.updateDescription,
      ),
      canChangeGroupImage: hasPermission(
        clientPermissionLevel,
        permissions?.policySet.updateImage,
      ),
      canChangeMessageDisappearingPolicy: hasPermission(
        clientPermissionLevel,
        permissions?.policySet.updateDisappearing,
      ),
      canChangePermissionsPolicy: clientPermissionLevel === "superAdmin",
    };
  }, [clientPermissionLevel, permissions]);
};
