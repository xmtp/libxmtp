import { Box, Group, NativeSelect, Stack, Text, Tooltip } from "@mantine/core";
import {
  type GroupPolicyType,
  type PermissionPolicy,
  Group as XmtpGroup,
  type Conversation,
  type PermissionPolicySet,
} from "@xmtp/browser-sdk";
import { useEffect, useMemo, useState } from "react";

const PERMISSION_VALUES = [
  { value: "allow", label: "Everyone" },
  { value: "deny", label: "Disabled" },
  { value: "admin", label: "Admin only" },
  { value: "superAdmin", label: "Super admin only" },
];

const toPermissionValue = (permission: PermissionPolicy) => permission;

export const defaultPolicySet: PermissionPolicySet = {
  addAdmin: "superAdmin",
  addMember: "allow",
  removeAdmin: "superAdmin",
  removeMember: "admin",
  updateName: "allow",
  updateDescription: "allow",
  updateImage: "allow",
  updateDisappearing: "admin",
  updateAppData: "allow",
};

export const adminPolicySet: PermissionPolicySet = {
  addAdmin: "superAdmin",
  addMember: "admin",
  removeAdmin: "superAdmin",
  removeMember: "admin",
  updateName: "admin",
  updateDescription: "admin",
  updateImage: "admin",
  updateDisappearing: "admin",
  updateAppData: "admin",
};

export const processPermissionsUpdate = async (
  conversation: Conversation,
  permissionsPolicy: GroupPolicyType,
  policySet: PermissionPolicySet,
) => {
  if (!(conversation instanceof XmtpGroup)) {
    return;
  }

  const permissions = await conversation
    .state()
    .then((state) => state.permissions);
  if (
    permissions.policyType === permissionsPolicy &&
    permissionsPolicy !== "custom"
  ) {
    return;
  }
  const next =
    permissionsPolicy === "allMembers"
      ? defaultPolicySet
      : permissionsPolicy === "adminOnly"
        ? adminPolicySet
        : policySet;
  const fields = [
    ["addMember", "addMember", undefined],
    ["removeMember", "removeMember", undefined],
    ["addAdmin", "addAdmin", undefined],
    ["removeAdmin", "removeAdmin", undefined],
    ["updateName", "updateMetadata", "name"],
    ["updateDescription", "updateMetadata", "description"],
    ["updateImage", "updateMetadata", "imageUrl"],
    ["updateDisappearing", "updateMetadata", "disappearing"],
    ["updateAppData", "updateMetadata", "appData"],
  ] as const;
  let updated = false;
  for (const [field, kind, metadataField] of fields) {
    if (next[field] === permissions.policySet[field]) continue;
    await conversation.updatePermission(kind, next[field], metadataField);
    updated = true;
  }
  if (!updated && permissions.policyType !== permissionsPolicy) {
    await conversation.updatePermission("addMember", next.addMember, undefined);
  }
};

export type PermissionsProps = {
  conversation?: Conversation;
  onPermissionsPolicyChange: (permissionsPolicy: GroupPolicyType) => void;
  onPolicySetChange: (policySet: PermissionPolicySet) => void;
};

export const Permissions: React.FC<PermissionsProps> = ({
  conversation,
  onPermissionsPolicyChange,
  onPolicySetChange,
}) => {
  const [permissionsPolicy, setPermissionsPolicy] =
    useState<GroupPolicyType>("allMembers");
  const [policySet, setPolicySet] =
    useState<PermissionPolicySet>(defaultPolicySet);

  const policyTooltip = useMemo(() => {
    if (permissionsPolicy === "allMembers") {
      return "All members of the group can perform group actions";
    } else if (permissionsPolicy === "adminOnly") {
      return "Only admins can perform group actions";
    }
    return "Custom policy as defined below";
  }, [permissionsPolicy]);

  useEffect(() => {
    onPermissionsPolicyChange(permissionsPolicy);
  }, [onPermissionsPolicyChange, permissionsPolicy]);

  useEffect(() => {
    onPolicySetChange(policySet);
  }, [onPolicySetChange, policySet]);

  useEffect(() => {
    if (!conversation || !(conversation instanceof XmtpGroup)) {
      return;
    }

    let active = true;
    const loadPermissions = async () => {
      const permissions = await conversation
        .state()
        .then((state) => state.permissions);
      if (!active) return;
      const policyType = permissions.policyType;
      switch (policyType) {
        case "allMembers":
          setPermissionsPolicy("allMembers");
          setPolicySet(defaultPolicySet);
          break;
        case "adminOnly":
          setPermissionsPolicy("adminOnly");
          setPolicySet(adminPolicySet);
          break;
        case "custom":
          setPermissionsPolicy("custom");
          setPolicySet(permissions.policySet);
          break;
      }
    };
    void loadPermissions();
    return () => {
      active = false;
    };
  }, [conversation]);

  return (
    <Box p="md">
      <Stack gap="md">
        <Group gap="md" justify="space-between" align="center">
          <Text size="sm">Policy</Text>
          <Tooltip withArrow label={<Text size="xs">{policyTooltip}</Text>}>
            <NativeSelect
              value={permissionsPolicy}
              onChange={(event) => {
                const policy = event.currentTarget.value as GroupPolicyType;
                setPermissionsPolicy(policy);
                if (policy === "allMembers") {
                  setPolicySet(defaultPolicySet);
                } else if (policy === "adminOnly") {
                  setPolicySet(adminPolicySet);
                }
              }}
              data={[
                { value: "allMembers", label: "Default" },
                { value: "adminOnly", label: "Admin only" },
                { value: "custom", label: "Custom policy" },
              ]}
            />
          </Tooltip>
        </Group>
        <Group gap="md" justify="space-between" align="center">
          <Text size="sm">Add members</Text>
          <NativeSelect
            disabled={permissionsPolicy !== "custom"}
            value={toPermissionValue(policySet.addMember)}
            onChange={(event) => {
              setPolicySet({
                ...policySet,
                addMember: event.currentTarget.value as PermissionPolicy,
              });
            }}
            data={PERMISSION_VALUES}
          />
        </Group>
        <Group gap="md" justify="space-between" align="center">
          <Text size="sm">Remove members</Text>
          <NativeSelect
            disabled={permissionsPolicy !== "custom"}
            value={toPermissionValue(policySet.removeMember)}
            onChange={(event) => {
              setPolicySet({
                ...policySet,
                removeMember: event.currentTarget.value as PermissionPolicy,
              });
            }}
            data={PERMISSION_VALUES}
          />
        </Group>
        <Group gap="md" justify="space-between" align="center">
          <Text size="sm">Add admins</Text>
          <NativeSelect
            disabled={permissionsPolicy !== "custom"}
            value={toPermissionValue(policySet.addAdmin)}
            onChange={(event) => {
              setPolicySet({
                ...policySet,
                addAdmin: event.currentTarget.value as PermissionPolicy,
              });
            }}
            data={PERMISSION_VALUES}
          />
        </Group>
        <Group gap="md" justify="space-between" align="center">
          <Text size="sm">Remove admins</Text>
          <NativeSelect
            disabled={permissionsPolicy !== "custom"}
            value={toPermissionValue(policySet.removeAdmin)}
            onChange={(event) => {
              setPolicySet({
                ...policySet,
                removeAdmin: event.currentTarget.value as PermissionPolicy,
              });
            }}
            data={PERMISSION_VALUES}
          />
        </Group>
        <Group gap="md" justify="space-between" align="center">
          <Text size="sm">Update group name</Text>
          <NativeSelect
            disabled={permissionsPolicy !== "custom"}
            value={toPermissionValue(policySet.updateName)}
            onChange={(event) => {
              setPolicySet({
                ...policySet,
                updateName: event.currentTarget.value as PermissionPolicy,
              });
            }}
            data={PERMISSION_VALUES}
          />
        </Group>
        <Group gap="md" justify="space-between" align="center">
          <Text size="sm">Update group description</Text>
          <NativeSelect
            disabled={permissionsPolicy !== "custom"}
            value={toPermissionValue(policySet.updateDescription)}
            onChange={(event) => {
              setPolicySet({
                ...policySet,
                updateDescription: event.currentTarget
                  .value as PermissionPolicy,
              });
            }}
            data={PERMISSION_VALUES}
          />
        </Group>
        <Group gap="md" justify="space-between" align="center">
          <Text size="sm">Update group image</Text>
          <NativeSelect
            disabled={permissionsPolicy !== "custom"}
            value={toPermissionValue(policySet.updateImage)}
            onChange={(event) => {
              setPolicySet({
                ...policySet,
                updateImage: event.currentTarget.value as PermissionPolicy,
              });
            }}
            data={PERMISSION_VALUES}
          />
        </Group>
        <Group gap="md" justify="space-between" align="center">
          <Text size="sm">Update app data</Text>
          <NativeSelect
            disabled={permissionsPolicy !== "custom"}
            value={toPermissionValue(policySet.updateAppData)}
            onChange={(event) => {
              setPolicySet({
                ...policySet,
                updateAppData: event.currentTarget.value as PermissionPolicy,
              });
            }}
            data={PERMISSION_VALUES}
          />
        </Group>
      </Stack>
    </Box>
  );
};
